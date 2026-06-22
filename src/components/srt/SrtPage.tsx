import { useState, useEffect } from "react";
import {
  FolderOpen,
  RefreshCw,
  FileText,
  BookOpen,
  Layers,
  MessageSquare,
  Settings,
} from "lucide-react";
import { exists } from "@tauri-apps/plugin-fs";
import { useSrtStore } from "../../stores/useSrtStore";
import { useDictionaryStore } from "../../stores/useDictionaryStore";
import { useAppLogStore } from "../../stores/useAppLogStore";
import { useProjectStore } from "../../stores/useProjectStore";
import { appendToGlossary } from "../../lib/dictionarySync";
import {
  listSrtInDir,
  parseSrtFile,
  generateSrtSynopsis,
  detectSrtScenes,
  analyzeSceneContext,
  saveSrtAnalysis,
  loadSrtAnalyses,
  resolveSynopsisKatakana,
  resolveUnresolvedTermAiOpenai,
  resolveUnresolvedTermsBatchOpenai,
  extractSrtBodyCandidates,
  loadCharacterDictionary,
  loadGlossaryDictionary,
  loadDramaInfo,
} from "../../lib/tauri";
import type { SrtFileState } from "../../stores/useSrtStore";
import type { SubtitleEntry, KatakanaKanjiMap, TermVariantEntry, UnresolvedTerm, GlossaryEntry, BatchTermRequest } from "../../types";

/** Normalize an English name for dictionary matching: lowercase, strip spaces/hyphens/apostrophes/punctuation */
function normalizeEn(text: string): string {
  return text.toLowerCase().replace(/[^a-z0-9]/g, "");
}

/** Filter out unresolved_terms that already match dictionary entries (characters or glossary). */
function filterUnresolvedByDict(
  terms: UnresolvedTerm[],
  characters: ReturnType<typeof useDictionaryStore.getState>["characters"],
  glossary: ReturnType<typeof useDictionaryStore.getState>["glossary"],
): { filtered: UnresolvedTerm[]; removedCount: number } {
  const dictEnKeys = new Set<string>();
  const dictJaKeys = new Set<string>();
  for (const c of characters) {
    if (c.english_name) dictEnKeys.add(normalizeEn(c.english_name));
    if (c.chinese_name) dictEnKeys.add(normalizeEn(c.chinese_name));
    if (c.japanese_name) dictJaKeys.add(c.japanese_name);
    for (const alias of c.aliases) {
      if (alias) dictEnKeys.add(normalizeEn(alias));
    }
  }
  for (const g of glossary) {
    if (g.source) dictEnKeys.add(normalizeEn(g.source));
    if (g.target) dictJaKeys.add(g.target);
  }

  // Diagnostic logging — always log to diagnose filtering issues
  console.log("[SRT] filterUnresolvedByDict: chars=", characters.length, "glossary=", glossary.length, "enKeys=", dictEnKeys.size, "jaKeys=", dictJaKeys.size, "rawTerms=", terms.length);
  if (dictEnKeys.size === 0 && dictJaKeys.size === 0 && terms.length > 0) {
    console.warn("[SRT] filterUnresolvedByDict: dictionary keys are EMPTY — filter will remove nothing!");
    useAppLogStore.getState().addLog("warn", "SRT", `[WARN] dictionary empty; unresolved_terms cannot be filtered (chars=${characters.length}, glossary=${glossary.length}, rawTerms=${terms.length})`);
  }

  const filtered = terms.filter((t) => {
    const normSource = normalizeEn(t.source_text);
    if (dictEnKeys.has(normSource)) {
      console.log(`[SRT] filtered by enKey match: "${t.source_text}" → normSource="${normSource}"`);
      return false;
    }
    if (t.surface_ja && dictJaKeys.has(t.surface_ja)) {
      console.log(`[SRT] filtered by jaKey exact: "${t.source_text}" → surface_ja="${t.surface_ja}"`);
      return false;
    }
    if (t.surface_ja) {
      for (const ja of dictJaKeys) {
        if (ja.includes(t.surface_ja) || t.surface_ja.includes(ja)) {
          console.log(`[SRT] filtered by jaKey substring: "${t.source_text}" → surface_ja="${t.surface_ja}" vs dict="${ja}"`);
          return false;
        }
      }
    }
    return true;
  });

  const removedCount = terms.length - filtered.length;
  if (removedCount > 0) {
    console.log(`[SRT] filterUnresolvedByDict: removed ${removedCount}/${terms.length} terms`);
  } else if (terms.length > 0) {
    console.warn(`[SRT] filterUnresolvedByDict: 0/${terms.length} terms filtered — dictionary keys may not match`);
  }

  return { filtered, removedCount };
}

/** Normalize English text for dedup (same as normalizeEn, kept separate for semantics). */
function normalizeForDedup(text: string): string {
  return text.toLowerCase().replace(/[^a-z0-9]/g, "");
}

/** Merge unresolved terms from synopsis LLM and SRT body extraction.
 *  Deduplicates by normalized key. Terms found in both sources get source="srt_body+synopsis".
 *  Sorted: both-source first, then by occurrence_count desc. */
function mergeUnresolvedTerms(
  synopsisTerms: UnresolvedTerm[],
  bodyTerms: UnresolvedTerm[],
): UnresolvedTerm[] {
  const map = new Map<string, UnresolvedTerm>();

  // First pass: body terms (higher occurrence_count, lower priority for surface_ja)
  for (const t of bodyTerms) {
    const key = normalizeForDedup(t.source_text);
    map.set(key, { ...t });
  }

  // Second pass: synopsis terms override body when both exist
  for (const t of synopsisTerms) {
    const key = normalizeForDedup(t.source_text);
    if (map.has(key)) {
      const existing = map.get(key)!;
      map.set(key, {
        ...existing,
        source: "srt_body+synopsis",
        surface_ja: t.surface_ja || existing.surface_ja,
        reason: `${existing.reason}; あらすじでも検出`,
        occurrence_count: existing.occurrence_count,
      });
    } else {
      map.set(key, { ...t });
    }
  }

  // Sort: "both" source first, then by occurrence_count desc
  return Array.from(map.values()).sort((a, b) => {
    const aBoth = a.source === "srt_body+synopsis" ? 0 : 1;
    const bBoth = b.source === "srt_body+synopsis" ? 0 : 1;
    if (aBoth !== bBoth) return aBoth - bBoth;
    return (b.occurrence_count ?? 0) - (a.occurrence_count ?? 0);
  });
}

/** Export unresolved terms as CSV. Includes all terms (not just pending). */
function exportUnresolvedTermsCsv(terms: UnresolvedTerm[], filename: string) {
  const BOM = "﻿";
  const headers = [
    "source_text", "surface_ja", "term_type", "status", "source", "occurrence_count",
    "reason", "web_candidate_zh", "web_status", "web_confidence",
    "web_evidence_summary", "web_evidence_urls", "adopted",
  ];
  const rows = terms.map((t) => [
    t.source_text,
    t.surface_ja,
    t.term_type,
    t.status,
    t.source ?? "",
    t.occurrence_count ?? 0,
    t.reason,
    t.webResult?.candidate_zh ?? "",
    t.webResult?.status ?? "",
    t.webResult?.confidence ?? "",
    t.webResult?.evidence_summary ?? "",
    (t.webResult?.evidence_urls ?? []).join(" | "),
    t.adopted ?? false,
  ]);
  const escape = (v: string | number | boolean) => {
    const s = String(v);
    if (s.includes(",") || s.includes('"') || s.includes("\n")) {
      return `"${s.replace(/"/g, '""')}"`;
    }
    return s;
  };
  const csv = BOM + [headers, ...rows].map((r) => r.map(escape).join(",")).join("\n");
  const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename.replace(/\.srt$/i, "") + "_unresolved.csv";
  a.click();
  URL.revokeObjectURL(url);
}

/** Split a path into segments, handling both / and \ separators.
 *  For UNC paths (\\server\share\...) preserves the leading empty segments
 *  so the reconstruction produces a valid UNC path. */
