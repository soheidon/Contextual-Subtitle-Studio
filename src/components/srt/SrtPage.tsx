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
import { appendToGlossary, batchSaveAdoptedTerms } from "../../lib/dictionarySync";
import { buildBatchPrompt, parseChatGptResponse } from "../../lib/chatGptPaste";
import { extractCleanUrls } from "../../lib/urlCleaner";
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
import type { SubtitleEntry, KatakanaKanjiMap, TermVariantEntry, UnresolvedTerm, GlossaryEntry, BatchTermRequest, WebTermResolution, Character } from "../../types";

/** Normalize an English name for dictionary matching: lowercase, strip spaces/hyphens/apostrophes/punctuation */
function normalizeEn(text: string): string {
  return text.toLowerCase().replace(/[^a-z0-9]/g, "");
}

// ---------------------------------------------------------------------------
// Romanized-to-katakana helper (TypeScript port of Rust romanized_to_katakana_candidates)
// ---------------------------------------------------------------------------

const ROMAJI_MAP: [string, string][] = [
  // Chinese pinyin overrides (match before generic romaji)
  ["qian","チェン"],["cheng","チェン"],["zhang","ジャン"],
  ["chang","チャン"],["chun","チュン"],["xian","シエン"],
  ["jian","ジエン"],["yuan","ユエン"],["hui","フイ"],
  ["song","ソン"],["tong","トン"],["dong","ドン"],
  ["zhen","ジェン"],["zheng","ジェン"],["shan","シャン"],
  ["shen","シェン"],["jing","ジン"],["yong","ヨン"],
  ["tuo","トウ"],["jin","ジン"],["er","アル"],
  ["lun","ルン"],["run","ルン"],["jun","ジュン"],
  // Palatalized clusters
  ["kya","キャ"],["kyu","キュ"],["kyo","キョ"],
  ["sha","シャ"],["shu","シュ"],["sho","ショ"],
  ["cha","チャ"],["chu","チュ"],["cho","チョ"],
  ["nya","ニャ"],["nyu","ニュ"],["nyo","ニョ"],
  ["hya","ヒャ"],["hyu","ヒュ"],["hyo","ヒョ"],
  ["mya","ミャ"],["myu","ミュ"],["myo","ミョ"],
  ["rya","リャ"],["ryu","リュ"],["ryo","リョ"],
  ["gya","ギャ"],["gyu","ギュ"],["gyo","ギョ"],
  ["ja","ジャ"],["ju","ジュ"],["jo","ジョ"],
  ["bya","ビャ"],["byu","ビュ"],["byo","ビョ"],
  ["pya","ピャ"],["pyu","ピュ"],["pyo","ピョ"],
  // Voiced consonant+vowel
  ["ga","ガ"],["gi","ギ"],["gu","グ"],["ge","ゲ"],["go","ゴ"],
  ["za","ザ"],["ji","ジ"],["zu","ズ"],["ze","ゼ"],["zo","ゾ"],
  ["da","ダ"],["de","デ"],["do","ド"],
  ["ba","バ"],["bi","ビ"],["bu","ブ"],["be","ベ"],["bo","ボ"],
  ["pa","パ"],["pi","ピ"],["pu","プ"],["pe","ペ"],["po","ポ"],
  // Basic consonant+vowel
  ["ka","カ"],["ki","キ"],["ku","ク"],["ke","ケ"],["ko","コ"],
  ["sa","サ"],["shi","シ"],["su","ス"],["se","セ"],["so","ソ"],
  ["ta","タ"],["chi","チ"],["tsu","ツ"],["te","テ"],["to","ト"],
  ["na","ナ"],["ni","ニ"],["nu","ヌ"],["ne","ネ"],["no","ノ"],
  ["ha","ハ"],["hi","ヒ"],["fu","フ"],["he","ヘ"],["ho","ホ"],
  ["ma","マ"],["mi","ミ"],["mu","ム"],["me","メ"],["mo","モ"],
  ["ya","ヤ"],["yu","ユ"],["yo","ヨ"],
  ["ra","ラ"],["ri","リ"],["ru","ル"],["re","レ"],["ro","ロ"],
  ["wa","ワ"],["wo","ヲ"],
  // Standalone vowels + syllabic n
  ["a","ア"],["i","イ"],["u","ウ"],["e","エ"],["o","オ"],
  ["n","ン"],
];

function romanizeWord(word: string): string {
  const lower = word.toLowerCase();
  let result = "";
  let pos = 0;
  while (pos < lower.length) {
    let matched = false;
    for (let len = Math.min(6, lower.length - pos); len >= 1; len--) {
      const slice = lower.slice(pos, pos + len);
      const entry = ROMAJI_MAP.find(([pat]) => pat === slice);
      if (entry) {
        result += entry[1];
        pos += len;
        matched = true;
        break;
      }
    }
    if (!matched) pos++;
  }
  return result;
}