function splitPath(p: string): string[] {
  const parts = p.split(/[/\\]/);
  // UNC: \\ → split gives ['', '', 'server', ...] — keep empties for reconstruction
  if (p.startsWith("\\\\")) return parts;
  return parts.filter(Boolean);
}

/** Join path segments back with the separator found in the original path. */
function joinPath(segments: string[], sep: string): string {
  const joined = segments.join(sep);
  // UNC: segments = ['', '', 'server', ...] → join already produces \\server\...
  if (sep === "\\" && segments.length >= 2 && segments[0] === "" && segments[1] === "") {
    return joined;
  }
  return joined;
}

/**
 * Derive project base_dir from an SRT folder path by walking up
 * to find a parent that contains a `dictionaries/` directory.
 * At each level checks both `candidate/dictionaries/` and `candidate/works/dictionaries/`.
 * Returns the found base_dir (the directory whose `dictionaries/` subfolder was found) or null.
 */
async function deriveBaseDir(srtFolderPath: string): Promise<string | null> {
  const log = useAppLogStore.getState().addLog;
  const sep = srtFolderPath.includes("\\") ? "\\" : "/";
  const segments = splitPath(srtFolderPath);
  const isTauri = typeof window !== "undefined" && (window as any).__TAURI_INTERNALS__;
  if (!isTauri) return null;

  // Try the selected folder itself and up to 3 parents
  for (let depth = 0; depth <= 3; depth++) {
    const candidateSegments = segments.slice(0, segments.length - depth);
    if (candidateSegments.length === 0) break;
    const candidate = joinPath(candidateSegments, sep);

    // 1. candidate/dictionaries/{characters,glossary}.json
    const dictChar = `${candidate}${sep}dictionaries${sep}characters.json`;
    const dictGloss = `${candidate}${sep}dictionaries${sep}glossary.json`;
    log("info", "SRT", `check dictionary candidate: ${candidate}${sep}dictionaries/`);
    try {
      if (await exists(dictChar)) {
        log("info", "SRT", `辞書ファイル検出: ${dictChar}`);
        return candidate;
      }
      if (await exists(dictGloss)) {
        log("info", "SRT", `辞書ファイル検出: ${dictGloss}`);
        return candidate;
      }
    } catch (e) {
      log("debug", "SRT", `exists() failed for ${candidate}${sep}dictionaries/: ${String(e).slice(0, 120)}`);
    }

    // 2. candidate/works/dictionaries/{characters,glossary}.json
    const worksDictChar = `${candidate}${sep}works${sep}dictionaries${sep}characters.json`;
    const worksDictGloss = `${candidate}${sep}works${sep}dictionaries${sep}glossary.json`;
    log("info", "SRT", `check dictionary candidate: ${candidate}${sep}works${sep}dictionaries/`);
    try {
      if (await exists(worksDictChar)) {
        log("info", "SRT", `辞書ファイル検出: ${worksDictChar}`);
        return `${candidate}${sep}works`;
      }
      if (await exists(worksDictGloss)) {
        log("info", "SRT", `辞書ファイル検出: ${worksDictGloss}`);
        return `${candidate}${sep}works`;
      }
    } catch (e) {
      log("debug", "SRT", `exists() failed for ${candidate}${sep}works${sep}dictionaries/: ${String(e).slice(0, 120)}`);
    }
  }

  log("warn", "SRT", `辞書フォルダ not found within 3 levels of ${srtFolderPath}`);
  return null;
}

/** Load character + glossary dictionaries from a base_dir, updating the store and UI status. */
async function loadDictionariesFromDir(
  baseDir: string,
  setCharacters: ReturnType<typeof useDictionaryStore.getState>["setCharacters"],
  setGlossary: ReturnType<typeof useDictionaryStore.getState>["setGlossary"],
  setProject: ReturnType<typeof useProjectStore.getState>["setProject"],
  setDictStatus: (s: string) => void,
): Promise<{ chars: number; gloss: number }> {
  const log = useAppLogStore.getState().addLog;
  const sep = baseDir.includes("\\") ? "\\" : "/";
  const dirName = splitPath(baseDir).pop() || baseDir;

  setProject(dirName, baseDir);
  log("info", "SRT", `project base_dir set: ${baseDir}`);

  const charPath = `${baseDir}${sep}dictionaries${sep}characters.json`;
  const glossPath = `${baseDir}${sep}dictionaries${sep}glossary.json`;
  log("info", "SRT", `辞書 load from: ${baseDir}${sep}dictionaries`);

  let charsCount = 0;
  let glossCount = 0;

  try {
    const chars = await loadCharacterDictionary(charPath);
    setCharacters(chars, charPath);
    charsCount = chars.length;
    log("success", "SRT", `characters.json loaded: ${chars.length}件`);
  } catch (e) {
    log("warn", "SRT", `characters.json load failed: ${charPath} — ${String(e)}`);
  }

  try {
    const entries = await loadGlossaryDictionary(glossPath);
    setGlossary(entries, glossPath);
    glossCount = entries.length;
    log("success", "SRT", `glossary.json loaded: ${entries.length}件`);
  } catch (e) {
    log("warn", "SRT", `glossary.json load failed: ${glossPath} — ${String(e)}`);
  }

  if (charsCount === 0 && glossCount === 0) {
    setDictStatus("ファイルなし");
  } else {
    setDictStatus(`OK (人物${charsCount}人, 用語${glossCount}件)`);
  }

  return { chars: charsCount, gloss: glossCount };
}

/** Build a text block from dictionary store data to pass to LLM commands */
function buildPromptContext(
  characters: ReturnType<typeof useDictionaryStore.getState>["characters"],
  glossary: ReturnType<typeof useDictionaryStore.getState>["glossary"],
  katakanaMap?: KatakanaKanjiMap[],
  termVariants?: TermVariantEntry[],
  unresolvedTerms?: UnresolvedTerm[],
  adoptedTerms?: GlossaryEntry[],
): string {
  const lines: string[] = [];

  if (characters.length > 0) {
    lines.push("【登場人物名対応表】");
    lines.push("英語 : 中文 : 日本語");
    lines.push("");
    for (const c of characters) {
      const en = c.english_name || "";
      const cn = c.chinese_name || "";
      const jp = c.japanese_name || "";
      if (en || cn || jp) {
        lines.push(`${en} : ${cn} : ${jp}`);
      }
    }
  }

  if (glossary.length > 0) {
    lines.push("");
    lines.push("【用語表】");
    lines.push("英語 : 中文 : 日本語");
    lines.push("");
    for (const g of glossary) {
      lines.push(`${g.source} :  : ${g.target}`);
    }
  }

  if (katakanaMap && katakanaMap.length > 0) {
    const resolved = katakanaMap.filter((m) => m.status === "resolved" && m.kanji);
    if (resolved.length > 0) {
      lines.push("");
      lines.push("【カタカナ→漢字補正表（確定済）】");
      lines.push("カタカナ : 漢字");
      lines.push("");
      for (const m of resolved) {
        lines.push(`${m.katakana} : ${m.kanji}`);
      }
    }
  }

  if (adoptedTerms && adoptedTerms.length > 0) {
    lines.push("");
    lines.push("【追加採用用語（AI確認済）】");
    lines.push("以下はAI確認で根拠つき表記を確認し、ユーザーが採用した固有名詞です。翻訳時にこの表記を使ってください。");
    lines.push("英語 : 中文 : 日本語");
    lines.push("");
    for (const a of adoptedTerms) {
      lines.push(`${a.source} :  : ${a.target}`);
    }
  }

  if (unresolvedTerms && unresolvedTerms.length > 0) {
    const pending = unresolvedTerms.filter((t) => !t.adopted);
    if (pending.length > 0) {
      lines.push("");
      lines.push("【未確定の固有名詞候補】");
      lines.push("以下は英語SRT中に出現したが、用語表に未登録の固有名詞候補です。根拠ある日本語漢字表記が未確認のため、無理に漢字化しないでください。");
      lines.push("");
      for (const t of pending) {
        lines.push(`${t.source_text} : ${t.surface_ja || "未確認"} : 未確定`);
      }
    }
  } else if (katakanaMap && katakanaMap.length > 0) {
    const unresolved = katakanaMap.filter((m) => m.status !== "resolved");
    if (unresolved.length > 0) {
      lines.push("");
      lines.push("【未確定の固有名詞候補】");
      lines.push("以下は用語表にないカタカナ固有名詞です。根拠ある漢字表記が未確認のため、無理に漢字化しないでください。");
      lines.push("");
      for (const m of unresolved) {
        lines.push(`${m.katakana}：未確定`);
      }
    }
  }

  if (termVariants && termVariants.length > 0) {
    const resolved = termVariants.filter((v) => v.status === "resolved" && v.canonical);
    if (resolved.length > 0) {
      lines.push("");
      lines.push("【表記揺れ補正（確定済）】");
      lines.push("異表記 : 統一表記");
      lines.push("");
      for (const v of resolved) {
        lines.push(`${v.variants.join(", ")} : ${v.canonical}`);
      }
    }
  }

  return lines.join("\n");
}

/** Build the assembled translation prompt for a single scene (2.4) */
function buildSceneTranslationPrompt(
  characters: ReturnType<typeof useDictionaryStore.getState>["characters"],
  glossary: ReturnType<typeof useDictionaryStore.getState>["glossary"],
  sceneContextJa: string,
  sceneEntries: SubtitleEntry[],
  katakanaMap?: KatakanaKanjiMap[],
  termVariants?: TermVariantEntry[],
  unresolvedTerms?: UnresolvedTerm[],
  adoptedTerms?: GlossaryEntry[],
): string {
  const lines: string[] = [];

  // Base translation instructions (same preamble as buildPromptText)
  lines.push("あなたはドラマ字幕を日本語に翻訳する翻訳者です。");
  lines.push("");
  lines.push("以下の字幕を、自然な日本語字幕に翻訳してください。");
  lines.push("");
  lines.push("翻訳では、まず用語表を最優先してください。");
  lines.push(
    "用語表にある語が字幕中に出た場合、英語表記・中国語表記・表記揺れのいずれで出ても、" +
    "必ず用語表の「日本語」欄の表記に統一してください。",
  );
  lines.push("");
  lines.push("出力ルール：");
  lines.push("");
  lines.push("* 原文にない説明を追加しない。");
  lines.push("* 字幕として短く自然な日本語にする。");
  lines.push("* 直訳しすぎず、ただし意味を変えない。");
  lines.push("* 人名・地名・勢力名・役名は用語表の日本語表記を使う。");
  lines.push("* 用語表に漢字表記がある語は、必ず漢字のみで出力する。「カタカナ（漢字）」のような併記は禁止。");
  lines.push("* 例：楚喬（チュウ・チャオとは書かない）、諸葛玥（ヅーグー・ユエとは書かない）");
  lines.push("* 同じ人物・地名・勢力名の表記を途中で変えない。");
  lines.push("* 身分語・役職語は現代語に寄せすぎず、時代劇調の自然な日本語にする。");
  lines.push(
    "* 英語字幕から翻訳する場合も、中国語字幕から翻訳する場合も、" +
    "用語表の「日本語」欄を出力表記として使う。",
  );
  lines.push("* 出力は翻訳文のみとし、解説や注釈は付けない。");
  lines.push("");
  lines.push(
    "以下の対応表では、「英語」は英語字幕に出る可能性のある表記、" +
    "「中文」は中国語字幕に出る可能性のある表記、" +
    "「日本語」は日本語字幕で出力すべき表記です。" +
    "対応表にある語は、日本語欄の表記に統一してください。",
  );

  // Section: この場面の状況設定
  if (sceneContextJa) {
    lines.push("");
    lines.push("【この場面の状況設定】");
    lines.push("");
    lines.push(sceneContextJa);
  }

  // Section: 登場人物名
  if (characters.length > 0) {
    const charRows = characters
      .filter((c) => c.english_name && c.japanese_name)
      .map((c) => ({
        en: c.english_name,
        cn: c.chinese_name || "",
        jp: c.japanese_name,
      }));
    if (charRows.length > 0) {
      lines.push("");
      lines.push("【登場人物名】");
      lines.push("英語 : 中文 : 日本語");
      lines.push("");
      for (const r of charRows) {
        lines.push(`${r.en} : ${r.cn} : ${r.jp}`);
      }
    }
  }

  // Section: 固有名詞対応表（用語表）
  if (glossary.length > 0) {
    lines.push("");
    lines.push("【固有名詞対応表】");
    lines.push("英語 : 中文 : 日本語");
    lines.push("");
    for (const g of glossary) {
      lines.push(`${g.source} :  : ${g.target}`);
    }
  }

  // Section: カタカナ→漢字補正表（確定済のみ）
  if (katakanaMap && katakanaMap.length > 0) {
    const resolved = katakanaMap.filter((m) => m.status === "resolved" && m.kanji);
    if (resolved.length > 0) {
      lines.push("");
      lines.push("【カタカナ→漢字補正表（確定済）】");
      lines.push("カタカナ : 漢字");
      lines.push("");
      for (const m of resolved) {
        lines.push(`${m.katakana} : ${m.kanji}`);
      }
    }
  }

  // Section: 追加採用用語（AI確認済）
  if (adoptedTerms && adoptedTerms.length > 0) {
    lines.push("");
    lines.push("【追加採用用語（AI確認済）】");
    lines.push("以下はAI確認で根拠つき表記を確認し、ユーザーが採用した固有名詞です。翻訳時にこの表記を使ってください。");
    lines.push("英語 : 中文 : 日本語");
    lines.push("");
    for (const a of adoptedTerms) {
      lines.push(`${a.source} :  : ${a.target}`);
    }
  }

  // Section: 未確定の固有名詞候補（LLM unresolved_terms 優先、katakanaMap fallback）
  if (unresolvedTerms && unresolvedTerms.length > 0) {
    const pending = unresolvedTerms.filter((t) => !t.adopted);
    if (pending.length > 0) {
      lines.push("");
      lines.push("【未確定の固有名詞候補】");
      lines.push("以下は英語SRT中に出現したが、用語表に未登録の固有名詞候補です。根拠ある日本語漢字表記が未確認のため、無理に漢字化しないでください。");
      lines.push("");
      for (const t of pending) {
        lines.push(`${t.source_text} : ${t.surface_ja || "未確認"} : 未確定`);
      }
    }
  } else if (katakanaMap && katakanaMap.length > 0) {
    const unresolved = katakanaMap.filter((m) => m.status !== "resolved");
    if (unresolved.length > 0) {
      lines.push("");
      lines.push("【未確定の固有名詞候補】");
      lines.push("以下は用語表にないカタカナ固有名詞です。根拠ある漢字表記が未確認のため、無理に漢字化しないでください。");
      lines.push("");
      for (const m of unresolved) {
        lines.push(`${m.katakana}：未確定`);
      }
    }
  }

  // Section: 表記揺れ候補
  if (termVariants && termVariants.length > 0) {
    const unresolved = termVariants.filter((v) => v.status === "needs_review");
    if (unresolved.length > 0) {
      lines.push("");
      lines.push("【未確定の表記揺れ候補】");
      lines.push("以下は同一語の可能性がありますが、未確認です。翻訳時に表記を統一する必要がある場合は文脈を確認してください。");
      lines.push("");
      for (const v of unresolved) {
        lines.push(v.variants.join(" / "));
      }
    }
  }

  // Section: 翻訳対象SRT
  lines.push("");
  lines.push("【翻訳対象SRT】");
  lines.push("");
  for (const entry of sceneEntries) {
    lines.push(`${entry.index}`);
    lines.push(`${entry.start} --> ${entry.end}`);
    lines.push(entry.text);
    lines.push("");
  }

  return lines.join("\n");
}