function romanizedToKatakanaCandidates(source: string): string[] {
  const candidates: string[] = [];

  // Pass A: concatenated (strip spaces/hyphens/apostrophes)
  const stripped = source.toLowerCase().replace(/[\s\-']/g, "");
  const concat = romanizeWord(stripped);
  if (concat) candidates.push(concat);

  // Pass B: word-separated form with middle dots
  const words = source.split(/\s+/).filter(Boolean);
  if (words.length >= 2) {
    const dotForm = words.map((w) => romanizeWord(w)).filter(Boolean);
    if (dotForm.length === words.length) {
      candidates.push(dotForm.join("・"));
    }
  }

  return [...new Set(candidates)];
}

// ---------------------------------------------------------------------------
// Dictionary replacement map (TypeScript port of Rust build_dictionary_replacement_map + resolve_known_terms_in_text)
// ---------------------------------------------------------------------------

function hiraganaToKatakana(s: string): string {
  return s.replace(/[ぁ-ゖ]/g, (ch) =>
    String.fromCharCode(ch.charCodeAt(0) + 0x60),
  );
}

function buildDictionaryReplacementMap(
  characters: Character[],
  glossary: GlossaryEntry[],
): [string, string][] {
  const pairs: [string, string][] = [];

  // 1. Characters: hiragana→katakana of japanese_name → japanese_name
  for (const c of characters) {
    const reading = hiraganaToKatakana(c.japanese_name);
    if (reading !== c.japanese_name && reading) pairs.push([reading, c.japanese_name]);
  }

  // 2. Characters: aliases → japanese_name
  for (const c of characters) {
    for (const alias of c.aliases) {
      if (alias) pairs.push([alias, c.japanese_name]);
    }
  }

  // 2b. Characters: English name variants (space-stripped, underscored) → japanese_name
  for (const c of characters) {
    const en = (c.english_name || "").trim();
    if (!en) continue;
    const noSpace = en.replace(/\s+/g, "");
    if (noSpace !== en) pairs.push([noSpace, c.japanese_name]);
    const underscored = en.replace(/\s+/g, "_");
    if (underscored !== en && underscored !== noSpace) pairs.push([underscored, c.japanese_name]);
  }

  // 3. Glossary: source → target (direct)
  for (const g of glossary) {
    if (g.source && g.target) pairs.push([g.source, g.target]);
  }

  // 4. Glossary: stored aliases → target
  for (const g of glossary) {
    if (g.aliases) {
      for (const alias of g.aliases) {
        if (alias) pairs.push([alias, g.target]);
      }
    }
  }

  // 5. Glossary: romanized source → katakana candidates → target
  for (const g of glossary) {
    if (g.source && g.target) {
      for (const kana of romanizedToKatakanaCandidates(g.source)) {
        pairs.push([kana, g.target]);
      }
    }
  }

  // 6. Glossary: hiragana→katakana of target → target
  for (const g of glossary) {
    const reading = hiraganaToKatakana(g.target);
    if (reading !== g.target && reading) pairs.push([reading, g.target]);
  }

  // Sort longest pattern first, deduplicate by pattern (last wins)
  const seen = new Set<string>();
  const result: [string, string][] = [];
  // Reverse iterate to keep later entries (higher priority) when deduping
  for (let i = pairs.length - 1; i >= 0; i--) {
    if (!seen.has(pairs[i][0])) {
      seen.add(pairs[i][0]);
      result.push(pairs[i]);
    }
  }
  result.reverse();
  result.sort((a, b) => b[0].length - a[0].length);
  return result;
}

function resolveKnownTermsInText(
  text: string,
  characters: Character[],
  glossary: GlossaryEntry[],
): string {
  const map = buildDictionaryReplacementMap(characters, glossary);
  let result = text;
  for (const [pattern, replacement] of map) {
    if (pattern === replacement) continue;
    // Use split-join to replace all occurrences
    result = result.split(pattern).join(replacement);
  }
  return result;
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

/**
 * Derive needs_human_review from evidence fields, overriding stale stored values.
 * - status==="found" && confidence==="high" && evidence_strength==="direct" && match_judgment==="exact" → false
 * - weak/not_found/none evidence → true
 * - otherwise fall back to stored value (or true as safe default)
 */
function deriveNeedsHumanReview(wr: WebTermResolution): boolean {
  // Strong evidence combo → no human review needed
  if (
    wr.status === "found" &&
    wr.confidence === "high" &&
    wr.evidence_strength === "direct" &&
    wr.match_judgment === "exact"
  ) {
    return false;
  }
  // Clear weak/no-evidence cases → human review needed
  if (
    wr.status === "not_found" ||
    wr.status === "uncertain" ||
    wr.match_judgment === "weak" ||
    wr.match_judgment === "not_found" ||
    wr.evidence_strength === "none" ||
    wr.confidence === "low"
  ) {
    return true;
  }
  // Fall back to stored value
  return wr.needs_human_review ?? true;
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
        // Preserve alias fields: synopsis aliases take priority if body doesn't have them
        search_text: existing.search_text || t.search_text,
        generic_suffix: existing.generic_suffix || t.generic_suffix,
        aliases: existing.aliases || t.aliases,
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

/** Export scene detection results as CSV. */
function exportSceneDetectionCsv(
  scenes: { scene_index: number; title: string; start_entry_index: number; end_entry_index: number; entry_count: number; reason: string }[],
  filename: string,
  sceneContexts?: Record<number, { hierarchy: string | null; gender_notes: string[]; context_ja: string }>,
) {
  const BOM = "﻿";
  const headers = ["scene_index", "title", "start_entry_index", "end_entry_index", "entry_count", "reason", "関係性", "上下関係", "性別注記"];
  const rows = scenes.map((s) => {
    const ctx = sceneContexts?.[s.scene_index];
    const summary = ctx
      ? (ctx.hierarchy
        || ctx.gender_notes?.join(" / ")
        || ctx.context_ja?.split(/[。\n]/)[0]
        || "")
      : "";
    return [
      s.scene_index + 1,
      s.title,
      s.start_entry_index,
      s.end_entry_index,
      s.entry_count,
      s.reason,
      summary,
      ctx?.hierarchy ?? "",
      ctx?.gender_notes?.join(" / ") ?? "",
    ];
  });
  const escape = (v: string | number) => {
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
  a.download = filename.replace(/\.srt$/i, "") + "_scenes.csv";
  a.click();
  URL.revokeObjectURL(url);
}

/** Export unresolved terms as CSV. Includes all terms (not just pending). */
function exportUnresolvedTermsCsv(terms: UnresolvedTerm[], filename: string) {
  const BOM = "﻿";
  const headers = [
    "source_text", "surface_ja", "search_text", "generic_suffix", "search_aliases",
    "term_type", "status", "source", "occurrence_count",
    "reason", "web_candidate_zh", "web_status", "web_confidence",
    "web_evidence_summary", "web_evidence_urls",
    "web_evidence_strength", "web_match_judgment", "web_needs_human_review", "web_confidence_reason",
    "adopted",
  ];
  const rows = terms.map((t) => [
    t.source_text,
    t.surface_ja,
    t.search_text ?? "",
    t.generic_suffix ?? "",
    (t.aliases ?? []).join(" | "),
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
    t.webResult?.evidence_strength ?? "",
    t.webResult?.match_judgment ?? "",
    t.webResult ? deriveNeedsHumanReview(t.webResult) : "",
    t.webResult?.confidence_reason ?? "",
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
    batchAdoptTerms,
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
  const [chatGptPasteText, setChatGptPasteText] = useState("");
  const [pasteError, setPasteError] = useState<string | null>(null);
  const [showBatchApi, setShowBatchApi] = useState(false);
  const [expandedEvidence, setExpandedEvidence] = useState<Set<string>>(new Set());

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

  // Known-title map: when folderLabel is detected but zh/en are missing, fill from this table.
  // TODO: replace with per-project title editing in Settings/DramaInfo UI.
  const KNOWN_TITLES: Record<string, { zh: string; en: string }> = {
    "氷湖重生": { zh: "冰湖重生", en: "Rebirth" },
  };

  // Resolve all title/label fields:
  //   ja:   official Japanese title (metadata.drama_title). Never from folder name.
  //   zh:   Chinese title (metadata.search_title_zh → known-title map → "")
  //   en:   English title (metadata.search_title_en → known-title map → "")
  //   folderLabel: baseDir parent folder name (work label, never auto-promoted to ja)
  const resolveDramaTitle = (): { ja: string; zh: string; en: string; folderLabel: string; source: string } => {
    let ja = dramaTitleJa;
    let zh = dramaTitleZh;
    let en = dramaTitleEn;
    let folderLabel = "";
    let source = "DramaInfo";

    // Extract folder label from baseDir (for known-title lookup, never for ja)
    if (projectBaseDir) {
      const sep = projectBaseDir.includes("\\") ? "\\" : "/";
      const parts = projectBaseDir.split(sep).filter(Boolean);
      if (parts.length >= 2) {
        const parent = parts[parts.length - 2];
        if (parent && parent.length > 0) folderLabel = parent;
      }
    }

    // Fill zh/en from known-title map (keyed by folderLabel)
    if (folderLabel && KNOWN_TITLES[folderLabel]) {
      if (!zh) zh = KNOWN_TITLES[folderLabel].zh;
      if (!en) en = KNOWN_TITLES[folderLabel].en;
      if (!ja && !dramaTitleZh && !dramaTitleEn) {
        source = "known-title-map";
      } else if (KNOWN_TITLES[folderLabel].zh === zh || KNOWN_TITLES[folderLabel].en === en) {
        source = source === "DramaInfo" ? "DramaInfo" : `DramaInfo+known-title-map`;
      }
    }

    // Also try known-title map by ja
    if (ja && KNOWN_TITLES[ja]) {
      if (!zh) zh = KNOWN_TITLES[ja].zh;
      if (!en) en = KNOWN_TITLES[ja].en;
    }

    return { ja, zh, en, folderLabel, source };
  };

  const persistCurrentFile = async () => {
    if (!activeFile || !folderPath) return;
    const f = useSrtStore.getState().files.find((x) => x.path === activeFile.path);
    if (!f) return;
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
  };

  // --- AI term resolution handler ---
  const handleAiConfirm = async (term: UnresolvedTerm) => {
    if (!activeFile) return;
    setTermLoading(activeFile.path, term.source_text, true);
    try {
      const result = await resolveUnresolvedTermAiOpenai({
        source_text: term.source_text,
        surface_ja: term.surface_ja,
        drama_title: resolveDramaTitle().ja || undefined,
        prompt_context: promptContext,
        srt_filename: activeFile.name,
      });
      setTermWebResult(activeFile.path, term.source_text, result);
      console.log(`[SRT] AI確認: ${term.source_text} → ${result.candidate_zh ?? "なし"} (${result.confidence})`);
      persistCurrentFile();
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
      persistCurrentFile();
    } finally {
      setTermLoading(activeFile.path, term.source_text, false);
    }
  };

  // --- ChatGPT paste handlers ---
  const handleCopyPrompt = async () => {
    if (!activeFile || !activeFile.synopsis) return;
    const pending = activeFile.unresolvedTerms.filter((t) => !t.adopted && !t.webResult);
    if (pending.length === 0) {
      useAppLogStore.getState().addLog("warn", "ChatGPT", "コピー対象の未確認語がありません");
      return;
    }
    const batchTerms: BatchTermRequest[] = pending.map((t) => ({
      source_text: t.source_text,
      surface_ja: t.surface_ja,
      aliases: t.aliases,
    }));
    const resolvedTitle = resolveDramaTitle();
    const promptText = buildBatchPrompt({
      terms: batchTerms,
      drama_title_zh: resolvedTitle.zh,
      drama_title_en: resolvedTitle.en,
      drama_title_ja: resolvedTitle.ja,
      folder_label: resolvedTitle.folderLabel,
      srt_filename: activeFile.name,
      short_context: shortContext,
    });
    try {
      await navigator.clipboard.writeText(promptText);
      useAppLogStore.getState().addLog("info", "ChatGPT", `プロンプトをクリップボードにコピーしました (${batchTerms.length}件)`);
    } catch {
      useAppLogStore.getState().addLog("error", "ChatGPT", "クリップボードへのコピーに失敗しました");
    }
  };

  const handlePasteChatGptResponse = async () => {
    if (!activeFile || !activeFile.synopsis) return;
    if (!chatGptPasteText.trim()) {
      setPasteError("ChatGPTの回答を貼り付けてください。");
      return;
    }
    setPasteError(null);
    const pending = activeFile.unresolvedTerms.filter((t) => !t.adopted && !t.webResult);
    const batchTerms: BatchTermRequest[] = pending.map((t) => ({
      source_text: t.source_text,
      surface_ja: t.surface_ja,
      aliases: t.aliases,
    }));
    try {
      const { results, warnings } = parseChatGptResponse(chatGptPasteText, batchTerms);
      for (const w of warnings) {
        useAppLogStore.getState().addLog("warn", "ChatGPT", w);
      }
      const store = useSrtStore.getState();
      store.setBatchTermResults(activeFile.path, results);
      const foundCount = results.filter((r) => r.status === "found").length;
      const notFoundCount = results.filter((r) => r.status === "not_found").length;
      useAppLogStore.getState().addLog("info", "ChatGPT", `貼り付け結果: ${foundCount}件候補あり, ${notFoundCount}件見つからず (${results.length}件中)`);
      setChatGptPasteText("");
      await persistCurrentFile();
    } catch (e: any) {
      setPasteError(e.message ?? String(e));
      useAppLogStore.getState().addLog("error", "ChatGPT", `解析失敗: ${e.message ?? String(e)}`);
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
      aliases: t.aliases,
    }));
    const store = useSrtStore.getState();
    store.setBatchTermLoading(activeFile.path, true);
    try {
      const resolvedTitle = resolveDramaTitle();
      console.log(`[SRT] Batch title resolved on frontend: ja="${resolvedTitle.ja}", zh="${resolvedTitle.zh}", en="${resolvedTitle.en}", folderLabel="${resolvedTitle.folderLabel}", source=${resolvedTitle.source}`);
      const results = await resolveUnresolvedTermsBatchOpenai({
        terms: batchTerms,
        drama_title_ja: resolvedTitle.ja,
        drama_title_zh: resolvedTitle.zh || null,
        drama_title_en: resolvedTitle.en || null,
        folder_label: resolvedTitle.folderLabel || null,
        short_context: shortContext || null,
        srt_filename: activeFile.name || null,
      });
      store.setBatchTermResults(activeFile.path, results);
      console.log(`[SRT] 一括AI確認: ${results.length}件解決`);
      persistCurrentFile();
    } catch (e) {
      const msg = String(e);
      console.error("[SRT] 一括AI確認エラー:", e);
      setError(`一括AI確認エラー: ${msg}`);
      store.setBatchTermLoading(activeFile.path, false);
    }
  };

  /** Build a GlossaryEntry from an UnresolvedTerm using the shared target resolution.
   *  Returns null when no valid target (candidate_ja > candidate_zh > surface_ja all empty). */
  const buildGlossaryEntry = (term: UnresolvedTerm, opts?: {
    notes?: string;
    includeMeta?: boolean;
  }): { entry: GlossaryEntry; target: string } | null => {
    const target = term.webResult?.candidate_ja ?? term.webResult?.candidate_zh ?? term.surface_ja;
    if (!target) return null;
    const aliases = term.webResult?.alternatives?.length
      ? term.webResult.alternatives.filter((a) => a !== target)
      : undefined;
    const entry: GlossaryEntry = {
      source: term.source_text,
      target,
      type: term.term_type || "proper_noun",
      notes: opts?.notes || "AI確認採用",
      aliases,
    };
    if (opts?.includeMeta) {
      const wr = term.webResult;
      if (wr) {
        entry.status = wr.status;
        entry.confidence = wr.confidence;
        entry.evidence_urls = extractCleanUrls(wr.evidence?.map((e) => e.url));
      }
    }
    return { entry, target };
  };

  const handleAdoptTerm = (term: UnresolvedTerm) => {
    if (!activeFile) return;
    const built = buildGlossaryEntry(term, { notes: "AI確認採用", includeMeta: true });
    if (!built) {
      useAppLogStore.getState().addLog("warn", "SRT", `採用スキップ (候補なし): ${term.source_text}`);
      return;
    }
    const displayJa = term.webResult?.candidate_ja ?? term.webResult?.candidate_zh ?? term.surface_ja ?? "";
    const confirmedSurface = term.webResult?.candidate_ja ?? term.webResult?.candidate_zh ?? term.surface_ja ?? term.source_text;
    batchAdoptTerms(activeFile.path, [
      { sourceText: term.source_text, entry: built.entry, surfaceJa: displayJa, confirmedSurface },
    ]);
    // Also persist to shared glossary.json
    if (projectBaseDir) {
      appendToGlossary(projectBaseDir, [built.entry]);
    }
    console.log(`[SRT] 採用: ${term.source_text} → ${built.target}`);
    persistCurrentFile();
  };

  const handleDismissTerm = (term: UnresolvedTerm) => {
    if (!activeFile) return;
    removeUnresolvedTerm(activeFile.path, term.source_text);
    console.log(`[SRT] 無視: ${term.source_text}`);
    persistCurrentFile();
  };

  // --- Manual input modal state ---
  const [manualTarget, setManualTarget] = useState<UnresolvedTerm | null>(null);
  const [manualZh, setManualZh] = useState("");
  const [manualJa, setManualJa] = useState("");
  const [manualType, setManualType] = useState("proper_noun");
  const [manualNote, setManualNote] = useState("");

  const openManualInput = (term: UnresolvedTerm) => {
    setManualTarget(term);
    setManualZh("");
    setManualJa("");
    setManualType("proper_noun");
    setManualNote("");
  };

  const handleManualAdopt = () => {
    if (!activeFile || !manualTarget) return;
    const zh = manualZh.trim();
    const ja = manualJa.trim();
    if (!zh && !ja) return;
    const target = zh || ja;
    const entry: GlossaryEntry = {
      source: manualTarget.source_text,
      target,
      type: manualType || "proper_noun",
      notes: manualNote || "手入力採用",
    };
    batchAdoptTerms(activeFile.path, [
      { sourceText: manualTarget.source_text, entry },
    ]);
    // Also persist to glossary.json
    if (projectBaseDir) {
      appendToGlossary(projectBaseDir, [entry]);
    }
    useAppLogStore.getState().addLog("info", "SRT",
      `手入力採用: ${manualTarget.source_text} → zh="${zh}" ja="${ja}" type="${manualType}"`);
    setManualTarget(null);
    persistCurrentFile();
  };

  // --- Batch adopt high-confidence terms ---
  const handleBatchAdoptHighConfidence = () => {
    if (!activeFile) return;
    const items: { sourceText: string; entry: GlossaryEntry; surfaceJa?: string; confirmedSurface?: string }[] = [];
    for (const t of activeFile.unresolvedTerms) {
      if (t.adopted) continue;
      const wr = t.webResult;
      if (!wr) continue;
      if (wr.status !== "found") continue;
      if (wr.confidence !== "high") continue;
      if (!wr.candidate_zh && !wr.candidate_ja) continue;
      if (!wr.evidence || wr.evidence.length === 0) continue;
      if (wr.source_text === wr.candidate_zh || wr.source_text === wr.candidate_ja) continue;
      const built = buildGlossaryEntry(t, { notes: "AI確認採用（一括）", includeMeta: true });
      if (!built) continue;
      const displayJa = wr.candidate_ja ?? wr.candidate_zh ?? t.surface_ja ?? "";
      const confirmedSurface = wr.candidate_ja ?? wr.candidate_zh ?? t.surface_ja ?? t.source_text;
      items.push({
        sourceText: t.source_text,
        entry: built.entry,
        surfaceJa: displayJa,
        confirmedSurface,
      });
    }
    if (items.length === 0) {
      useAppLogStore.getState().addLog("info", "SRT", "一括採用できる高確度候補がありませんでした。");
      return;
    }
    batchAdoptTerms(activeFile.path, items);
    // Also persist to shared glossary.json
    if (projectBaseDir) {
      const entries = items.map((it) => it.entry);
      appendToGlossary(projectBaseDir, entries);
    }
    const remaining = activeFile.unresolvedTerms.filter((t) => !t.adopted).length - items.length;
    useAppLogStore.getState().addLog("success", "SRT",
      `高確度候補 ${items.length}件を一括採用しました。未採用: ${Math.max(0, remaining)}件`);
    persistCurrentFile();
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

      // --- Resolve baseDir and load dictionaries + drama info ---
      // Priority: 1) useProjectStore baseDir, 2) derive from SRT folder path.
      let baseDir: string | null = null;
      {
        const projStore = useProjectStore.getState();
        if (projStore.baseDir) {
          baseDir = projStore.baseDir;
          useAppLogStore.getState().addLog("info", "SRT", `useProjectStore.baseDir から試行: ${baseDir}`);
        }
      }
      if (!baseDir) {
        baseDir = await deriveBaseDir(dirPath);
      }

      // Load dictionaries (if not already in store)
      const dictStore = useDictionaryStore.getState();
      if (dictStore.characters.length > 0 || dictStore.glossary.length > 0) {
        setDictStatus(`OK (人物${dictStore.characters.length}人, 用語${dictStore.glossary.length}件)`);
        useAppLogStore.getState().addLog("info", "SRT", "辞書は既に読み込み済みです");
      } else if (baseDir) {
        setDictStatus("検索中...");
        await loadDictionariesFromDir(baseDir, setCharacters, setGlossary, setProject, setDictStatus);
      } else {
        setDictStatus("辞書フォルダ未検出");
        useAppLogStore.getState().addLog("warn", "SRT", `project base_dir not derived. dict not found within 3 levels of: ${dirPath}`);
      }

      // Load drama info for batch AI確認 context (always, not gated on dict load).
      // Try baseDir first, then parent (dictionaries may be in a subfolder while drama_info/ is in the parent).
      if (baseDir) {
        try {
          let dramaInfo = await loadDramaInfo(baseDir);
          let infoSource = baseDir;
          if (!dramaInfo?.metadata?.drama_title && !dramaInfo?.metadata?.search_title_zh) {
            // Try parent directory
            const sep = baseDir.includes("\\") ? "\\" : "/";
            const parts = baseDir.split(sep).filter(Boolean);
            if (parts.length >= 2) {
              const parentDir = parts.slice(0, -1).join(sep);
              const absParent = parentDir.startsWith(sep) ? parentDir : sep + parentDir;
              dramaInfo = await loadDramaInfo(absParent);
              if (dramaInfo?.metadata) {
                infoSource = absParent;
              }
            }
          }
          const zh = dramaInfo?.metadata?.search_title_zh ?? "";
          const en = dramaInfo?.metadata?.search_title_en ?? "";
          const rawTitle = dramaInfo?.metadata?.drama_title ?? "";
          // drama_title is a generic field; only treat it as a Japanese title
          // if it contains kana/kanji. ASCII-only values like "Rebirth" are
          // English titles mistakenly stored in the generic field.
          const ja = /\p{Script=Hiragana}|\p{Script=Katakana}|\p{Script=Han}/u.test(rawTitle)
            ? rawTitle : "";
          const ctx = dramaInfo?.synopsis_summary?.translation_context_short_ja ?? "";
          setDramaTitleZh(zh);
          setDramaTitleEn(en);
          setDramaTitleJa(ja);
          setShortContext(ctx);
          useAppLogStore.getState().addLog("debug", "SRT",
            `DramaInfo loaded from ${infoSource}: ja="${ja}" zh="${zh}" en="${en}"`);
        } catch {
          useAppLogStore.getState().addLog("debug", "SRT", "DramaInfo not found (no drama_info/ directory)");
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

  /** Shared core: adopt terms → persist → reload dicts → regenerate synopsis → clear.
   *  Used by both batch-adopt-and-regenerate buttons. */
  const saveAdoptedTermsAndRegenerate = async (
    termsToAdopt: UnresolvedTerm[],
    reason: string,
    logPrefix: string,
  ) => {
    if (!activeFile || !projectBaseDir || !hasLoaded) return;
    setLoadingSynop(true);
    setError(null);
    const log = useAppLogStore.getState().addLog;

    try {
      // Step 1: Filter & build GlossaryEntry items with target determination
      const items: { sourceText: string; entry: GlossaryEntry; surfaceJa?: string; confirmedSurface?: string }[] = [];
      let skippedEmpty = 0;
      for (const t of termsToAdopt) {
        if (t.adopted) continue;
        const built = buildGlossaryEntry(t, { notes: reason, includeMeta: true });
        if (!built) {
          skippedEmpty++;
          log("warn", "SRT", `採用スキップ (glossaryTarget 空): ${t.source_text}`);
          continue;
        }
        const displayJa = t.webResult?.candidate_ja ?? t.webResult?.candidate_zh ?? t.surface_ja ?? "";
        const confirmedSurface = t.webResult?.candidate_ja ?? t.webResult?.candidate_zh ?? t.surface_ja ?? t.source_text;
        items.push({
          sourceText: t.source_text,
          entry: built.entry,
          surfaceJa: displayJa,
          confirmedSurface,
        });
      }

      if (items.length === 0) {
        log("info", "SRT", skippedEmpty > 0
          ? `採用対象の候補がありません (${skippedEmpty}件はglossaryTarget空でスキップ)。`
          : "採用対象の候補がありません。");
        return;
      }

      // Step 2: Mark adopted + update display fields in store
      batchAdoptTerms(activeFile.path, items);

      // Step 3: Collect alias candidate terms from our batch
      const adoptedTextSet = new Set(items.map((it) => it.sourceText));
      const aliasCandidateTerms = termsToAdopt.filter(
        (t) => adoptedTextSet.has(t.source_text) && t.term_type === "alias_candidate",
      );
      const dictChars = useDictionaryStore.getState().characters;
      const entries = items.map((it) => it.entry);

      // Step 4: Save to glossary.json + characters.json
      const { glossaryAdded, charactersAliasAdded } = await batchSaveAdoptedTerms(
        projectBaseDir,
        entries,
        aliasCandidateTerms,
        dictChars,
      );
      const saveParts: string[] = [];
      if (glossaryAdded > 0) saveParts.push(`added=${glossaryAdded}`);
      if (charactersAliasAdded > 0) saveParts.push(`aliasMerged=${charactersAliasAdded}`);
      log("success", "SRT", `用語集に保存しました: ${saveParts.join(", ") || "全件既存"}`);

      // Step 5: Reload dictionaries from disk
      const { chars, gloss } = await loadDictionariesFromDir(
        projectBaseDir,
        setCharacters,
        setGlossary,
        setProject,
        setDictStatus,
      );
      log("success", "SRT", `辞書を再読み込みしました (characters ${chars}件, glossary ${gloss}件)`);

      // Step 6: Snapshot pre-regeneration count
      const beforeCount = activeFile.unresolvedTerms.length;

      // Step 7: Re-run synopsis with updated dictionaries
      const updatedPromptContext = buildPromptContext(
        useDictionaryStore.getState().characters,
        useDictionaryStore.getState().glossary,
        activeFile.katakanaMap,
        activeFile.termVariants,
        activeFile.unresolvedTerms,
        activeFile.adoptedTerms,
      );
      let result = await generateSrtSynopsis(activeFile.entries, updatedPromptContext);

      // Step 8: Katakana resolution
      let katakanaMap: KatakanaKanjiMap[] = [];
      try {
        katakanaMap = await resolveSynopsisKatakana(
          result.synopsis_ja,
          result.unresolved_terms ?? [],
        );
        for (const m of katakanaMap) {
          if (m.status === "resolved" && m.kanji) {
            result = {
              ...result,
              synopsis_ja: result.synopsis_ja.split(m.katakana).join(m.kanji),
            };
          }
        }
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
      } catch (_) { /* proceed with raw text */ }

      const termVariants = result.term_variants ?? [];

      // Step 9: Extract body candidates, merge
      let bodyTerms: UnresolvedTerm[] = [];
      try {
        const dictForBody = useDictionaryStore.getState();
        bodyTerms = await extractSrtBodyCandidates(
          activeFile.entries,
          dictForBody.characters,
          dictForBody.glossary,
        );
      } catch (_) { /* best-effort */ }

      const synopsisTerms = (result.unresolved_terms ?? []).map((t) => ({
        ...t,
        source: t.source || "synopsis",
        occurrence_count: t.occurrence_count || 0,
      }));
      const merged = mergeUnresolvedTerms(synopsisTerms, bodyTerms);

      const rawUnresolved = merged.map((t) => ({
        ...t,
        surface_ja: /[々〆〡-〩぀-ゟ゠-ヿ一-鿿㐀-䶿]/.test(t.surface_ja) ? t.surface_ja : "",
      }));

      // Step 10: Filter against updated dictionaries
      const dictState = useDictionaryStore.getState();
      const { filtered: newUnresolvedTerms, removedCount } = filterUnresolvedByDict(
        rawUnresolved,
        dictState.characters,
        dictState.glossary,
      );

      // Step 11: Clear adoptedTerms
      useSrtStore.getState().setFileAdoptedTerms(activeFile.path, []);
      log("success", "SRT", `採用済み語をクリアしました (${items.length}件)`);

      // Step 12: Save regenerated state
      setFileSynopsis(activeFile.path, result, katakanaMap, termVariants, newUnresolvedTerms);
      {
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
            unresolved_terms: newUnresolvedTerms,
            adopted_terms: [],
          });
        }
      }

      // Step 13: Log results
      log("success", "SRT", `2.1 あらすじを再生成しました。`);
      const afterCount = newUnresolvedTerms.length;
      const reduction = beforeCount - afterCount;
      if (reduction > 0) {
        log("success", "SRT", `再生成完了: unresolved_terms ${beforeCount}件 → ${afterCount}件 (${reduction}件削減)`);
      } else {
        log("success", "SRT", `再生成完了: unresolved_terms ${beforeCount}件 → ${afterCount}件 (変化なし)`);
      }
      if (removedCount > 0) {
        log("success", "SRT", `辞書フィルタで ${removedCount}件の既知語を除外しました`);
      }

      // Step 14: Auto re-run scene detection if previously run
      if (activeFile.sceneDetection) {
        log("info", "SRT", "場面検出を再実行します（辞書更新に伴う再生成）");
        try {
          const f2 = useSrtStore.getState().files.find((x) => x.path === activeFile.path);
          if (f2) {
            const ctx2 = buildPromptContext(
              useDictionaryStore.getState().characters,
              useDictionaryStore.getState().glossary,
              f2.katakanaMap,
              f2.termVariants,
              f2.unresolvedTerms,
              f2.adoptedTerms,
            );
            const sceneResult = await detectSrtScenes(f2.entries, ctx2);
            for (const scene of sceneResult.scenes) {
              scene.title = resolveKnownTermsInText(scene.title, useDictionaryStore.getState().characters, useDictionaryStore.getState().glossary);
              scene.reason = resolveKnownTermsInText(scene.reason, useDictionaryStore.getState().characters, useDictionaryStore.getState().glossary);
            }
            setFileSceneDetection(activeFile.path, sceneResult);
            const f3 = useSrtStore.getState().files.find((x) => x.path === activeFile.path);
            if (f3 && folderPath) {
              await saveSrtAnalysis({
                srt_path: f3.path,
                srt_name: f3.name,
                synopsis: f3.synopsis,
                scene_detection: sceneResult,
                scene_contexts: Object.values(f3.sceneContexts),
                katakana_map: f3.katakanaMap,
                term_variants: f3.termVariants,
                unresolved_terms: f3.unresolvedTerms,
                adopted_terms: f3.adoptedTerms,
              });
            }
            log("success", "SRT", `場面検出を再生成しました (${sceneResult.scenes.length}場面)`);
          }
        } catch (sceneErr) {
          log("warn", "SRT", `場面検出の再実行に失敗しました: ${String(sceneErr)}`);
        }
      }
    } catch (e) {
      log("error", "SRT", `${logPrefix}エラー: ${String(e)}`);
      setError(String(e));
    } finally {
      setLoadingSynop(false);
    }
  };

  // --- Batch adopt & regenerate: high confidence only ---
  const handleBatchAdoptHighAndRegen = async () => {
    if (!activeFile || !projectBaseDir || !hasLoaded) return;
    const termsToAdopt: UnresolvedTerm[] = [];
    for (const t of activeFile.unresolvedTerms) {
      const wr = t.webResult;
      if (!wr || t.adopted || !wr.candidate_zh && !wr.candidate_ja) continue;
      if (wr.status !== "found" || wr.confidence !== "high") continue;
      if (wr.evidence_strength !== "direct" || wr.match_judgment !== "exact") continue;
      if (deriveNeedsHumanReview(wr) !== false) continue;
      termsToAdopt.push(t);
    }
    if (termsToAdopt.length === 0) {
      useAppLogStore.getState().addLog("info", "SRT", "採用対象の候補がありません。");
      return;
    }
    const log = useAppLogStore.getState().addLog;
    log("info", "SRT", `高確度候補 ${termsToAdopt.length}件を採用します (表示フィールドを更新しました)。`);
    await saveAdoptedTermsAndRegenerate(termsToAdopt, "AI確認採用（高確度一括）", "高確度一括採用・再生成");
  };

  // --- Batch adopt & regenerate: all found terms ---
  const handleBatchAdoptAllAndRegen = async () => {
    if (!activeFile || !projectBaseDir || !hasLoaded) return;
    const termsToAdopt: UnresolvedTerm[] = [];
    for (const t of activeFile.unresolvedTerms) {
      const wr = t.webResult;
      if (!wr || t.adopted || !wr.candidate_zh && !wr.candidate_ja) continue;
      if (wr.status !== "found") continue;
      termsToAdopt.push(t);
    }
    if (termsToAdopt.length === 0) {
      useAppLogStore.getState().addLog("info", "SRT", "採用対象の候補がありません。");
      return;
    }
    if (!window.confirm(
      `found の候補 ${termsToAdopt.length}件をすべて採用し、用語集に保存して 2.1 あらすじを再生成します。\n要確認・低確度の候補も含まれる可能性があります。\n実行しますか？`
    )) return;
    const log = useAppLogStore.getState().addLog;
    log("info", "SRT", `found候補 ${termsToAdopt.length}件を一括採用しました (表示フィールドを更新しました)。`);
    log("warn", "SRT", "低確度・要確認候補を含む可能性があります。");
    await saveAdoptedTermsAndRegenerate(termsToAdopt, "AI確認採用（全件一括）", "全件一括採用・再生成");
  };

  const handleRegenerateWithDict = async () => {
    if (!activeFile || !projectBaseDir || !hasLoaded) return;
    setLoadingSynop(true);
    setError(null);
    const log = useAppLogStore.getState().addLog;

    try {
      const adoptedTerms = activeFile.adoptedTerms;
      if (adoptedTerms.length === 0) {
        log("warn", "SRT", "採用済み用語がありません。辞書保存をスキップします。");
        return;
      }

      // Filter alias_candidate terms from the active file's unresolved terms
      const aliasCandidateTerms = activeFile.unresolvedTerms.filter(
        (t) => t.adopted && t.term_type === "alias_candidate",
      );
      const dictChars = useDictionaryStore.getState().characters;

      // Phase 1: Save adopted terms to glossary.json + characters.json
      const { glossaryAdded, charactersAliasAdded } = await batchSaveAdoptedTerms(
        projectBaseDir,
        adoptedTerms,
        aliasCandidateTerms,
        dictChars,
      );
      const saveParts: string[] = [];
      if (glossaryAdded > 0) saveParts.push(`用語集 ${glossaryAdded}件`);
      if (charactersAliasAdded > 0) saveParts.push(`人物別名 ${charactersAliasAdded}件`);
      if (saveParts.length > 0) {
        log("success", "SRT", `辞書保存: ${saveParts.join(", ")}`);
      } else {
        log("info", "SRT", "辞書保存: 全件既存のため追加なし");
      }

      // Phase 2: Reload dictionaries from disk (updates frontend store + Rust in-memory state)
      const { chars, gloss } = await loadDictionariesFromDir(
        projectBaseDir,
        setCharacters,
        setGlossary,
        setProject,
        setDictStatus,
      );
      log("success", "SRT", `辞書再読み込み: characters ${chars}件, glossary ${gloss}件`);

      // Phase 3: Snapshot pre-regeneration unresolved count
      const beforeCount = activeFile.unresolvedTerms.length;
      log("info", "SRT", `再生成前 unresolved_terms: ${beforeCount}件`);

      // Phase 4: Re-run synopsis generation with updated dictionaries
      const updatedPromptContext = buildPromptContext(
        useDictionaryStore.getState().characters,
        useDictionaryStore.getState().glossary,
        activeFile.katakanaMap,
        activeFile.termVariants,
        activeFile.unresolvedTerms,
        activeFile.adoptedTerms,
      );

      let result = await generateSrtSynopsis(activeFile.entries, updatedPromptContext);

      // Katakana resolution (same as handleSynopsis)
      let katakanaMap: KatakanaKanjiMap[] = [];
      try {
        katakanaMap = await resolveSynopsisKatakana(
          result.synopsis_ja,
          result.unresolved_terms ?? [],
        );
        for (const m of katakanaMap) {
          if (m.status === "resolved" && m.kanji) {
            result = {
              ...result,
              synopsis_ja: result.synopsis_ja.split(m.katakana).join(m.kanji),
            };
          }
        }
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
      } catch (_) { /* proceed with raw text */ }

      const termVariants = result.term_variants ?? [];

      // Extract body candidates with updated dictionaries
      let bodyTerms: UnresolvedTerm[] = [];
      try {
        const dictForBody = useDictionaryStore.getState();
        bodyTerms = await extractSrtBodyCandidates(
          activeFile.entries,
          dictForBody.characters,
          dictForBody.glossary,
        );
      } catch (_) { /* best-effort */ }

      // Merge synopsis + body terms
      const synopsisTerms = (result.unresolved_terms ?? []).map((t) => ({
        ...t,
        source: t.source || "synopsis",
        occurrence_count: t.occurrence_count || 0,
      }));
      const merged = mergeUnresolvedTerms(synopsisTerms, bodyTerms);

      const rawUnresolved = merged.map((t) => ({
        ...t,
        surface_ja: /[々〆〡-〩぀-ゟ゠-ヿ一-鿿㐀-䶿]/.test(t.surface_ja) ? t.surface_ja : "",
      }));

      // Filter against UPDATED dictionaries
      const dictState = useDictionaryStore.getState();
      const { filtered: newUnresolvedTerms, removedCount } = filterUnresolvedByDict(
        rawUnresolved,
        dictState.characters,
        dictState.glossary,
      );

      // Phase 5: Clear adoptedTerms (saved to dict, no longer needed)
      useSrtStore.getState().setFileAdoptedTerms(activeFile.path, []);
      log("success", "SRT", `採用済み語をクリアしました (${adoptedTerms.length}件)`);

      // Phase 6: Save regenerated state (adopted_terms cleared)
      setFileSynopsis(activeFile.path, result, katakanaMap, termVariants, newUnresolvedTerms);
      {
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
            unresolved_terms: newUnresolvedTerms,
            adopted_terms: [],
          });
        }
      }

      // Phase 7: Log regeneration stats
      const afterCount = newUnresolvedTerms.length;
      const reduction = beforeCount - afterCount;
      if (reduction > 0) {
        log("success", "SRT", `再生成完了: unresolved_terms ${beforeCount}件 → ${afterCount}件 (${reduction}件削減)`);
      } else {
        log("success", "SRT", `再生成完了: unresolved_terms ${beforeCount}件 → ${afterCount}件 (変化なし)`);
      }
      if (removedCount > 0) {
        log("success", "SRT", `辞書フィルタで ${removedCount}件の既知語を除外しました`);
      }

      // Phase 8: Auto re-run scene detection if previously run
      if (activeFile.sceneDetection) {
        log("info", "SRT", "場面検出を再実行します（辞書更新に伴う再生成）");
        try {
          const f2 = useSrtStore.getState().files.find((x) => x.path === activeFile.path);
          if (f2) {
            const ctx2 = buildPromptContext(
              useDictionaryStore.getState().characters,
              useDictionaryStore.getState().glossary,
              f2.katakanaMap,
              f2.termVariants,
              f2.unresolvedTerms,
              f2.adoptedTerms,
            );
            const sceneResult = await detectSrtScenes(f2.entries, ctx2);
            for (const scene of sceneResult.scenes) {
              scene.title = resolveKnownTermsInText(scene.title, useDictionaryStore.getState().characters, useDictionaryStore.getState().glossary);
              scene.reason = resolveKnownTermsInText(scene.reason, useDictionaryStore.getState().characters, useDictionaryStore.getState().glossary);
            }
            setFileSceneDetection(activeFile.path, sceneResult);
            const f3 = useSrtStore.getState().files.find((x) => x.path === activeFile.path);
            if (f3 && folderPath) {
              await saveSrtAnalysis({
                srt_path: f3.path,
                srt_name: f3.name,
                synopsis: f3.synopsis,
                scene_detection: sceneResult,
                scene_contexts: Object.values(f3.sceneContexts),
                katakana_map: f3.katakanaMap,
                term_variants: f3.termVariants,
                unresolved_terms: f3.unresolvedTerms,
                adopted_terms: f3.adoptedTerms,
              });
            }
            log("success", "SRT", `場面検出を再生成しました (${sceneResult.scenes.length}場面)`);
          }
        } catch (sceneErr) {
          log("warn", "SRT", `場面検出の再実行に失敗しました: ${String(sceneErr)}`);
        }
      }
    } catch (e) {
      log("error", "SRT", `辞書更新後再生成エラー: ${String(e)}`);
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
      // Belt-and-suspenders: apply dictionary replacements client-side
      for (const scene of result.scenes) {
        scene.title = resolveKnownTermsInText(scene.title, characters, glossary);
        scene.reason = resolveKnownTermsInText(scene.reason, characters, glossary);
      }
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
        // Apply dictionary normalization to context fields
        const chars = useDictionaryStore.getState().characters;
        const glos = useDictionaryStore.getState().glossary;
        result.context_ja = resolveKnownTermsInText(result.context_ja, chars, glos);
        if (result.hierarchy) result.hierarchy = resolveKnownTermsInText(result.hierarchy, chars, glos);
        result.gender_notes = result.gender_notes.map((g) => resolveKnownTermsInText(g, chars, glos));
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
                {activeFile?.adoptedTerms && activeFile.adoptedTerms.length > 0 && (
                  <button
                    className="btn"
                    onClick={handleRegenerateWithDict}
                    disabled={!hasLoaded || loadingSynop || !projectBaseDir}
                    style={{
                      fontSize: 12,
                      background: "linear-gradient(135deg, #7c3aed, #a855f7)",
                      color: "#fff",
                      border: "none",
                    }}
                  >
                    <RefreshCw size={14} />
                    採用語を辞書保存して再生成 ({activeFile.adoptedTerms.length}件採用済)
                  </button>
                )}
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
                        {/* ChatGPT paste — primary flow */}
                        <div className="srt-chatgpt-paste-panel">
                          <div className="srt-chatgpt-paste-panel-heading">
                            ChatGPT連携
                          </div>
                          <p className="srt-chatgpt-paste-panel-desc">
                            1. プロンプトをコピーしてChatGPTに貼り付け → 2. 回答を下に貼り付けて「結果を取り込む」
                          </p>
                          <div className="srt-chatgpt-paste-btn-row">
                            {pending.filter((t) => !t.adopted && !t.webResult).length > 0 && (
                              <button
                                className="btn btn-primary"
                                onClick={handleCopyPrompt}
                                style={{ fontSize: 12 }}
                              >
                                プロンプトをコピー ({pending.filter((t) => !t.adopted && !t.webResult).length}件)
                              </button>
                            )}
                          </div>
                          <textarea
                            value={chatGptPasteText}
                            onChange={(e) => { setChatGptPasteText(e.target.value); setPasteError(null); }}
                            placeholder="ChatGPTの回答をここに貼り付け..."
                            rows={4}
                            className={`srt-chatgpt-paste-textarea${pasteError ? " has-error" : ""}`}
                          />
                          <div style={{ display: "flex", gap: 8, alignItems: "center", marginTop: 8 }}>
                            <button
                              className="btn btn-primary"
                              onClick={handlePasteChatGptResponse}
                              disabled={!chatGptPasteText.trim()}
                              style={{ fontSize: 12 }}
                            >
                              結果を取り込む
                            </button>
                            {pasteError && (
                              <span className="srt-chatgpt-paste-error">{pasteError}</span>
                            )}
                          </div>
                        </div>

                        {/* Accordion: API batch AI confirm (optional, collapsed by default) */}
                        <details open={showBatchApi} onToggle={(e) => setShowBatchApi((e.target as HTMLDetailsElement).open)} style={{ marginBottom: 8 }}>
                          <summary style={{ fontSize: 12, cursor: "pointer", color: "var(--text-secondary)", userSelect: "none" }}>
                            APIでAI確認 (オプション)
                          </summary>
                          <div style={{ marginTop: 6, display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
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
                            {/* Batch adopt high-confidence button */}
                            {(() => {
                              const highCount = pending.filter((t) => {
                                const wr = t.webResult;
                                if (!wr) return false;
                                if (wr.status !== "found") return false;
                                if (wr.confidence !== "high") return false;
                                if (!wr.candidate_zh && !wr.candidate_ja) return false;
                                if (!wr.evidence || wr.evidence.length === 0) return false;
                                if (wr.source_text === wr.candidate_zh || wr.source_text === wr.candidate_ja) return false;
                                return true;
                              }).length;
                              if (highCount === 0) return null;
                              return (
                                <button
                                  className="btn btn-sm"
                                  onClick={handleBatchAdoptHighConfidence}
                                  style={{ fontSize: 12, background: "var(--success, #22c55e)", color: "#fff" }}
                                >
                                  found/high を一括採用 ({highCount}件)
                                </button>
                              );
                            })()}
                          </div>
                        </details>
                        <div className="table-container" style={{ maxHeight: 300, overflowY: "auto" }}>
                          <table>
                            <thead>
                              <tr>
                                <th style={{ width: "13%" }}>元の表記</th>
                                <th style={{ width: 80 }}>日本語候補</th>
                                <th style={{ width: 90 }}>検索候補</th>
                                <th style={{ width: "10%" }}>確定表記</th>
                                <th style={{ width: 90, textAlign: "center" }}>判定</th>
                                <th style={{ maxWidth: 260 }}>理由</th>
                                <th style={{ width: 140, textAlign: "center" }}>操作</th>
                              </tr>
                            </thead>
                            <tbody>
                              {pending.map((t, index) => {
                                const isLoading = activeFile.termLoading[t.source_text] ?? false;
                                const hasCandidate = (t.webResult?.status === "candidate_found" || t.webResult?.status === "found") && t.webResult?.candidate_zh;
                                const noEvidence = t.webResult?.status === "not_found";
                                const hasError = t.webResult?.status === "error";
                                return (
                                  <tr key={t.source_text}>
                                    <td style={{ fontFamily: "monospace", fontSize: 12 }}>
                                      {t.source_text}
                                      {t.search_text && (
                                        <div style={{ fontSize: 10, color: "#888", marginTop: 1 }}>
                                          検索別名: {(t.aliases ?? [t.source_text]).join(", ")}
                                        </div>
                                      )}
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
                                    <td style={{ fontFamily: "monospace", fontSize: 12 }}>{t.confirmed_surface || "—"}</td>
                                    <td style={{ textAlign: "center" }}>
                                      {!t.webResult && (
                                        <span className="status-pill high">未確認</span>
                                      )}
                                      {t.webResult && (() => {
                                        const wr = t.webResult;
                                        const mj = wr.match_judgment;
                                        const es = wr.evidence_strength;
                                        const review = deriveNeedsHumanReview(wr);
                                        if (mj === "exact" && es === "direct" && !review) {
                                          return <span className="status-pill success" title="高確度: exact + direct">高確度</span>;
                                        }
                                        if (mj === "weak" || mj === "not_found" || es === "none") {
                                          return <span className="status-pill high" title="手入力推奨">手入力推奨</span>;
                                        }
                                        return <span className="status-pill medium" title={`判定:${mj} 根拠:${es} 確認:${review ? "要" : "不要"}`}>
                                          要確認{review ? "" : " (自動可)"}
                                        </span>;
                                      })()}
                                    </td>
                                    <td style={{ fontSize: 11, color: "var(--text-secondary)", maxWidth: 260 }}>
                                      {(() => {
                                        const wr = t.webResult;
                                        if (!wr) return <span>{t.reason}</span>;
                                        const ek = `${index}:${t.source_text}`;
                                        const isExpanded = expandedEvidence.has(ek);
                                        const parts: (string | JSX.Element)[] = [];
                                        // 1. confidence_reason (primary, bold)
                                        if (wr.confidence_reason) {
                                          const cr = wr.confidence_reason;
                                          const truncated = cr.length > 200 ? cr.slice(0, 200) + "…" : cr;
                                          parts.push(
                                            <span key="cr" style={{ fontWeight: 600, color: "var(--text-primary)" }}>
                                              {truncated}
                                            </span>
                                          );
                                          parts.push(<br key="br1" />);
                                        }
                                        // 2. Evidence toggle
                                        const ev = wr.evidence;
                                        if (ev?.length) {
                                          parts.push(
                                            <button
                                              key="ev-btn"
                                              className="srt-evidence-toggle"
                                              onClick={() => {
                                                setExpandedEvidence((prev) => {
                                                  const next = new Set(prev);
                                                  if (next.has(ek)) next.delete(ek);
                                                  else next.add(ek);
                                                  return next;
                                                });
                                              }}
                                            >
                                              根拠({ev.length}件) {isExpanded ? "▾" : "▸"}
                                            </button>
                                          );
                                          if (isExpanded) {
                                            parts.push(
                                              <span key="ev-list" className="srt-evidence-list">
                                                {ev.slice(0, 5).map((e, i) => (
                                                  <span key={i} className="srt-evidence-item">
                                                    <a href={e.url} target="_blank" rel="noopener noreferrer" style={{ color: "var(--accent)", textDecoration: "underline" }}>
                                                      {e.title || e.url}
                                                    </a>
                                                    {e.quote && <span className="srt-evidence-item-quote">"{e.quote}"</span>}
                                                  </span>
                                                ))}
                                                {ev.length > 5 && <span style={{ fontSize: 10, color: "#888" }}>…他{ev.length - 5}件</span>}
                                              </span>
                                            );
                                          }
                                        } else if (wr.evidence_summary) {
                                          const es = wr.evidence_summary;
                                          const truncated = es.length > 150 ? es.slice(0, 150) + "…" : es;
                                          parts.push(<span key="es">{truncated}</span>);
                                        } else if (t.reason) {
                                          const clean = t.reason.replace(/https?:\/\/\S+/g, "");
                                          const truncated = clean.length > 150 ? clean.slice(0, 150) + "…" : clean;
                                          parts.push(<span key="tr">{truncated}</span>);
                                        }
                                        return parts;
                                      })()}
                                    </td>
                                    <td style={{ textAlign: "center", whiteSpace: "nowrap" }}>
                                      <button
                                        className="btn btn-sm"
                                        style={{ background: "#3b82f6", color: "#fff", marginRight: 4 }}
                                        onClick={() => openManualInput(t)}
                                      >
                                        手入力
                                      </button>
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
                        <div style={{ display: "flex", justifyContent: "flex-end", marginTop: 6 }}>
                          <button
                            className="btn btn-sm"
                            style={{ fontSize: 12, background: "#6b7280", color: "#fff" }}
                            onClick={() => exportUnresolvedTermsCsv(activeFile.unresolvedTerms, activeFile.name)}
                          >
                            CSV出力 ({activeFile.unresolvedTerms.length}件)
                          </button>
                        </div>
                        {/* Batch adopt & regenerate buttons at the bottom of the proper noun correction block */}
                        <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginTop: 8 }}>
                          {(() => {
                            const highCount = activeFile ? activeFile.unresolvedTerms.filter(t => {
                              const wr = t.webResult;
                              if (!wr || t.adopted || !wr.candidate_zh && !wr.candidate_ja) return false;
                              if (wr.status !== "found" || wr.confidence !== "high") return false;
                              if (wr.evidence_strength !== "direct" || wr.match_judgment !== "exact") return false;
                              if (deriveNeedsHumanReview(wr) !== false) return false;
                              return true;
                            }).length : 0;
                            return (
                              <button
                                className="btn btn-primary"
                                onClick={handleBatchAdoptHighAndRegen}
                                disabled={highCount === 0 || !hasLoaded || loadingSynop || !projectBaseDir}
                                style={{ fontSize: 12 }}
                              >
                                <RefreshCw size={14} />
                                {loadingSynop ? "保存・再生成中…" : `高確度のものを受け入れて再生成 (${highCount}件)`}
                              </button>
                            );
                          })()}
                          {(() => {
                            const allFoundCount = activeFile ? activeFile.unresolvedTerms.filter(t => {
                              const wr = t.webResult;
                              if (!wr || t.adopted || !wr.candidate_zh && !wr.candidate_ja) return false;
                              if (wr.status !== "found") return false;
                              return true;
                            }).length : 0;
                            return (
                              <button
                                className="btn btn-primary"
                                onClick={handleBatchAdoptAllAndRegen}
                                disabled={allFoundCount === 0 || !hasLoaded || loadingSynop || !projectBaseDir}
                                style={{ fontSize: 12 }}
                              >
                                <RefreshCw size={14} />
                                {loadingSynop ? "保存・再生成中…" : `すべて受け入れて再生成 (${allFoundCount}件)`}
                              </button>
                            );
                          })()}
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

              {/* 2.2 Scene detection button — placed below proper noun correction block */}
              {activeFile.synopsis && (
                <div style={{ marginBottom: 16 }}>
                  <button
                    className="btn btn-primary"
                    onClick={handleDetectScenes}
                    disabled={!hasLoaded || loadingScene}
                    style={{ fontSize: 12 }}
                  >
                    <Layers size={14} />
                    2.2 場面検出
                  </button>
                </div>
              )}

              {/* 2.2 Scene detection result */}
              {activeFile.sceneDetection && (
                <div style={{ marginBottom: 16 }}>
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 8 }}>
                    <h4 style={{ fontSize: 13, margin: 0 }}>
                      2.2 場面検出 ({activeFile.sceneDetection.scenes.length}場面)
                    </h4>
                    <button
                      className="btn btn-primary btn-sm"
                      onClick={handleAnalyzeContext}
                      disabled={!hasSceneDetection || loadingContext}
                      style={{ fontSize: 11 }}
                      title="2.3 状況分析を実行"
                    >
                      <MessageSquare size={12} />
                      {loadingContext ? "分析中…" : "2.3 状況分析を実行"}
                    </button>
                  </div>
                  {Object.keys(activeFile.sceneContexts).length === 0 && (
                    <p style={{ fontSize: 11, color: "var(--text-secondary)", margin: "0 0 6px 0" }}>
                      実行後、各場面に関係性が表示されます
                    </p>
                  )}
                  <div className="table-container" style={{ maxHeight: 300, overflowY: "auto" }}>
                    <table>
                      <thead>
                        <tr>
                          <th style={{ width: 30 }}>#</th>
                          <th style={{ width: 140 }}>場面ラベル</th>
                          <th style={{ textAlign: "center", width: 80 }}>字幕範囲</th>
                          <th style={{ textAlign: "center", width: 50 }}>行数</th>
                          <th style={{ width: 120 }}>理由</th>
                          <th style={{ width: 180 }}>関係性</th>
                        </tr>
                      </thead>
                      <tbody>
                        {activeFile.sceneDetection.scenes.map((s) => {
                          const ctx = activeFile.sceneContexts[s.scene_index];
                          const relationSummary = ctx
                            ? (ctx.hierarchy
                              || ctx.gender_notes?.join(" / ")
                              || ctx.context_ja?.split(/[。\n]/)[0]
                              || "")
                            : "";
                          const shortSummary = relationSummary.length > 40
                            ? relationSummary.slice(0, 40) + "…"
                            : relationSummary;
                          return (
                            <tr key={s.scene_index}>
                              <td style={{ textAlign: "center", fontSize: 12 }}>{s.scene_index + 1}</td>
                              <td style={{ fontSize: 12 }}>{s.title}</td>
                              <td style={{ textAlign: "center", fontSize: 12, fontFamily: "monospace" }}>
                                {s.start_entry_index}–{s.end_entry_index}
                              </td>
                              <td style={{ textAlign: "center", fontSize: 12 }}>{s.entry_count}</td>
                              <td style={{ fontSize: 12, color: "var(--text-secondary)" }}>{s.reason || "理由未出力"}</td>
                              <td style={{ fontSize: 12 }}>
                                {ctx ? (
                                  relationSummary.length > 40 ? (
                                    <details>
                                      <summary style={{ cursor: "pointer", color: "var(--text-primary)" }}>{shortSummary}</summary>
                                      <div style={{ marginTop: 4, fontSize: 11, color: "var(--text-secondary)", lineHeight: 1.5 }}>
                                        {ctx.context_ja && <div>{ctx.context_ja}</div>}
                                        {ctx.hierarchy && <div style={{ marginTop: 2 }}>上下関係: {ctx.hierarchy}</div>}
                                        {ctx.gender_notes.length > 0 && (
                                          <ul style={{ margin: "2px 0 0 0", paddingLeft: 16 }}>
                                            {ctx.gender_notes.map((note, i) => <li key={i}>{note}</li>)}
                                          </ul>
                                        )}
                                      </div>
                                    </details>
                                  ) : (
                                    <span>{relationSummary}</span>
                                  )
                                ) : (
                                  <span style={{ color: "var(--text-secondary)", opacity: 0.5 }}>未分析</span>
                                )}
                              </td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                  </div>
                  <div style={{ display: "flex", justifyContent: "flex-end", marginTop: 8 }}>
                    <button
                      className="btn btn-sm"
                      style={{ fontSize: 11 }}
                      onClick={() => exportSceneDetectionCsv(
                        activeFile.sceneDetection!.scenes,
                        activeFile.name,
                        activeFile.sceneContexts,
                      )}
                    >
                      CSV出力
                    </button>
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

      {/* Manual input modal */}
      {manualTarget && (
        <div style={{
          position: "fixed", inset: 0, background: "rgba(0,0,0,0.5)",
          display: "flex", alignItems: "center", justifyContent: "center", zIndex: 1000,
        }} onClick={() => setManualTarget(null)}>
          <div style={{
            background: "var(--bg-primary)", borderRadius: 8, padding: 20, minWidth: 360, maxWidth: 440,
            boxShadow: "0 8px 32px rgba(0,0,0,0.3)",
          }} onClick={(e) => e.stopPropagation()}>
            <h3 style={{ margin: "0 0 12px", fontSize: 14 }}>
              手入力採用: {manualTarget.source_text}
            </h3>
            <div style={{ marginBottom: 8 }}>
              <label style={{ display: "block", fontSize: 11, marginBottom: 2, color: "var(--text-secondary)" }}>中文表記 (candidate_zh)</label>
              <input
                value={manualZh}
                onChange={(e) => setManualZh(e.target.value)}
                style={{ width: "100%", padding: "6px 8px", fontSize: 13, borderRadius: 4, border: "1px solid var(--border)", background: "var(--bg-secondary)", color: "var(--text-primary)" }}
                placeholder="例: 金辉部族"
                autoFocus
              />
            </div>
            <div style={{ marginBottom: 8 }}>
              <label style={{ display: "block", fontSize: 11, marginBottom: 2, color: "var(--text-secondary)" }}>日本語表記 (candidate_ja)</label>
              <input
                value={manualJa}
                onChange={(e) => setManualJa(e.target.value)}
                style={{ width: "100%", padding: "6px 8px", fontSize: 13, borderRadius: 4, border: "1px solid var(--border)", background: "var(--bg-secondary)", color: "var(--text-primary)" }}
                placeholder="例: 金暉部族"
              />
            </div>
            <div style={{ marginBottom: 8 }}>
              <label style={{ display: "block", fontSize: 11, marginBottom: 2, color: "var(--text-secondary)" }}>種別 (term_type)</label>
              <select
                value={manualType}
                onChange={(e) => setManualType(e.target.value)}
                style={{ width: "100%", padding: "6px 8px", fontSize: 13, borderRadius: 4, border: "1px solid var(--border)", background: "var(--bg-secondary)", color: "var(--text-primary)" }}
              >
                <option value="proper_noun">proper_noun（固有名詞）</option>
                <option value="person">person（人物）</option>
                <option value="place">place（地名）</option>
                <option value="group">group / tribe / organization（集団）</option>
                <option value="title">title（称号）</option>
                <option value="technique">technique（技・術）</option>
                <option value="other">other（その他）</option>
              </select>
            </div>
            <div style={{ marginBottom: 12 }}>
              <label style={{ display: "block", fontSize: 11, marginBottom: 2, color: "var(--text-secondary)" }}>メモ (note)</label>
              <input
                value={manualNote}
                onChange={(e) => setManualNote(e.target.value)}
                style={{ width: "100%", padding: "6px 8px", fontSize: 13, borderRadius: 4, border: "1px solid var(--border)", background: "var(--bg-secondary)", color: "var(--text-primary)" }}
                placeholder="手入力採用（任意）"
              />
            </div>
            <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
              <button
                className="btn btn-sm"
                style={{ background: "#6b7280", color: "#fff" }}
                onClick={() => setManualTarget(null)}
              >
                キャンセル
              </button>
              <button
                className="btn btn-sm"
                style={{ background: "#3b82f6", color: "#fff" }}
                disabled={!manualZh.trim() && !manualJa.trim()}
                onClick={handleManualAdopt}
              >
                採用
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