export default function SrtPage() {
  const {
    folderPath,
    files,
    setFolder,
    setFileEntries,
    setFileStatus,
    setFileSynopsis,
    setFileSceneDetection,
    setFileSceneContext,
    setTermWebResult,
    adoptTerm,
    removeUnresolvedTerm,
    setTermLoading,
    setFileAdoptedTerms,
  } = useSrtStore();
  // batchTermLoading and setBatchTermResults are accessed via useSrtStore.getState() in the handler
  const characters = useDictionaryStore((s) => s.characters);
  const glossary = useDictionaryStore((s) => s.glossary);
  const setCharacters = useDictionaryStore((s) => s.setCharacters);
  const setGlossary = useDictionaryStore((s) => s.setGlossary);
  const setProject = useProjectStore((s) => s.setProject);
  const projectBaseDir = useProjectStore((s) => s.baseDir);
  const [dictStatus, setDictStatus] = useState<string>("未確認");
  const [dramaTitleZh, setDramaTitleZh] = useState<string>("");
  const [dramaTitleEn, setDramaTitleEn] = useState<string>("");
  const [dramaTitleJa, setDramaTitleJa] = useState<string>("");
  const [shortContext, setShortContext] = useState<string>("");

  // Auto-load dictionaries when projectBaseDir becomes available (Tauri only)
  useEffect(() => {
    if (typeof window !== "undefined" && !(window as any).__TAURI_INTERNALS__) return;
    if (!projectBaseDir) return;
    const dictStore = useDictionaryStore.getState();
    if (dictStore.characters.length > 0 || dictStore.glossary.length > 0) {
      setDictStatus(`OK (人物${dictStore.characters.length}人, 用語${dictStore.glossary.length}件)`);
      return;
    }
    loadDictionariesFromDir(projectBaseDir, setCharacters, setGlossary, setProject, setDictStatus);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectBaseDir]);

  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState(0);

  // Loading states for analysis buttons
  const [loadingSynop, setLoadingSynop] = useState(false);
  const [loadingScene, setLoadingScene] = useState(false);
  const [loadingContext, setLoadingContext] = useState(false);

  // Safe drama title fallback: metadata → search titles → folder name → empty
  const resolveDramaTitle = (): string => {
    if (dramaTitleJa) return dramaTitleJa;
    if (dramaTitleZh) return dramaTitleZh;
    if (dramaTitleEn) return dramaTitleEn;
    if (activeFile) {
      const parts = activeFile.path.replace(/\\/g, "/").split("/");
      if (parts.length >= 2) return parts[parts.length - 2];
    }
    return "";
  };

  // --- AI term resolution handler ---
  const handleAiConfirm = async (term: UnresolvedTerm) => {
    if (!activeFile) return;
    setTermLoading(activeFile.path, term.source_text, true);
    try {
      const result = await resolveUnresolvedTermAiOpenai({
        source_text: term.source_text,
        surface_ja: term.surface_ja,
        drama_title: resolveDramaTitle() || undefined,
        prompt_context: promptContext,
        srt_filename: activeFile.name,
      });
      setTermWebResult(activeFile.path, term.source_text, result);
      console.log(`[SRT] AI確認: ${term.source_text} → ${result.candidate_zh ?? "なし"} (${result.confidence})`);
    } catch (e) {
      const msg = String(e);
      console.error(`[SRT] AI確認エラー: ${term.source_text}`, e);
      setError(`AI確認エラー(${term.source_text}): ${msg}`);
      setTermWebResult(activeFile.path, term.source_text, {
        source_text: term.source_text,
        surface_ja: term.surface_ja,
        candidate_zh: null,
        candidate_ja: null,
        confidence: "none" as const,
        evidence_summary: msg,
        evidence_urls: [],
        status: "error" as const,
      });
    } finally {
      setTermLoading(activeFile.path, term.source_text, false);
    }
  };

  // --- Batch AI確認 handler ---
  const handleBatchAiConfirm = async () => {
    if (!activeFile || !activeFile.synopsis) return;
    const pending = activeFile.unresolvedTerms.filter((t) => !t.adopted && !t.webResult);
    if (pending.length === 0) return;
    const batchTerms: BatchTermRequest[] = pending.map((t) => ({
      source_text: t.source_text,
      surface_ja: t.surface_ja,
    }));
    const store = useSrtStore.getState();
    store.setBatchTermLoading(activeFile.path, true);
    try {
      const results = await resolveUnresolvedTermsBatchOpenai({
        terms: batchTerms,
        drama_title_ja: resolveDramaTitle(),
        drama_title_zh: dramaTitleZh || null,
        drama_title_en: dramaTitleEn || null,
        short_context: shortContext || null,
        srt_filename: activeFile.name || null,
      });
      store.setBatchTermResults(activeFile.path, results);
      console.log(`[SRT] 一括AI確認: ${results.length}件解決`);
    } catch (e) {
      const msg = String(e);
      console.error("[SRT] 一括AI確認エラー:", e);
      setError(`一括AI確認エラー: ${msg}`);
      store.setBatchTermLoading(activeFile.path, false);
    }
  };

  const handleAdoptTerm = (term: UnresolvedTerm) => {
    if (!activeFile) return;
    adoptTerm(activeFile.path, term.source_text);
    // Also persist to shared glossary.json
    if (projectBaseDir) {
      const resolvedTarget = term.webResult?.candidate_zh ?? term.webResult?.candidate_ja ?? term.surface_ja;
      const aliases = term.webResult?.alternatives?.length
        ? term.webResult.alternatives.filter((a) => a !== resolvedTarget)
        : undefined;
      const entry: GlossaryEntry = {
        source: term.source_text,
        target: resolvedTarget,
        type: "proper_noun",
        notes: "AI確認採用",
        aliases,
      };
      appendToGlossary(projectBaseDir, [entry]);
    }
    console.log(`[SRT] 採用: ${term.source_text} → ${term.webResult?.candidate_zh ?? term.surface_ja}`);
  };

  const handleDismissTerm = (term: UnresolvedTerm) => {
    if (!activeFile) return;
    removeUnresolvedTerm(activeFile.path, term.source_text);
    console.log(`[SRT] 無視: ${term.source_text}`);
  };

  const handleSelectFolder = async () => {
    try {
      setScanning(true);
      setError(null);
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true, multiple: false });
      if (!selected) return;
      const dirPath = selected as string;
      const discovered = await listSrtInDir(dirPath);
      if (discovered.length === 0) {
        setError("フォルダ内に英語字幕ファイルが見つかりませんでした。");
        setFolder(dirPath, []);
        return;
      }
      setFolder(dirPath, discovered);
      setActiveTab(0);

      // Derive project base_dir from SRT folder path and load dictionaries.
      // Priority: 1) already-loaded store, 2) useProjectStore baseDir, 3) derive from SRT path.
      const dictStore = useDictionaryStore.getState();
      if (dictStore.characters.length > 0 || dictStore.glossary.length > 0) {
        // Dictionaries already available — keep existing status
        setDictStatus(`OK (人物${dictStore.characters.length}人, 用語${dictStore.glossary.length}件)`);
        useAppLogStore.getState().addLog("info", "SRT", "辞書は既に読み込み済みです");
      } else {
        setDictStatus("検索中...");
        const projStore = useProjectStore.getState();
        let baseDir: string | null = null;
        if (projStore.baseDir) {
          baseDir = projStore.baseDir;
          useAppLogStore.getState().addLog("info", "SRT", `useProjectStore.baseDir から辞書を試行: ${baseDir}`);
        }
        if (!baseDir) {
          baseDir = await deriveBaseDir(dirPath);
        }
        if (baseDir) {
          await loadDictionariesFromDir(baseDir, setCharacters, setGlossary, setProject, setDictStatus);
          // Load drama info for batch AI確認 context
          try {
            const dramaInfo = await loadDramaInfo(baseDir);
            if (dramaInfo) {
              const zh = dramaInfo.metadata?.search_title_zh ?? "";
              const en = dramaInfo.metadata?.search_title_en ?? "";
              const ja = dramaInfo.metadata?.drama_title ?? "";
              const ctx = dramaInfo.synopsis_summary?.translation_context_short_ja ?? "";
              setDramaTitleZh(zh);
              setDramaTitleEn(en);
              setDramaTitleJa(ja);
              setShortContext(ctx);
              console.log(`[SRT] drama info loaded: ja="${ja}", zh="${zh}", en="${en}", context=${ctx.length}chars`);
            }
          } catch {
            // No drama info — fine, batch will use synopsis text as fallback
          }
        } else {
          setDictStatus("辞書フォルダ未検出");
          useAppLogStore.getState().addLog("warn", "SRT", `project base_dir not derived. dict not found within 3 levels of: ${dirPath}`);
        }
      }

      // Auto-load existing analyses (use fresh store state after dictionary load)
      const srtPaths = discovered.map((d) => d.path);
      try {
        const existing = await loadSrtAnalyses(srtPaths);
        const loadDictState = useDictionaryStore.getState();
        for (const a of existing) {
          if (a.synopsis) {
            const rawTerms = a.unresolved_terms ?? [];
            const { filtered: unresolvedTerms, removedCount } = filterUnresolvedByDict(rawTerms, loadDictState.characters, loadDictState.glossary);
            if (removedCount > 0) {
              console.log(`[SRT] ${a.srt_name}: unresolved_terms filtered by dictionary: ${removedCount}`);
            }
            setFileSynopsis(a.srt_path, a.synopsis, a.katakana_map ?? [], a.term_variants ?? [], unresolvedTerms);
          }
          if (a.adopted_terms && a.adopted_terms.length > 0) {
            setFileAdoptedTerms(a.srt_path, a.adopted_terms);
          }
          if (a.scene_detection) setFileSceneDetection(a.srt_path, a.scene_detection);
          for (const ctx of a.scene_contexts) {
            setFileSceneContext(a.srt_path, ctx.scene_index, ctx);
          }
        }
      } catch (_) {
        // No saved analyses yet — that's fine
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setScanning(false);
    }
  };

  const handleLoadAll = async () => {
    for (const f of files) {
      if (f.status === "loaded") continue;
      setFileStatus(f.path, "loading");
      try {
        const entries = await parseSrtFile(f.path);
        setFileEntries(f.path, entries);
      } catch (e) {
        setFileStatus(f.path, "error", String(e));
      }
    }
  };

  // Only loaded files become tabs
  const loadedFiles = files.filter((f) => f.status === "loaded");
  const activeFile: SrtFileState | undefined = loadedFiles[activeTab];

  // Button dependency states
  const hasLoaded = !!activeFile && activeFile.entries.length > 0;
  const hasSceneDetection = !!activeFile?.sceneDetection && activeFile.sceneDetection.scenes.length > 0;
  const allContextsReady = hasSceneDetection && activeFile!.sceneDetection!.scenes.every(
    (s) => activeFile!.sceneContexts[s.scene_index],
  );

  // Prompt context from dictionary store
  const promptContext = buildPromptContext(characters, glossary, activeFile?.katakanaMap, activeFile?.termVariants, activeFile?.unresolvedTerms, activeFile?.adoptedTerms);

  const handleSynopsis = async () => {
    if (!activeFile) return;
    setLoadingSynop(true);
    setError(null);
    try {
      let result = await generateSrtSynopsis(activeFile.entries, promptContext);

      // Resolve katakana proper nouns → kanji (only dictionary-backed matches)
      let katakanaMap: KatakanaKanjiMap[] = [];
      try {
        katakanaMap = await resolveSynopsisKatakana(
          result.synopsis_ja,
          result.unresolved_terms ?? [],
        );
        // Apply only resolved corrections to the synopsis text
        for (const m of katakanaMap) {
          if (m.status === "resolved" && m.kanji) {
            result = {
              ...result,
              synopsis_ja: result.synopsis_ja.split(m.katakana).join(m.kanji),
            };
          }
        }
        // Also correct detected_characters (resolved only)
        const correctedChars = result.detected_characters.map((ch) => {
          let c = ch;
          for (const m of katakanaMap) {
            if (m.status === "resolved" && m.kanji) {
              c = c.split(m.katakana).join(m.kanji);
            }
          }
          return c;
        });
        result = { ...result, detected_characters: correctedChars };
      } catch (_) {
        // Katakana resolution failed — proceed with raw text
      }

      const termVariants = result.term_variants ?? [];

      // Extract proper noun candidates from the full SRT body text
      let bodyTerms: UnresolvedTerm[] = [];
      try {
        const dictForBody = useDictionaryStore.getState();
        bodyTerms = await extractSrtBodyCandidates(activeFile.entries, dictForBody.characters, dictForBody.glossary);
        console.log(`[SRT] body candidates extracted: ${bodyTerms.length}`);
      } catch (_) {
        // Body extraction is best-effort
      }

      // Merge synopsis LLM terms with body extraction terms
      const synopsisTerms = (result.unresolved_terms ?? []).map((t) => ({
        ...t,
        source: t.source || "synopsis",
        occurrence_count: t.occurrence_count || 0,
      }));
      const merged = mergeUnresolvedTerms(synopsisTerms, bodyTerms);
      console.log(`[SRT] merged unresolved: synopsis=${synopsisTerms.length} body=${bodyTerms.length} → merged=${merged.length}`);

      const rawUnresolved = merged.map((t) => ({
        ...t,
        // Client-side safety net: clear surface_ja if it's English-only
        surface_ja: /[々〆〡-〩぀-ゟ゠-ヿ一-鿿㐀-䶿]/.test(t.surface_ja) ? t.surface_ja : "",
      }));
      // Read dictionary store directly (not from closure) to ensure current state
      const dictState = useDictionaryStore.getState();
      const { filtered: unresolvedTerms, removedCount } = filterUnresolvedByDict(rawUnresolved, dictState.characters, dictState.glossary);
      if (removedCount > 0) {
        console.log(`[SRT] unresolved_terms filtered by dictionary: ${removedCount}`);
      }
      setFileSynopsis(activeFile.path, result, katakanaMap, termVariants, unresolvedTerms);
      // Save after setting state
      const state = useSrtStore.getState();
      const f = state.files.find((x) => x.path === activeFile.path);
      if (f && folderPath) {
        await saveSrtAnalysis({
          srt_path: f.path,
          srt_name: f.name,
          synopsis: result,
          scene_detection: f.sceneDetection,
          scene_contexts: Object.values(f.sceneContexts),
          katakana_map: katakanaMap,
          term_variants: termVariants,
          unresolved_terms: unresolvedTerms,
          adopted_terms: f.adoptedTerms,
        });
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingSynop(false);
    }
  };

  const handleDetectScenes = async () => {
    if (!activeFile) return;
    setLoadingScene(true);
    setError(null);
    try {
      const ctx = buildPromptContext(characters, glossary, activeFile.katakanaMap, activeFile.termVariants, activeFile.unresolvedTerms, activeFile.adoptedTerms);
      const result = await detectSrtScenes(activeFile.entries, ctx);
      setFileSceneDetection(activeFile.path, result);
      const state = useSrtStore.getState();
      const f = state.files.find((x) => x.path === activeFile.path);
      if (f && folderPath) {
        await saveSrtAnalysis({
          srt_path: f.path,
          srt_name: f.name,
          synopsis: f.synopsis,
          scene_detection: result,
          scene_contexts: Object.values(f.sceneContexts),
          katakana_map: f.katakanaMap,
          term_variants: f.termVariants,
          unresolved_terms: f.unresolvedTerms,
          adopted_terms: f.adoptedTerms,
        });
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingScene(false);
    }
  };

  const handleAnalyzeContext = async () => {
    if (!activeFile) return;
    const sceneDetection = activeFile.sceneDetection;
    if (!sceneDetection || sceneDetection.scenes.length === 0) return;
    setLoadingContext(true);
    setError(null);
    try {
      const characterNames = activeFile.synopsis?.detected_characters;
      for (const scene of sceneDetection.scenes) {
        const sceneEntries = activeFile.entries.filter(
          (e) => e.index >= scene.start_entry_index && e.index <= scene.end_entry_index,
        );
        if (sceneEntries.length === 0) continue;
        const ctx = buildPromptContext(characters, glossary, activeFile.katakanaMap, activeFile.termVariants, activeFile.unresolvedTerms, activeFile.adoptedTerms);
        const result = await analyzeSceneContext(sceneEntries, characterNames, ctx);
        result.scene_index = scene.scene_index;
        setFileSceneContext(activeFile.path, scene.scene_index, result);
      }
      // Save after all contexts are set
      const state = useSrtStore.getState();
      const f = state.files.find((x) => x.path === activeFile.path);
      if (f && folderPath) {
        await saveSrtAnalysis({
          srt_path: f.path,
          srt_name: f.name,
          synopsis: f.synopsis,
          scene_detection: f.sceneDetection,
          scene_contexts: Object.values(f.sceneContexts),
          katakana_map: f.katakanaMap,
          term_variants: f.termVariants,
          unresolved_terms: f.unresolvedTerms,
          adopted_terms: f.adoptedTerms,
        });
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingContext(false);
    }
  };

  const pendingCount = files.filter((f) => f.status === "pending").length;
  const loadedCount = loadedFiles.length;
  const errorCount = files.filter((f) => f.status === "error").length;

  return (
    <div className="card">
      <h2 className="card-title">SRT読み込み</h2>
      <p style={{ color: "var(--text-secondary)", marginBottom: 16, fontSize: 13 }}>
        字幕ファイルが含まれるフォルダを選択してください。
        <br />
        英語字幕ファイルが自動で検出されます。
      </p>

      <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
        <button
          className="btn btn-primary"
          onClick={handleSelectFolder}
          disabled={scanning}
        >
          <FolderOpen size={16} />
          {scanning ? "スキャン中..." : "フォルダを選択"}
        </button>

        {files.length > 0 && pendingCount > 0 && (
          <button className="btn btn-secondary" onClick={handleLoadAll}>
            <RefreshCw size={16} />
            すべて読み込み ({pendingCount}件)
          </button>
        )}
      </div>

      {error && (
        <p style={{ color: "var(--error)", marginTop: 12, fontSize: 13 }}>{error}</p>
      )}

      {folderPath && (
        <div style={{ marginTop: 12 }}>
          <p style={{ fontSize: 12, color: "var(--text-muted)", marginBottom: 4 }}>
            フォルダ: <span style={{ fontFamily: "monospace" }}>{folderPath}</span>
          </p>
          <p style={{ fontSize: 12, color: "var(--text-muted)", marginBottom: 0 }}>
            {files.length} 件検出
            {loadedCount > 0 && `・${loadedCount} 件読み込み済`}
            {errorCount > 0 && (
              <span style={{ color: "var(--error)" }}>・{errorCount} 件エラー</span>
            )}
            {characters.length > 0 && (
              <span style={{ color: "var(--success, #16a34a)" }}>
                ・登場人物 {characters.length}人
              </span>
            )}
            {glossary.length > 0 && (
              <span style={{ color: "var(--success, #16a34a)" }}>
                ・用語 {glossary.length}件
              </span>
            )}
            <span style={{ color: dictStatus.startsWith("OK") ? "var(--success, #16a34a)" : dictStatus === "未確認" ? "var(--text-muted)" : "var(--error, #dc2626)" }}>
              ・辞書: {dictStatus}
            </span>
          </p>
        </div>
      )}

      {/* File list table (pre-load) */}
      {files.length > 0 && loadedCount === 0 && (
        <div className="table-container" style={{ marginTop: 8, maxHeight: 320, overflowY: "auto" }}>
          <table>
            <thead>
              <tr>
                <th style={{ width: "50%" }}>
                  <FileText size={14} style={{ verticalAlign: "middle", marginRight: 4 }} />
                  ファイル名
                </th>
                <th style={{ width: "20%", textAlign: "center" }}>行数</th>
                <th style={{ width: "30%", textAlign: "center" }}>状態</th>
              </tr>
            </thead>
            <tbody>
              {files.map((f) => (
                <tr key={f.path}>
                  <td style={{ fontFamily: "monospace", fontSize: 12, wordBreak: "break-all" }}>
                    {f.name}
                  </td>
                  <td style={{ textAlign: "center", fontSize: 12 }}>
                    {f.status === "loaded" ? f.entries.length : "—"}
                  </td>
                  <td style={{ textAlign: "center" }}>
                    {f.status === "pending" && (
                      <span style={{ color: "var(--text-muted)", fontSize: 12 }}>待機中</span>
                    )}
                    {f.status === "loading" && (
                      <span style={{ color: "var(--accent)", fontSize: 12 }}>読み込み中...</span>
                    )}
                    {f.status === "loaded" && <span className="status-pill success">完了</span>}
                    {f.status === "error" && (
                      <span className="status-pill high">エラー: {f.error ?? "不明"}</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Tabs + Analysis (post-load) */}
      {loadedCount > 0 && (
        <div style={{ marginTop: 16 }}>
          {/* Tab bar */}
          <div style={{ display: "flex", gap: 2, borderBottom: "1px solid var(--border)" }}>
            {loadedFiles.map((f, i) => (
              <button
                key={f.path}
                className={i === activeTab ? "btn btn-primary" : "btn btn-secondary"}
                style={{
                  borderRadius: "6px 6px 0 0",
                  borderBottom: i === activeTab ? "2px solid var(--accent)" : "1px solid var(--border)",
                  fontSize: 12,
                  padding: "6px 12px",
                  minHeight: 30,
                }}
                onClick={() => setActiveTab(i)}
              >
                <FileText size={12} />
                {f.name}
              </button>
            ))}
          </div>

          {/* Tab content */}
          {activeFile && (
            <div style={{ padding: "16px 0" }}>
              <h3 style={{ fontSize: 14, marginBottom: 12, color: "var(--text-secondary)" }}>
                分析 — {activeFile.name} ({activeFile.entries.length}行)
              </h3>

              {/* 4 fire buttons */}
              <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginBottom: 16 }}>
                <button
                  className="btn btn-primary"
                  onClick={handleSynopsis}
                  disabled={!hasLoaded || loadingSynop}
                  style={{ fontSize: 12 }}
                >
                  <BookOpen size={14} />
                  2.1 あらすじ生成
                </button>
                <button
                  className="btn btn-primary"
                  onClick={handleDetectScenes}
                  disabled={!hasLoaded || loadingScene}
                  style={{ fontSize: 12 }}
                >
                  <Layers size={14} />
                  2.2 場面検出
                </button>
                <button
                  className="btn btn-primary"
                  onClick={handleAnalyzeContext}
                  disabled={!hasSceneDetection || loadingContext}
                  style={{ fontSize: 12 }}
                >
                  <MessageSquare size={14} />
                  2.3 状況分析
                </button>
                <button
                  className="btn btn-primary"
                  disabled={!allContextsReady}
                  style={{ fontSize: 12 }}
                >
                  <Settings size={14} />
                  2.4 翻訳設定
                </button>
              </div>

              {characters.length === 0 && glossary.length === 0 && (
                <div style={{ color: "#e6a817", fontSize: 12, marginTop: 4 }}>
                  辞書が未読み込みのため、既知語の除外ができません。
                </div>
              )}

              {/* 2.1 Synopsis result */}
              {activeFile.synopsis && (
                <div style={{ marginBottom: 16 }}>
                  <h4 style={{ fontSize: 13, marginBottom: 8 }}>2.1 あらすじ</h4>
                  <div
                    style={{
                      background: "var(--bg-secondary)",
                      padding: 12,
                      borderRadius: 4,
                      fontSize: 13,
                      lineHeight: 1.6,
                      whiteSpace: "pre-wrap",
                      border: "1px solid var(--border)",
                    }}
                  >
                    {activeFile.synopsis.synopsis_ja}
                  </div>
                  {activeFile.synopsis.detected_characters.length > 0 && (
                    <div style={{ marginTop: 8, display: "flex", gap: 4, flexWrap: "wrap" }}>
                      {activeFile.synopsis.detected_characters.map((ch) => (
                        <span key={ch} className="status-pill medium">
                          {ch}
                        </span>
                      ))}
                    </div>
                  )}
                  {/* Unresolved terms (primary) or Katakana map (fallback) */}
                  {activeFile.unresolvedTerms.length > 0 && (() => {
                    const katakanaLookup = new Map<string, string>();
                    for (const m of activeFile.katakanaMap) {
                      if (m.status === "resolved" && m.kanji) katakanaLookup.set(m.katakana, m.kanji);
                    }
                    const pending = activeFile.unresolvedTerms.filter((t) => !t.adopted);
                    return (
                      <div style={{ marginTop: 12 }}>
                        <h4 style={{ fontSize: 12, marginBottom: 6, color: "var(--text-secondary)" }}>
                          固有名詞補正
                          {activeFile.adoptedTerms.length > 0 && (
                            <span style={{ marginLeft: 8, fontSize: 11, color: "var(--success)" }}>
                              （採用済: {activeFile.adoptedTerms.length}件）
                            </span>
                          )}
                        </h4>
                        {/* Batch AI確認 button + CSV export */}
                        <div style={{ marginBottom: 8, display: "flex", gap: 8, alignItems: "center" }}>
                          {pending.filter((t) => !t.adopted && !t.webResult).length > 0 && (
                            <button
                              className="btn btn-primary"
                              onClick={handleBatchAiConfirm}
                              disabled={activeFile.batchTermLoading}
                              style={{ fontSize: 12 }}
                            >
                              {activeFile.batchTermLoading
                                ? "一括AI確認中…"
                                : `未確認語をまとめてAI確認 (${pending.filter((t) => !t.adopted && !t.webResult).length}件)`}
                            </button>
                          )}
                          <button
                            className="btn btn-sm"
                            style={{ fontSize: 12, background: "#6b7280", color: "#fff" }}
                            onClick={() => exportUnresolvedTermsCsv(activeFile.unresolvedTerms, activeFile.name)}
                          >
                            CSV出力 ({activeFile.unresolvedTerms.length}件)
                          </button>
                        </div>
                        <div className="table-container" style={{ maxHeight: 300, overflowY: "auto" }}>
                          <table>
                            <thead>
                              <tr>
                                <th style={{ width: "15%" }}>元の表記</th>
                                <th style={{ width: "12%" }}>日本語候補</th>
                                <th style={{ width: "15%" }}>検索候補</th>
                                <th style={{ width: "12%" }}>確定表記</th>
                                <th style={{ width: "10%", textAlign: "center" }}>状態</th>
                                <th>理由</th>
                                <th style={{ width: "160px", textAlign: "center" }}>操作</th>
                              </tr>
                            </thead>
                            <tbody>
                              {pending.map((t) => {
                                const isLoading = activeFile.termLoading[t.source_text] ?? false;
                                const hasCandidate = (t.webResult?.status === "candidate_found" || t.webResult?.status === "found") && t.webResult?.candidate_zh;
                                const isUncertain = t.webResult?.status === "uncertain";
                                const noEvidence = t.webResult?.status === "not_found";
                                const hasError = t.webResult?.status === "error";
                                return (
                                  <tr key={t.source_text}>
                                    <td style={{ fontFamily: "monospace", fontSize: 12 }}>
                                      {t.source_text}
                                      {t.source && t.source !== "synopsis" && (
                                        <span style={{
                                          display: "inline-block",
                                          marginLeft: 6,
                                          padding: "1px 5px",
                                          borderRadius: 3,
                                          fontSize: 10,
                                          fontWeight: 600,
                                          background: t.source === "srt_body+synopsis" ? "#7c3aed" : "#3b82f6",
                                          color: "#fff",
                                          verticalAlign: "middle",
                                        }}>
                                          {t.source === "srt_body+synopsis" ? "本文+あらすじ" : "本文"}
                                        </span>
                                      )}
                                      {t.source === "synopsis" && (
                                        <span style={{
                                          display: "inline-block",
                                          marginLeft: 6,
                                          padding: "1px 5px",
                                          borderRadius: 3,
                                          fontSize: 10,
                                          fontWeight: 600,
                                          background: "#22c55e",
                                          color: "#fff",
                                          verticalAlign: "middle",
                                        }}>
                                          あらすじ
                                        </span>
                                      )}
                                    </td>
                                    <td style={{ fontFamily: "monospace", fontSize: 12 }}>
                                      {t.webResult ? (t.surface_ja || "—") : "—"}
                                    </td>
                                    <td style={{ fontFamily: "monospace", fontSize: 12 }}>
                                      {t.webResult ? (t.webResult?.candidate_zh ?? "—") : "—"}
                                    </td>
                                    <td style={{ fontFamily: "monospace", fontSize: 12 }}>—</td>
                                    <td style={{ textAlign: "center" }}>
                                      {!t.webResult && (
                                        <span className="status-pill high">未確認</span>
                                      )}
                                      {hasCandidate && (
                                        <span className="status-pill medium" title={`source: ${t.webResult?.source ?? "web"}, confidence: ${t.webResult?.confidence}`}>候補あり{(t.webResult?.source === "gemini" || t.webResult?.source === "openai") ? " (AI)" : ""}</span>
                                      )}
                                      {isUncertain && (
                                        <span className="status-pill" style={{ background: "#f59e0b", color: "#fff" }} title={`confidence: ${t.webResult?.confidence}`}>推定 (低信頼)</span>
                                      )}
                                      {noEvidence && (
                                        <span className="status-pill" style={{ background: "#6b7280", color: "#fff" }}>根拠なし</span>
                                      )}
                                      {hasError && (
                                        <span className="status-pill" style={{ background: "#ef4444", color: "#fff" }}>エラー</span>
                                      )}
                                    </td>
                                    <td style={{ fontSize: 11, color: "var(--text-secondary)" }}>
                                      {(() => {
                                        const wr = t.webResult;
                                        const ev = wr?.evidence;
                                        if (ev?.length) {
                                          return (
                                            <span>
                                              {ev.slice(0, 3).map((e, i) => (
                                                <span key={i}>
                                                  <a href={e.url} target="_blank" rel="noopener noreferrer" style={{ color: "var(--accent)", textDecoration: "underline" }}>
                                                    {e.title || e.url}
                                                  </a>
                                                  {i < Math.min(ev.length, 3) - 1 ? " / " : ""}
                                                </span>
                                              ))}
                                            </span>
                                          );
                                        }
                                        return wr?.evidence_summary ?? t.reason;
                                      })()}
                                    </td>
                                    <td style={{ textAlign: "center", whiteSpace: "nowrap" }}>
                                      {!t.webResult && (
                                        <>
                                          <button
                                            className="btn btn-sm"
                                            disabled={isLoading}
                                            onClick={() => handleAiConfirm(t)}
                                            style={{ marginRight: 4 }}
                                          >
                                            {isLoading ? "検索中…" : "AI確認"}
                                          </button>
                                          <button
                                            className="btn btn-sm"
                                            style={{ background: "#6b7280", color: "#fff" }}
                                            onClick={() => handleDismissTerm(t)}
                                          >
                                            無視
                                          </button>
                                        </>
                                      )}
                                      {hasCandidate && (
                                        <>
                                          <button
                                            className="btn btn-sm"
                                            style={{ background: "var(--success, #22c55e)", color: "#fff", marginRight: 4 }}
                                            onClick={() => handleAdoptTerm(t)}
                                          >
                                            採用
                                          </button>
                                          <button
                                            className="btn btn-sm"
                                            disabled={isLoading}
                                            onClick={() => handleAiConfirm(t)}
                                            style={{ marginRight: 4 }}
                                          >
                                            {isLoading ? "検索中…" : "AI再確認"}
                                          </button>
                                          <button
                                            className="btn btn-sm"
                                            style={{ background: "#6b7280", color: "#fff" }}
                                            onClick={() => handleDismissTerm(t)}
                                          >
                                            無視
                                          </button>
                                        </>
                                      )}
                                      {hasError && (
                                        <>
                                          <button
                                            className="btn btn-sm"
                                            disabled={isLoading}
                                            onClick={() => handleAiConfirm(t)}
                                            style={{ marginRight: 4 }}
                                          >
                                            {isLoading ? "検索中…" : "AI再確認"}
                                          </button>
                                          <button
                                            className="btn btn-sm"
                                            style={{ background: "#6b7280", color: "#fff" }}
                                            onClick={() => handleDismissTerm(t)}
                                          >
                                            無視
                                          </button>
                                        </>
                                      )}
                                      {noEvidence && (
                                        <>
                                          <button
                                            className="btn btn-sm"
                                            disabled={isLoading}
                                            onClick={() => handleAiConfirm(t)}
                                            style={{ marginRight: 4 }}
                                          >
                                            {isLoading ? "検索中…" : "AI再確認"}
                                          </button>
                                          <button
                                            className="btn btn-sm"
                                            style={{ background: "#6b7280", color: "#fff" }}
                                            onClick={() => handleDismissTerm(t)}
                                          >
                                            無視
                                          </button>
                                        </>
                                      )}
                                    </td>
                                  </tr>
                                );
                              })}
                            </tbody>
                          </table>
                        </div>
                      </div>
                    );
                  })()}
                  {activeFile.unresolvedTerms.length === 0 && activeFile.katakanaMap.length > 0 && (
                    <div style={{ marginTop: 12 }}>
                      <h4 style={{ fontSize: 12, marginBottom: 6, color: "var(--text-secondary)" }}>
                        固有名詞補正
                      </h4>
                      <div className="table-container" style={{ maxHeight: 200, overflowY: "auto" }}>
                        <table>
                          <thead>
                            <tr>
                              <th style={{ width: "25%" }}>元の表記</th>
                              <th style={{ width: "25%" }}>カタカナ</th>
                              <th style={{ width: "20%" }}>漢字</th>
                              <th style={{ width: "10%", textAlign: "center" }}>状態</th>
                              <th>理由</th>
                            </tr>
                          </thead>
                          <tbody>
                            {activeFile.katakanaMap.map((m) => (
                              <tr key={m.katakana}>
                                <td style={{ fontFamily: "monospace", fontSize: 12 }}>{m.original_text || "—"}</td>
                                <td style={{ fontFamily: "monospace", fontSize: 12 }}>{m.katakana}</td>
                                <td style={{ fontFamily: "monospace", fontSize: 12 }}>
                                  {m.kanji ?? "—"}
                                </td>
                                <td style={{ textAlign: "center" }}>
                                  {m.status === "resolved" ? (
                                    <span className="status-pill success">確定</span>
                                  ) : (
                                    <span className="status-pill high">要確認</span>
                                  )}
                                </td>
                                <td style={{ fontSize: 11, color: "var(--text-secondary)" }}>
                                  {m.reason}
                                </td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      </div>
                    </div>
                  )}
                  {/* Term variant candidates */}
                  {activeFile.termVariants.length > 0 && (
                    <div style={{ marginTop: 12 }}>
                      <h4 style={{ fontSize: 12, marginBottom: 6, color: "var(--text-secondary)" }}>
                        表記揺れ候補
                      </h4>
                      <div className="table-container" style={{ maxHeight: 200, overflowY: "auto" }}>
                        <table>
                          <thead>
                            <tr>
                              <th style={{ width: "50%" }}>異表記</th>
                              <th style={{ width: "20%", textAlign: "center" }}>状態</th>
                              <th>理由</th>
                            </tr>
                          </thead>
                          <tbody>
                            {activeFile.termVariants.map((v, i) => (
                              <tr key={i}>
                                <td style={{ fontFamily: "monospace", fontSize: 12 }}>
                                  {v.variants.join(" / ")}
                                </td>
                                <td style={{ textAlign: "center" }}>
                                  {v.status === "resolved" ? (
                                    <span className="status-pill success">確定</span>
                                  ) : (
                                    <span className="status-pill high">要確認</span>
                                  )}
                                </td>
                                <td style={{ fontSize: 11, color: "var(--text-secondary)" }}>
                                  {v.reason}
                                </td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      </div>
                    </div>
                  )}
                </div>
              )}

              {/* 2.2 Scene detection result */}
              {activeFile.sceneDetection && (
                <div style={{ marginBottom: 16 }}>
                  <h4 style={{ fontSize: 13, marginBottom: 8 }}>
                    2.2 場面検出 ({activeFile.sceneDetection.scenes.length}場面)
                  </h4>
                  <div className="table-container" style={{ maxHeight: 300, overflowY: "auto" }}>
                    <table>
                      <thead>
                        <tr>
                          <th style={{ width: 30 }}>#</th>
                          <th style={{ width: 140 }}>場面ラベル</th>
                          <th style={{ textAlign: "center", width: 80 }}>字幕範囲</th>
                          <th style={{ textAlign: "center", width: 50 }}>行数</th>
                          <th>理由</th>
                        </tr>
                      </thead>
                      <tbody>
                        {activeFile.sceneDetection.scenes.map((s) => (
                          <tr key={s.scene_index}>
                            <td style={{ textAlign: "center", fontSize: 12 }}>{s.scene_index + 1}</td>
                            <td style={{ fontSize: 12 }}>{s.title}</td>
                            <td style={{ textAlign: "center", fontSize: 12, fontFamily: "monospace" }}>
                              {s.start_entry_index}–{s.end_entry_index}
                            </td>
                            <td style={{ textAlign: "center", fontSize: 12 }}>{s.entry_count}</td>
                            <td style={{ fontSize: 12, color: "var(--text-secondary)" }}>{s.reason || "理由未出力"}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              )}

              {/* 2.3 Scene context results */}
              {Object.keys(activeFile.sceneContexts).length > 0 && (
                <div style={{ marginBottom: 16 }}>
                  <h4 style={{ fontSize: 13, marginBottom: 8 }}>2.3 場面の状況設定</h4>
                  {activeFile.sceneDetection?.scenes.map((s) => {
                    const ctx = activeFile.sceneContexts[s.scene_index];
                    if (!ctx) return null;
                    return (
                      <div
                        key={s.scene_index}
                        style={{
                          background: "var(--bg-secondary)",
                          padding: 12,
                          borderRadius: 4,
                          marginBottom: 8,
                          border: "1px solid var(--border)",
                        }}
                      >
                        <div style={{ fontSize: 12, fontWeight: 600, marginBottom: 6 }}>
                          場面 {s.scene_index + 1}: {s.title}
                        </div>
                        <div style={{ fontSize: 13, lineHeight: 1.6, whiteSpace: "pre-wrap" }}>
                          {ctx.context_ja}
                        </div>
                        {ctx.hierarchy && (
                          <div style={{ fontSize: 12, marginTop: 6, color: "var(--text-secondary)" }}>
                            上下関係: {ctx.hierarchy}
                          </div>
                        )}
                        {ctx.gender_notes.length > 0 && (
                          <div style={{ marginTop: 6, display: "flex", gap: 4, flexWrap: "wrap" }}>
                            {ctx.gender_notes.map((g, i) => (
                              <span key={i} className="status-pill medium" style={{ fontSize: 11 }}>
                                {g}
                              </span>
                            ))}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}

              {/* 2.4 Translation settings — assembled prompt per scene */}
              {allContextsReady && activeFile.sceneDetection && (
                <div>
                  <h4 style={{ fontSize: 13, marginBottom: 8 }}>2.4 翻訳設定</h4>
                  {activeFile.sceneDetection.scenes.map((s) => {
                    const ctx = activeFile.sceneContexts[s.scene_index];
                    if (!ctx) return null;
                    const sceneEntries = activeFile.entries.filter(
                      (e) => e.index >= s.start_entry_index && e.index <= s.end_entry_index,
                    );
                    const promptText = buildSceneTranslationPrompt(
                      characters,
                      glossary,
                      ctx.context_ja,
                      sceneEntries,
                      activeFile.katakanaMap,
                      activeFile.termVariants,
                      activeFile.unresolvedTerms,
                      activeFile.adoptedTerms,
                    );
                    return (
                      <div key={s.scene_index} style={{ marginBottom: 12 }}>
                        <div style={{ fontSize: 12, fontWeight: 600, marginBottom: 4 }}>
                          場面 {s.scene_index + 1}: {s.title} ({sceneEntries.length}行)
                        </div>
                        <textarea
                          readOnly
                          value={promptText}
                          style={{
                            width: "100%",
                            height: 200,
                            fontFamily: "monospace",
                            fontSize: 11,
                            lineHeight: 1.5,
                            padding: 8,
                            borderRadius: 4,
                            border: "1px solid var(--border)",
                            background: "var(--bg-secondary)",
                            resize: "vertical",
                          }}
                        />
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
