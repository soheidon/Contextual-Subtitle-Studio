import { useState, useRef } from "react";
import { ClipboardPaste, GitMerge, Download, Globe, Loader2, FolderOpen, Search, FileText, Copy, Save } from "lucide-react";
import { searchTmdb, scrapeTmdbCredits, buildCharacterDict, scrapeUrl, saveDramaInfo, loadDramaInfo, parseMdlHtmlPaste, openUrl, summarizeSynopsis, mergeCastEntries, correctJaKanji, enrichDictKanji } from "../../lib/tauri";
import { buildPromptText } from "../../lib/prompt";
import DramaSearchPanel from "./DramaSearchPanel";
import type { PastedEntry, CharacterDict, DramaInfo, TmdbSearchResult, SynopsisSummary, MergedCastEntry } from "../../types";
import { useAppLogStore } from "../../stores/useAppLogStore";

function normalizeDoubanUrl(url: string): string | null {
  const m = url.match(/douban\.com\/subject\/(\d+)/);
  if (!m) return null;
  return `https://movie.douban.com/subject/${m[1]}/celebrities`;
}

type Tab = "douban" | "imdb" | "mdl" | "merge";

export default function CharacterDictBuilder() {
  const [activeTab, setActiveTab] = useState<Tab>("douban");
  const [doubanUrl, setDoubanUrl] = useState("");
  const [doubanFetching, setDoubanFetching] = useState(false);
  const [doubanDrama, setDoubanDrama] = useState<DramaInfo | null>(null);
  const [tmdbQuery, setTmdbQuery] = useState("");
  const [tmdbSearching, setTmdbSearching] = useState(false);
  const [tmdbResults, setTmdbResults] = useState<TmdbSearchResult[]>([]);
  const [selectedTmdbId, setSelectedTmdbId] = useState<number | null>(null);
  const [selectedMediaType, setSelectedMediaType] = useState<string>("");
  const [tmdbFetching, setTmdbFetching] = useState(false);
  const [tmdbDrama, setTmdbDrama] = useState<DramaInfo | null>(null);
  const [tmdbEntries, setTmdbEntries] = useState<PastedEntry[]>([]);
  const [tmdbError, setTmdbError] = useState<string | null>(null);
  const [mdlPasteContent, setMdlPasteContent] = useState("");
  const [mdlPasteFormat, setMdlPasteFormat] = useState<"html" | "text" | "empty">("empty");
  const [mdlHtmlEntries, setMdlHtmlEntries] = useState<PastedEntry[]>([]);
  const [mdlHtmlParsing, setMdlHtmlParsing] = useState(false);
  const [mdlHtmlError, setMdlHtmlError] = useState<string | null>(null);
  const [doubanEntries, setDoubanEntries] = useState<PastedEntry[]>([]);
  const [dict, setDict] = useState<CharacterDict | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [workDir, setWorkDir] = useState<string>("");
  const [synopsisSummary, setSynopsisSummary] = useState<SynopsisSummary | null>(null);
  const [summarizing, setSummarizing] = useState(false);
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [mergedCast, setMergedCast] = useState<MergedCastEntry[] | null>(null);
  const [mergingCast, setMergingCast] = useState(false);
  const [mergedCastError, setMergedCastError] = useState<string | null>(null);
  const [editableJaKanji, setEditableJaKanji] = useState<Record<number, string>>({});
  const [editablePnKanji, setEditablePnKanji] = useState<Record<number, string>>({});
  const addLog = useAppLogStore((s) => s.addLog);

  // Drama title state (lifted — single source of truth for search panel + MDL tab)
  const [searchTitleZh, setSearchTitleZh] = useState("");
  const [searchTitleEn, setSearchTitleEn] = useState("");
  const [searchYear, setSearchYear] = useState("");

  const restoringRef = useRef(false);

  const baseMetadata = () => ({
    drama_title: null as string | null,
    douban_url: null as string | null,
    tmdb_url: null as string | null,
    search_title_zh: searchTitleZh.trim() || null,
    search_title_en: searchTitleEn.trim() || null,
    search_year: searchYear.trim() || null,
    updated_at: new Date().toISOString(),
  });

  const handleSelectWorkDir = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true, multiple: false, title: "作業ディレクトリを選択" });
      if (selected && typeof selected === "string") {
        setWorkDir(selected);
        await restoreFromWorkDir(selected);
      }
    } catch (e) {
      console.error("ディレクトリ選択エラー:", e);
    }
  };

  // DramaSearchPanel callbacks
  const handleSearchTmdbAutoSelect = (result: TmdbSearchResult) => {
    setSelectedTmdbId(result.tmdb_id);
    setSelectedMediaType(result.media_type);
    setActiveTab("imdb");
    // Trigger credits fetch
    (async () => {
      try {
        setTmdbFetching(true);
        setTmdbError(null);
        addLog("info", "TMDb", `自動採用: ${result.title} (ID:${result.tmdb_id}), credits取得開始`);
        const scrapeResult = await scrapeTmdbCredits(result.tmdb_id, result.media_type);
        const entries = scrapeResult.characters.map((c) => {
          const name = c.actor_name || "";
          const alias = c.aliases[0] || "";
          const combined = (alias && name && alias !== name) ? `${alias} ${name}` : (name || alias);
          return {
            actor_name: combined,
            character_name: c.character_name,
            role_type: c.role_type,
            source: "Tmdb" as const,
          };
        });
        setTmdbEntries(entries);
        setTmdbDrama({ title: scrapeResult.drama_title || result.title, synopsis: scrapeResult.synopsis || "", source: "tmdb" });
        addLog("success", "TMDb", `credits取得完了: ${entries.length}件`);
        if (workDir) {
          saveDramaInfo(workDir, {
            metadata: { ...baseMetadata(), drama_title: scrapeResult.drama_title || result.title },
            synopsis_douban: null, synopsis_tmdb: scrapeResult.synopsis || null, cast_douban: null, cast_tmdb: entries, cast_mdl: mdlHtmlEntries, character_dict: null,
            synopsis_summary: synopsisSummary,
            merged_cast: mergedCast,
          }).catch((e) => console.error("保存エラー:", e));
        }
      } catch (e) {
        setTmdbError(String(e));
        addLog("error", "TMDb", `credits取得失敗: ${e}`);
      } finally {
        setTmdbFetching(false);
      }
    })();
  };

  const handleSearchDoubanAutoSelect = (url: string) => {
    setDoubanUrl(url);
    setActiveTab("douban");
    // Trigger Douban fetch
    (async () => {
      try {
        setDoubanFetching(true);
        const normalized = normalizeDoubanUrl(url);
        if (!normalized) {
          addLog("error", "豆瓣", `URL正規化失敗: ${url}`);
          return;
        }
        setDoubanUrl(normalized);
        addLog("info", "豆瓣", `自動採用: ${normalized}`);
        const result = await scrapeUrl(normalized, "Douban");
        const entries = result.characters.map((c) => {
          const name = c.actor_name || "";
          const alias = c.aliases[0] || "";
          const combined = (alias && name && alias !== name) ? `${alias} ${name}` : (name || alias);
          return {
            actor_name: combined,
            character_name: c.character_name,
            role_type: c.role_type,
            source: "Douban" as const,
          };
        });
        setDoubanEntries(entries);
        const title = result.drama_title || result.page_title || "";
        setDoubanDrama({ title, synopsis: result.synopsis || "", source: "douban" });
        addLog("success", "豆瓣", `成功 — ${entries.length}件のキャスト取得`);
        if (workDir) {
          saveDramaInfo(workDir, {
            metadata: { ...baseMetadata(), drama_title: title, douban_url: normalized },
            synopsis_douban: result.synopsis || null, synopsis_tmdb: null, cast_douban: entries, cast_tmdb: null, cast_mdl: mdlHtmlEntries, character_dict: null,
            synopsis_summary: synopsisSummary,
            merged_cast: mergedCast,
          }).catch((e) => console.error("保存エラー:", e));
        }
      } catch (e) {
        addLog("error", "豆瓣", `取得失敗: ${e}`);
      } finally {
        setDoubanFetching(false);
      }
    })();
  };

  const handleSearchTmdbCandidates = (_results: TmdbSearchResult[]) => {
    setActiveTab("imdb");
    addLog("info", "TMDb", `自動採用できず — 候補一覧を表示`);
  };

  const handleMdlGoogleSearch = async () => {
    const parts = ["site:mydramalist.com"];
    if (searchTitleZh.trim()) parts.push(`"${searchTitleZh.trim()}"`);
    if (searchTitleEn.trim()) parts.push(`"${searchTitleEn.trim()}"`);
    parts.push("cast");
    const query = parts.join(" ");
    const url = `https://www.google.com/search?q=${encodeURIComponent(query)}`;
    try {
      addLog("info", "MDL Search", `Google検索を開きます: ${query}`);
      await openUrl(url);
    } catch (e) {
      addLog("error", "MDL Search", `Google検索を開けませんでした: ${e}`);
    }
  };

  const restoreFromWorkDir = async (dir: string) => {
    restoringRef.current = true;
    try {
      const bundle = await loadDramaInfo(dir);
      if (bundle.metadata?.douban_url) setDoubanUrl(bundle.metadata.douban_url);
      if (bundle.metadata?.search_title_zh) setSearchTitleZh(bundle.metadata.search_title_zh);
      if (bundle.metadata?.search_title_en) setSearchTitleEn(bundle.metadata.search_title_en);
      if (bundle.metadata?.search_year) setSearchYear(bundle.metadata.search_year);
      const title = bundle.metadata?.drama_title || "";
      if (bundle.synopsis_douban) setDoubanDrama({ title, synopsis: bundle.synopsis_douban, source: "douban" });
      if (bundle.synopsis_tmdb) setTmdbDrama({ title, synopsis: bundle.synopsis_tmdb, source: "tmdb" });
      if (bundle.cast_douban) setDoubanEntries(bundle.cast_douban);
      if (bundle.cast_tmdb) {
        const tmdb: PastedEntry[] = [];
        const mdl: PastedEntry[] = [];
        for (const e of bundle.cast_tmdb) {
          if (e.source === "MdlHtml") mdl.push(e);
          else tmdb.push(e);
        }
        setTmdbEntries(tmdb);
        setMdlHtmlEntries(mdl);
      }
      if (bundle.cast_mdl) {
        setMdlHtmlEntries(bundle.cast_mdl);
      }
      if (bundle.character_dict) {
        setDict(bundle.character_dict);
      }
      if (bundle.synopsis_summary) {
        setSynopsisSummary(bundle.synopsis_summary);
      }
      if (bundle.merged_cast) {
        setMergedCast(bundle.merged_cast);
      }
    } catch {
      // drama_info not found — new project, nothing to restore
    } finally {
      // Allow React to batch all setState calls before re-enabling auto-save
      setTimeout(() => { restoringRef.current = false; }, 500);
    }
  };


  const handleParseMdlHtml = async () => {
    if (!mdlPasteContent.trim()) return;
    try {
      setMdlHtmlParsing(true);
      setMdlHtmlError(null);

      if (mdlPasteFormat === "html") {
        addLog("info", "MDL HTML", `HTMLパース: ${mdlPasteContent.length}文字`);
        const entries = await parseMdlHtmlPaste(mdlPasteContent);
        setMdlHtmlEntries(entries);
        const withChar = entries.filter((e) => e.character_name).length;
        addLog("success", "MDL HTML", `抽出: ${entries.length}件 (characterあり: ${withChar}件)`);
        if (entries.length === 0) {
          setMdlHtmlError("MDLのCast欄HTMLではない可能性があります（list-itemが見つかりません）");
        }
      } else {
        // text format — can't use HTML parser, show warning
        addLog("warning", "MDL HTML", "HTML形式のデータがありません。テキストパースは未実装です。");
        setMdlHtmlError("HTML形式で貼り付けられていません。ブラウザのCast欄を範囲選択してコピーし、再度貼り付けてください。");
      }
    } catch (e) {
      const msg = String(e);
      addLog("error", "MDL HTML", msg);
      setMdlHtmlError(msg);
    } finally {
      setMdlHtmlParsing(false);
    }
  };

  const handleMdlHtmlPaste = (event: React.ClipboardEvent<HTMLDivElement>) => {
    addLog("info", "MDL HTML", "paste event received");

    const html = event.clipboardData.getData("text/html");
    const text = event.clipboardData.getData("text/plain");

    addLog("debug", "MDL HTML", `clipboard text/html length: ${html?.length || 0}`);
    addLog("debug", "MDL HTML", `clipboard text/plain length: ${text?.length || 0}`);

    if (html && html.trim().length > 100) {
      event.preventDefault();
      setMdlPasteContent(html);
      setMdlPasteFormat("html");
      setMdlHtmlEntries([]);
      setMdlHtmlError(null);
      addLog("success", "MDL HTML", `HTMLとして保存 (${html.length}文字)`);
      return;
    }

    if (text && text.trim().length > 0) {
      event.preventDefault();
      setMdlPasteContent(text);
      setMdlPasteFormat("text");
      setMdlHtmlEntries([]);
      setMdlHtmlError(null);
      addLog("warning", "MDL HTML", `text/plainにフォールバック (${text.length}文字)`);
      return;
    }

    addLog("warning", "MDL HTML", "クリップボードからHTML/テキストを取得できませんでした");
  };

  const handleMergeCast = async () => {

    try {
      setMergingCast(true);
      setMergedCastError(null);
      const englishEntries = [...tmdbEntries, ...mdlHtmlEntries.filter(
        (m) => !tmdbEntries.some((t) => t.actor_name === m.actor_name && t.character_name === m.character_name)
      )];
      addLog("info", "Merge", "キャスト統合を開始...");
      let result = await mergeCastEntries(englishEntries, doubanEntries, mdlHtmlEntries);

      // Auto-run Japanese kanji conversion after merge (no separate button needed)
      addLog("info", "JaKanji", `LLM conversion started: ${result.length} rows`);
      try {
        const dramaTitle = searchTitleZh.trim() || undefined;
        result = await correctJaKanji(result, dramaTitle);
        addLog("success", "JaKanji", "LLM conversion completed");
      } catch (e: any) {
        addLog("error", "JaKanji", `LLM conversion failed: ${e}`);
        // Keep merged result even if kanji correction fails
      }

      setMergedCast(result);
      setEditableJaKanji({});
      addLog("success", "Merge", `統合完了: ${result.length}件`);
      if (workDir) {
        saveDramaInfo(workDir, { metadata: null, synopsis_douban: doubanDrama?.synopsis || null, synopsis_tmdb: tmdbDrama?.synopsis || null, cast_douban: doubanEntries, cast_tmdb: englishEntries, cast_mdl: mdlHtmlEntries, character_dict: dict, synopsis_summary: synopsisSummary, merged_cast: result })
          .catch((e) => console.error("保存エラー:", e));
      }
    } catch (e) {
      const msg = String(e);
      setMergedCastError(msg);
      addLog("error", "Merge", msg);
    } finally {
      setMergingCast(false);
    }
  };

  const handleDownloadCastCsv = async () => {
    if (!mergedCast || mergedCast.length === 0) return;
    const header = ["俳優名・中国語", "俳優名・英語", "役名・中国語", "役名・英語", "役名・日本語漢字"];
    const csvEscape = (s: string) => `"${s.replace(/"/g, '""')}"`;
    const rows = mergedCast.map((e) =>
      [
        e.actor_zh,
        e.actor_en_douban || e.actor_en_matched,
        e.character_zh,
        e.character_en ?? "",
        e.character_ja_kanji ?? "",
      ].map(csvEscape).join(",")
    );
    const bom = "﻿";
    const csv = bom + [header.join(","), ...rows].join("\n");
    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const { writeTextFile } = await import("@tauri-apps/plugin-fs");
      const defaultPath = workDir ? `${workDir}/merged_cast.csv` : undefined;
      const path = await save({
        title: "キャストCSVを保存",
        defaultPath,
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (path) await writeTextFile(path, csv);
    } catch {
      const blob = new Blob([csv], { type: "text/csv" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url; a.download = "merged_cast.csv"; a.click();
      URL.revokeObjectURL(url);
    }
  };

  const handleSummarizeSynopsis = async () => {
    try {
      setSummarizing(true);
      setSummaryError(null);
      const cn = doubanDrama?.synopsis || "";
      const en = tmdbDrama?.synopsis || "";
      addLog("info", "Synopsis", "あらすじ要約を開始...");
      const summary = await summarizeSynopsis(
        cn,
        en,
        searchTitleZh.trim() || null,
        searchTitleEn.trim() || null,
        searchYear.trim() || null,
        mergedCast,
      );
      setSynopsisSummary(summary);
      setEditablePnKanji({});
      addLog("success", "Synopsis", `要約完了: human_summary=${summary.human_summary_ja.length}文字, context=${summary.translation_context_short_ja.length}文字, 固有名詞=${summary.proper_nouns.length}件`);
      if (workDir) {
        saveDramaInfo(workDir, { metadata: null, synopsis_douban: doubanDrama?.synopsis || null, synopsis_tmdb: tmdbDrama?.synopsis || null, cast_douban: doubanEntries, cast_tmdb: tmdbEntries, cast_mdl: mdlHtmlEntries, character_dict: dict, synopsis_summary: summary, merged_cast: mergedCast })
          .catch((e) => console.error("保存エラー:", e));
      }
    } catch (e) {
      const msg = String(e);
      setSummaryError(msg);
      addLog("error", "Synopsis", msg);
    } finally {
      setSummarizing(false);
    }
  };

  const handleBuild = async () => {
    try {
      setLoading(true);
      setError(null);
      // Include MDL HTML entries as English-side sources
      const englishEntries = [...tmdbEntries, ...mdlHtmlEntries.filter(
        (m) => !tmdbEntries.some((t) => t.actor_name === m.actor_name && t.character_name === m.character_name)
      )];
      let result = await buildCharacterDict(englishEntries, doubanEntries);
      // If mergedCast has LLM kanji, enrich the dictionary
      if (mergedCast && mergedCast.some(e => e.character_ja_kanji_source === "llm" || e.character_ja_kanji_source === "manual")) {
        result = await enrichDictKanji(result, mergedCast);
        addLog("info", "Dictionary", "LLM漢字を辞書に反映しました");
      }
      setDict(result);
      setActiveTab("merge");
      if (workDir) {
        saveDramaInfo(workDir, { metadata: null, synopsis_douban: doubanDrama?.synopsis || null, synopsis_tmdb: tmdbDrama?.synopsis || null, cast_douban: doubanEntries, cast_tmdb: englishEntries, cast_mdl: mdlHtmlEntries, character_dict: result, synopsis_summary: synopsisSummary, merged_cast: mergedCast })
          .catch((e) => console.error("保存エラー:", e));
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const getPromptInput = () => ({ synopsisSummary, dict, editablePnKanji });

  const handleCopyPrompt = async () => {
    const prompt = buildPromptText(getPromptInput());
    try {
      await navigator.clipboard.writeText(prompt);
      addLog("info", "Prompt", "プロンプトをクリップボードにコピーしました");
    } catch {
      addLog("error", "Prompt", "クリップボードへのコピーに失敗しました");
    }
  };

  const handleSavePrompt = async () => {
    const prompt = buildPromptText(getPromptInput());
    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const { writeTextFile } = await import("@tauri-apps/plugin-fs");
      const defaultPath = workDir ? `${workDir}/translation_prompt.txt` : undefined;
      const path = await save({
        title: "翻訳プロンプトを保存",
        defaultPath,
        filters: [{ name: "テキスト", extensions: ["txt"] }],
      });
      if (path) {
        await writeTextFile(path, prompt);
      }
    } catch {
      const blob = new Blob([prompt], { type: "text/plain" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "translation_prompt.txt";
      a.click();
      URL.revokeObjectURL(url);
    }
  };

  const handleDoubanFetch = async () => {
    const raw = doubanUrl.trim();
    if (!raw) return;
    const normalized = normalizeDoubanUrl(raw);
    if (!normalized) {
      addLog("error", "豆瓣", `Douban subject IDを取得できません。movie.douban.com/subject/... URLを入力してください: ${raw}`);
      setError("DoubanのURLが正しくありません。movie.douban.com/subject/... の形式で入力してください。");
      return;
    }
    try {
      setDoubanFetching(true);
      setError(null);
      addLog("info", "豆瓣", `入力URL: ${raw}`);
      if (raw !== normalized) addLog("info", "豆瓣", `正規化: ${normalized}`);
      const result = await scrapeUrl(normalized, "Douban");
      const title = result.drama_title || result.page_title || "";
      if (result.synopsis) {
        setDoubanDrama({ title, synopsis: result.synopsis, source: "douban" });
      }
      const entries: PastedEntry[] = result.characters.map((c) => {
        const name = c.actor_name || "";
        const alias = c.aliases[0] || "";
        const combined = (alias && name && alias !== name) ? `${alias} ${name}` : (name || alias);
        return {
          actor_name: combined,
          character_name: c.character_name,
          role_type: c.role_type,
          source: "Douban" as const,
        };
      });
      setDoubanEntries(entries);
      addLog("success", "豆瓣", `成功 — ${entries.length}件のキャスト取得`);
      if (workDir) {
        saveDramaInfo(workDir, {
          metadata: { ...baseMetadata(), drama_title: title, douban_url: normalized },
          synopsis_douban: result.synopsis || null,
          synopsis_tmdb: null,
          cast_douban: entries,
          cast_tmdb: null,
          cast_mdl: mdlHtmlEntries,
          character_dict: null,
            synopsis_summary: synopsisSummary,
            merged_cast: mergedCast,
        }).catch((e) => console.error("保存エラー:", e));
      }
    } catch (e) {
      const msg = String(e);
      addLog("error", "豆瓣", msg);
      setError(`豆瓣取得エラー — 詳細は下のログを展開してください`);
    } finally {
      setDoubanFetching(false);
    }
  };

  const handleTmdbSearch = async () => {
    if (!tmdbQuery.trim()) return;
    try {
      setTmdbSearching(true);
      setTmdbError(null);
      setTmdbResults([]);
      setSelectedTmdbId(null);
      setSelectedMediaType("");
      addLog("info", "TMDb", `検索: ${tmdbQuery.trim()}`);
      const results = await searchTmdb(tmdbQuery.trim());
      setTmdbResults(results);
      addLog("success", "TMDb", `検索結果: ${results.length}件`);
      if (results.length === 0) {
        addLog("warning", "TMDb", "該当する作品が見つかりませんでした。キーワードを変えて再検索してください。");
      }
      // Auto-select if only one result
      if (results.length === 1) {
        setSelectedTmdbId(results[0].tmdb_id);
        setSelectedMediaType(results[0].media_type);
        addLog("debug", "TMDb", "候補が1件のみのため自動選択しました");
      }
    } catch (e) {
      const msg = String(e);
      addLog("error", "TMDb", msg);
      setTmdbError(msg);
    } finally {
      setTmdbSearching(false);
    }
  };

  const handleTmdbFetchCredits = async () => {
    if (selectedTmdbId === null || !selectedMediaType) return;
    try {
      setTmdbFetching(true);
      setTmdbError(null);
      addLog("info", "TMDb", `credits取得: TMDb ID=${selectedTmdbId}, type=${selectedMediaType}`);
      const result = await scrapeTmdbCredits(selectedTmdbId, selectedMediaType);
      const title = result.drama_title || result.page_title || "";
      if (result.synopsis) {
        setTmdbDrama({ title, synopsis: result.synopsis, source: "tmdb" });
      }
      const entries: PastedEntry[] = result.characters.map((c) => {
        const name = c.actor_name || "";
        const alias = c.aliases[0] || "";
        const combined = (alias && name && alias !== name) ? `${alias} ${name}` : (name || alias);
        return {
          actor_name: combined,
          character_name: c.character_name,
          role_type: c.role_type,
          source: "Tmdb" as const,
        };
      });
      setTmdbEntries(entries);
      addLog("success", "TMDb", `成功 — ${entries.length}件のキャスト取得`);
      if (entries.length > 0) {
        const preview = entries.slice(0, 3).map((e) => `${e.actor_name} → ${e.character_name}`).join(" / ");
        addLog("debug", "TMDb", `先頭3件: ${preview}${entries.length > 3 ? ` ...他${entries.length - 3}件` : ""}`);
      }
      if (result.drama_title) {
        addLog("info", "TMDb", `作品タイトル: ${result.drama_title}`);
      }
      if (workDir) {
        saveDramaInfo(workDir, {
          metadata: { ...baseMetadata(), drama_title: title },
          synopsis_douban: null,
          synopsis_tmdb: result.synopsis || null,
          cast_douban: null,
          cast_tmdb: entries,
          cast_mdl: mdlHtmlEntries,
          character_dict: null,
            synopsis_summary: synopsisSummary,
            merged_cast: mergedCast,
        }).catch((e) => console.error("保存エラー:", e));
      }
    } catch (e) {
      const msg = String(e);
      addLog("error", "TMDb", msg);
      setTmdbError(msg);
    } finally {
      setTmdbFetching(false);
    }
  };

  const canBuild = tmdbEntries.length > 0 || doubanEntries.length > 0;

  const tabs: { key: Tab; label: string }[] = [
    { key: "douban", label: "豆瓣" },
    { key: "imdb", label: "TMDb" },
    { key: "mdl", label: "MyDramaList" },
    { key: "merge", label: "統合" },
  ];

  return (
    <div>
      {/* Working directory selector */}
      <div className="card" style={{ marginBottom: 16 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ fontSize: 13, fontWeight: 600, whiteSpace: "nowrap" }}>作業ディレクトリ</span>
          <input
            className="form-input"
            value={workDir}
            onChange={(e) => setWorkDir(e.target.value)}
            onBlur={() => { if (workDir) restoreFromWorkDir(workDir); }}
            placeholder="C:\Users\...\project"
            style={{ flex: 1, fontSize: 12 }}
          />
          <button
            className="btn btn-secondary"
            onClick={handleSelectWorkDir}
            style={{ fontSize: 12, whiteSpace: "nowrap" }}
          >
            <FolderOpen size={14} />
            参照
          </button>
        </div>
      </div>

      {/* Drama search panel */}
      <DramaSearchPanel
        titleZh={searchTitleZh}
        titleEn={searchTitleEn}
        year={searchYear}
        onTitleZhChange={setSearchTitleZh}
        onTitleEnChange={setSearchTitleEn}
        onYearChange={setSearchYear}
        onTmdbAutoSelect={handleSearchTmdbAutoSelect}
        onDoubanAutoSelect={handleSearchDoubanAutoSelect}
        onTmdbCandidates={handleSearchTmdbCandidates}
      />

      {/* Tab bar */}
      <div style={{ display: "flex", gap: 4, marginBottom: 16, borderBottom: "1px solid var(--border)" }}>
        {tabs.map((t) => (
          <button
            key={t.key}
            className={`btn ${activeTab === t.key ? "btn-primary" : "btn-secondary"}`}
            onClick={() => setActiveTab(t.key)}
            style={{
              fontSize: 13,
              padding: "6px 16px",
              borderRadius: "6px 6px 0 0",
              borderBottom: activeTab === t.key ? "2px solid var(--accent)" : undefined,
            }}
          >
            {t.label}
          </button>
        ))}
      </div>

      {/* 豆瓣 tab */}
      {activeTab === "douban" && (
        <div className="card">
          <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 8 }}>
            豆瓣（ドラマページ / キャストページ）
          </h3>
          <p style={{ fontSize: 11, color: "var(--text-muted)", marginBottom: 8 }}>
            ドラマ本体ページを入力した場合は、自動で /celebrities ページからキャストを取得します。
          </p>
          <div style={{ display: "flex", gap: 6, marginBottom: 8 }}>
            <input
              className="form-input"
              placeholder="https://movie.douban.com/subject/36809858/"
              value={doubanUrl}
              onChange={(e) => setDoubanUrl(e.target.value)}
              style={{ flex: 1, fontSize: 12 }}
            />
            <button
              className="btn btn-primary"
              onClick={handleDoubanFetch}
              disabled={doubanFetching || !doubanUrl.trim()}
              style={{ fontSize: 12, whiteSpace: "nowrap" }}
            >
              {doubanFetching ? <Loader2 size={14} className="spin" /> : <Globe size={14} />}
              {" "}自動取得
            </button>
          </div>
          {doubanDrama && (
            <details style={{ marginBottom: 8 }}>
              <summary style={{ fontSize: 12, cursor: "pointer", color: "var(--text-secondary)" }}>
                {doubanDrama.title ? `作品情報 — ${doubanDrama.title}` : "作品情報"}
              </summary>
              {doubanDrama.synopsis && (
                <p style={{ fontSize: 12, color: "var(--text-muted)", marginTop: 6, lineHeight: 1.5 }}>
                  {doubanDrama.synopsis}
                </p>
              )}
            </details>
          )}
          {doubanEntries.length > 0 && (
            <div style={{ marginTop: 8 }}>
              <div style={{ fontSize: 12, color: "var(--success)", marginBottom: 6 }}>
                取得結果: {doubanEntries.length}件
              </div>
              <div style={{ maxHeight: 300, overflowY: "auto", fontSize: 12, border: "1px solid var(--border)", borderRadius: 3 }}>
                <table style={{ width: "100%", borderCollapse: "collapse" }}>
                  <thead>
                    <tr style={{ backgroundColor: "var(--bg-table-head)", fontSize: 11 }}>
                      <th style={{ padding: "4px 8px", textAlign: "left" }}>Actor ZH</th>
                      <th style={{ padding: "4px 8px", textAlign: "left" }}>Actor EN</th>
                      <th style={{ padding: "4px 8px", textAlign: "left" }}>Character ZH</th>
                    </tr>
                  </thead>
                  <tbody>
                    {doubanEntries.map((e, i) => {
                      // Split actor_name into CN/EN if it contains both
                      const hasCnEn = /[一-鿿]/.test(e.actor_name) && /[a-zA-Z]/.test(e.actor_name);
                      let actorCn = "";
                      let actorEn = "";
                      if (hasCnEn) {
                        const m = e.actor_name.match(/^([一-鿿\s·]+)\s*([a-zA-Z\s.]+)$/);
                        if (m) { actorCn = m[1].trim(); actorEn = m[2].trim(); }
                      }
                      return (
                        <tr key={i}>
                          <td style={{ padding: "4px 8px", borderBottom: "1px solid var(--border)" }}>
                            {actorCn || (hasCnEn ? "" : e.actor_name)}
                          </td>
                          <td style={{ padding: "4px 8px", borderBottom: "1px solid var(--border)" }}>
                            {actorEn || (hasCnEn ? "" : <span style={{ color: "var(--text-muted)" }}>—</span>)}
                          </td>
                          <td style={{ padding: "4px 8px", borderBottom: "1px solid var(--border)" }}>
                            {e.character_name || <span style={{ color: "var(--text-muted)" }}>—</span>}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </div>
      )}

      {/* TMDb tab */}
      {activeTab === "imdb" && (
        <div className="card">
          <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 8 }}>
            TMDb (英語キャスト検索)
          </h3>

          {/* Search section */}
          <div style={{ display: "flex", gap: 6, marginBottom: 8 }}>
            <input
              className="form-input"
              placeholder="作品タイトルで検索（例: 冰湖重生 / Rebirth / The Double）"
              value={tmdbQuery}
              onChange={(e) => setTmdbQuery(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") handleTmdbSearch(); }}
              style={{ flex: 1, fontSize: 12 }}
            />
            <button
              className="btn btn-primary"
              onClick={handleTmdbSearch}
              disabled={tmdbSearching || !tmdbQuery.trim()}
              style={{ fontSize: 12, whiteSpace: "nowrap" }}
            >
              {tmdbSearching ? <Loader2 size={14} className="spin" /> : <Search size={14} />}
              {" "}TMDbで検索
            </button>
          </div>
          <p style={{ fontSize: 11, color: "var(--text-muted)", marginBottom: 8 }}>
            TMDb APIキー (<code>TMDB_API_KEY</code>) が設定ページで登録されている必要があります。
          </p>

          {/* Search results */}
          {tmdbResults.length > 0 && (
            <div style={{ marginBottom: 12, border: "1px solid var(--border)", borderRadius: 3 }}>
              <div style={{ fontSize: 12, fontWeight: 600, padding: "6px 10px", backgroundColor: "var(--bg-table-head)", borderBottom: "1px solid var(--border)" }}>
                候補一覧（{tmdbResults.length}件）
              </div>
              {tmdbResults.map((r) => (
                <div
                  key={r.tmdb_id}
                  onClick={() => { setSelectedTmdbId(r.tmdb_id); setSelectedMediaType(r.media_type); }}
                  style={{
                    padding: "8px 10px",
                    borderBottom: "1px solid var(--border)",
                    cursor: "pointer",
                    backgroundColor: selectedTmdbId === r.tmdb_id ? "var(--bg-hover)" : undefined,
                    display: "flex",
                    alignItems: "center",
                    gap: 8,
                  }}
                >
                  <input
                    type="radio"
                    checked={selectedTmdbId === r.tmdb_id}
                    onChange={() => { setSelectedTmdbId(r.tmdb_id); setSelectedMediaType(r.media_type); }}
                  />
                  <div style={{ flex: 1, fontSize: 12 }}>
                    <strong>{r.title}</strong>
                    {r.original_title && r.original_title !== r.title && (
                      <span style={{ color: "var(--text-muted)", marginLeft: 4 }}>({r.original_title})</span>
                    )}
                    <span style={{ color: "var(--text-muted)", marginLeft: 8 }}>
                      {r.year || "年不明"} — {r.media_type === "tv" ? "TV" : "Movie"}
                    </span>
                    <span style={{ color: "var(--text-muted)", marginLeft: 8, fontSize: 10 }}>
                      ID: {r.tmdb_id}
                    </span>
                    {r.overview && (
                      <div style={{ color: "var(--text-secondary)", marginTop: 2, fontSize: 11, lineHeight: 1.4, display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical", overflow: "hidden" }}>
                        {r.overview}
                      </div>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}

          {/* Select and fetch credits */}
          <div style={{ display: "flex", gap: 6, marginBottom: 12 }}>
            <button
              className="btn btn-primary"
              onClick={handleTmdbFetchCredits}
              disabled={tmdbFetching || selectedTmdbId === null || !selectedMediaType}
              style={{ fontSize: 12 }}
            >
              {tmdbFetching ? <Loader2 size={14} className="spin" /> : <Globe size={14} />}
              {" "}この作品を使う
            </button>
          </div>

          {tmdbError && (
            <div style={{ fontSize: 12, color: "var(--error)", marginBottom: 8 }}>
              {tmdbError}
            </div>
          )}

          {/* Credits result */}
          {tmdbDrama?.synopsis && (
            <div className="synopsis-card">
              <div className="synopsis-label">あらすじ (TMDb){tmdbDrama.title ? ` — ${tmdbDrama.title}` : ""}</div>
              <p>{tmdbDrama.synopsis}</p>
            </div>
          )}

          {tmdbEntries.length > 0 && (
            <div style={{ marginTop: 8, marginBottom: 12, fontSize: 12, color: "var(--success)" }}>
              TMDb credits: {tmdbEntries.length}件
            </div>
          )}
          {tmdbEntries.length > 0 && (
            <div style={{ marginBottom: 12, maxHeight: 200, overflowY: "auto", fontSize: 12, border: "1px solid var(--border)", borderRadius: 3 }}>
              <table style={{ width: "100%", borderCollapse: "collapse" }}>
                <thead>
                  <tr style={{ backgroundColor: "var(--bg-table-head)", fontSize: 11 }}>
                    <th style={{ padding: "4px 8px", textAlign: "left" }}>Actor EN</th>
                    <th style={{ padding: "4px 8px", textAlign: "left" }}>Character EN</th>
                  </tr>
                </thead>
                <tbody>
                  {tmdbEntries.map((e, i) => (
                    <tr key={i}>
                      <td style={{ padding: "4px 8px", borderBottom: "1px solid var(--border)" }}>{e.actor_name}</td>
                      <td style={{ padding: "4px 8px", borderBottom: "1px solid var(--border)" }}>{e.character_name}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

        </div>
      )}

      {/* MyDramaList HTML paste tab */}
      {activeTab === "mdl" && (
        <div className="card">
          <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 8 }}>
            MyDramaList — Cast HTML 貼り付け
          </h3>

          <p style={{ fontSize: 11, color: "var(--text-muted)", marginBottom: 8 }}>
            MDLのCast欄で範囲選択してコピー（Ctrl+C）し、下の枠内に貼り付けてください（Ctrl+V）。
          </p>

          <div style={{ marginBottom: 12 }}>
            <button
              className="btn btn-secondary btn-sm"
              onClick={handleMdlGoogleSearch}
              disabled={!searchTitleZh.trim() && !searchTitleEn.trim()}
            >
              MyDramaListをGoogleで探す
            </button>
          </div>

          {/* Paste target */}
          <div
            onPaste={handleMdlHtmlPaste}
            style={{
              border: "1px dashed var(--border)",
              borderRadius: 4,
              padding: mdlPasteContent ? 8 : 32,
              minHeight: 80,
              cursor: "pointer",
              backgroundColor: "var(--bg-input)",
              fontFamily: "monospace",
              fontSize: 12,
              marginBottom: 8,
              whiteSpace: "pre-wrap",
              wordBreak: "break-all",
              overflow: "auto",
              maxHeight: 200,
            }}
            tabIndex={0}
          >
            {!mdlPasteContent ? (
              <span style={{ color: "var(--text-muted)", display: "flex", alignItems: "center", justifyContent: "center", gap: 6, height: "100%", minHeight: 40 }}>
                <ClipboardPaste size={16} />
                ここに貼り付けてください（Ctrl+V）
              </span>
            ) : mdlPasteFormat === "html" ? (
              <div>
                <div style={{ marginBottom: 4 }}>
                  <span style={{ color: "var(--success)", fontWeight: 600, fontSize: 11 }}>貼り付け形式: HTML</span>
                  <span style={{ color: "var(--text-muted)", marginLeft: 8, fontSize: 11 }}>{mdlPasteContent.length}文字</span>
                </div>
                {/* Show compressed preview: first ~300 chars */}
                <div style={{ color: "var(--text-secondary)", fontSize: 11, lineHeight: 1.4 }}>
                  {mdlPasteContent.substring(0, 300)}
                  {mdlPasteContent.length > 300 ? "..." : ""}
                </div>
              </div>
            ) : (
              <div>
                <div style={{ marginBottom: 4 }}>
                  <span style={{ color: "var(--warning)", fontWeight: 600, fontSize: 11 }}>貼り付け形式: テキスト（HTML情報なし）</span>
                  <span style={{ color: "var(--text-muted)", marginLeft: 8, fontSize: 11 }}>{mdlPasteContent.length}文字</span>
                </div>
                <div style={{ color: "var(--text-secondary)", fontSize: 11 }}>
                  ブラウザのCast欄を範囲選択してコピーすると、HTML形式で貼り付けられます。
                </div>
              </div>
            )}
          </div>

          <div style={{ display: "flex", gap: 8 }}>
            <button
              className="btn btn-primary"
              onClick={handleParseMdlHtml}
              disabled={mdlHtmlParsing || !mdlPasteContent.trim() || mdlPasteFormat !== "html"}
              style={{ fontSize: 12 }}
            >
              {mdlHtmlParsing ? <Loader2 size={14} className="spin" /> : <ClipboardPaste size={14} />}
              {" "}HTMLから抽出
            </button>
            {mdlPasteContent && (
              <button
                className="btn btn-secondary"
                onClick={() => { setMdlPasteContent(""); setMdlPasteFormat("empty"); setMdlHtmlEntries([]); setMdlHtmlError(null); }}
                style={{ fontSize: 12, marginLeft: "auto" }}
              >
                クリア
              </button>
            )}
          </div>

          {mdlHtmlError && (
            <div style={{ marginTop: 8, fontSize: 12, color: "var(--error)" }}>
              {mdlHtmlError}
            </div>
          )}

          {mdlHtmlEntries.length > 0 && (
            <>
              <div style={{ marginTop: 8, fontSize: 12, color: "var(--success)" }}>
                {mdlHtmlEntries.length} 件抽出（characterあり: {mdlHtmlEntries.filter(e => e.character_name).length}件）
              </div>
              <div style={{ marginTop: 8, maxHeight: 300, overflowY: "auto", fontSize: 12, border: "1px solid var(--border)", borderRadius: 3 }}>
                <table style={{ width: "100%", borderCollapse: "collapse" }}>
                  <thead>
                    <tr style={{ backgroundColor: "var(--bg-table-head)", fontSize: 11 }}>
                      <th style={{ padding: "4px 8px", textAlign: "left" }}>Actor EN</th>
                      <th style={{ padding: "4px 8px", textAlign: "left" }}>Character EN</th>
                      <th style={{ padding: "4px 8px", textAlign: "left" }}>Role</th>
                      <th style={{ padding: "4px 8px", textAlign: "left" }}>Source</th>
                    </tr>
                  </thead>
                  <tbody>
                    {mdlHtmlEntries.map((e, i) => (
                      <tr key={i}>
                        <td style={{ padding: "4px 8px", borderBottom: "1px solid var(--border)" }}>{e.actor_name}</td>
                        <td style={{ padding: "4px 8px", borderBottom: "1px solid var(--border)" }}>{e.character_name || <span style={{ color: "var(--text-muted)" }}>—</span>}</td>
                        <td style={{ padding: "4px 8px", borderBottom: "1px solid var(--border)" }}>{e.role_type || "—"}</td>
                        <td style={{ padding: "4px 8px", borderBottom: "1px solid var(--border)" }}>
                          <span className="badge badge-mdl">MDL</span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          )}
        </div>
      )}

      {/* 統合 tab */}
      {activeTab === "merge" && (
        <>
          {error && (
            <div className="card" style={{ borderColor: "var(--error)", color: "var(--error)", marginBottom: 16, fontSize: 13 }}>
              {error}
            </div>
          )}

          {(doubanDrama?.synopsis || tmdbDrama?.synopsis) && (
            <div className="card" style={{ marginBottom: 16 }}>
              <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 8 }}>あらすじ</h3>
              <div style={{ display: "flex", gap: 16 }}>
                {doubanDrama?.synopsis && (
                  <div style={{ flex: 1 }}>
                    <div className="synopsis-label">豆瓣 (中国語){doubanDrama.title ? ` — ${doubanDrama.title}` : ""}</div>
                    <p style={{ fontSize: 13, lineHeight: 1.6, margin: 0 }}>{doubanDrama.synopsis}</p>
                  </div>
                )}
                {tmdbDrama?.synopsis && (
                  <div style={{ flex: 1 }}>
                    <div className="synopsis-label">TMDb (英語){tmdbDrama.title ? ` — ${tmdbDrama.title}` : ""}</div>
                    <p style={{ fontSize: 13, lineHeight: 1.6, margin: 0 }}>{tmdbDrama.synopsis}</p>
                  </div>
                )}
              </div>

              {/* あらすじをまとめる button */}
              <div style={{ marginTop: 12, display: "flex", alignItems: "center", gap: 8 }}>
                <button
                  className="btn btn-primary"
                  onClick={handleSummarizeSynopsis}
                  disabled={summarizing}
                  style={{ fontSize: 13, padding: "8px 20px" }}
                >
                  {summarizing ? <Loader2 size={16} className="spin" /> : <FileText size={16} />}
                  {summarizing ? "要約中..." : synopsisSummary ? "あらすじをまとめる（再実行）" : "あらすじをまとめる"}
                </button>
                {synopsisSummary && !summarizing && (
                  <span style={{ fontSize: 11, color: "var(--success)" }}>要約済み</span>
                )}
              </div>

              {summaryError && (
                <div style={{ marginTop: 8, fontSize: 12, color: "var(--error)" }}>{summaryError}</div>
              )}

              {/* Human summary */}
              {synopsisSummary?.human_summary_ja && (
                <div className="synopsis-card" style={{ marginTop: 12 }}>
                  <div className="synopsis-label">人間用あらすじ（日本語）</div>
                  <p style={{ fontSize: 13, lineHeight: 1.7, margin: 0 }}>{synopsisSummary.human_summary_ja}</p>
                </div>
              )}

              {/* Translation context memo (short) */}
              {synopsisSummary?.translation_context_short_ja && (
                <div className="synopsis-card" style={{ marginTop: 12 }}>
                  <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 8 }}>
                    <div className="synopsis-label" style={{ marginBottom: 0 }}>翻訳用背景メモ</div>
                    <button
                      className="btn btn-secondary btn-sm"
                      onClick={() => {
                        navigator.clipboard.writeText(synopsisSummary.translation_context_short_ja);
                        addLog("info", "Synopsis", "翻訳用背景メモをクリップボードにコピーしました");
                      }}
                      style={{ fontSize: 11 }}
                    >
                      翻訳用背景メモをコピー
                    </button>
                  </div>
                  <p style={{ fontSize: 13, lineHeight: 1.7, margin: 0 }}>{synopsisSummary.translation_context_short_ja}</p>
                </div>
              )}
            </div>
          )}

          {/* Proper noun table (shown even without synopsis cards if restored) */}
          {synopsisSummary && synopsisSummary.proper_nouns.length > 0 && (
            <div className="card" style={{ marginBottom: 16 }}>
              <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 8 }}>
                固有名詞対応表（{synopsisSummary.proper_nouns.length}件）
              </h3>
              <div className="table-container">
                <table>
                  <thead>
                    <tr>
                      <th>中国語</th>
                      <th>英語</th>
                      <th>日本語漢字</th>
                      <th>ソース</th>
                    </tr>
                  </thead>
                  <tbody>
                    {synopsisSummary.proper_nouns.map((noun, i) => (
                      <tr key={i}>
                        <td>{noun.chinese}</td>
                        <td>{noun.english}</td>
                        <td>
                          <input
                            className="form-input"
                            value={editablePnKanji[i] ?? noun.japanese_kanji ?? ""}
                            onChange={(ev) => setEditablePnKanji(prev => ({
                              ...prev,
                              [i]: ev.target.value
                            }))}
                            onBlur={() => {
                              const val = editablePnKanji[i];
                              if (val !== undefined && val !== noun.japanese_kanji) {
                                const updated = { ...synopsisSummary, proper_nouns: [...synopsisSummary.proper_nouns] };
                                updated.proper_nouns[i] = {
                                  ...updated.proper_nouns[i],
                                  japanese_kanji: val,
                                  ja_kanji_source: "manual",
                                  ja_kanji_confidence: 1.0,
                                };
                                setSynopsisSummary(updated);
                                addLog("info", "JaKanji", `proper noun manual edit: ${noun.japanese_kanji || "(empty)"} -> ${val}`);
                              }
                            }}
                            placeholder="日本語漢字"
                            style={{ width: "100%", fontSize: 12 }}
                          />
                        </td>
                        <td>
                          {(noun.ja_kanji_source === "pending_llm" || noun.ja_kanji_source === "rule" || !noun.ja_kanji_source) && (
                            <span className="badge badge-pending" style={{ fontSize: 9 }}>保留</span>
                          )}
                          {noun.ja_kanji_source === "llm" && (
                            <span className="badge badge-llm" style={{ fontSize: 9 }}>LLM</span>
                          )}
                          {noun.ja_kanji_source === "manual" && (
                            <span className="badge badge-manual" style={{ fontSize: 9 }}>手動</span>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}

          {(tmdbEntries.length > 0 || doubanEntries.length > 0 || mdlHtmlEntries.length > 0) && (
            <div className="card" style={{ marginBottom: 16 }}>
              <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 8 }}>俳優・登場人物リスト</h3>
              <div style={{ display: "flex", gap: 16 }}>
                {tmdbEntries.length > 0 && (
                  <div style={{ flex: 1 }}>
                    <h4 style={{ fontSize: 12, color: "var(--text-secondary)", marginBottom: 4 }}>
                      TMDb ({tmdbEntries.length}件)
                    </h4>
                    <div style={{ maxHeight: 200, overflowY: "auto", fontSize: 12 }}>
                      {tmdbEntries.map((e, i) => (
                        <div key={i} style={{ padding: "2px 0", borderBottom: "1px solid var(--border)" }}>
                          <strong>{e.actor_name}</strong> → {e.character_name}
                          {e.role_type && <span style={{ color: "var(--text-muted)", marginLeft: 4 }}>[{e.role_type}]</span>}
                        </div>
                      ))}
                    </div>
                  </div>
                )}
                {doubanEntries.length > 0 && (
                  <div style={{ flex: 1 }}>
                    <h4 style={{ fontSize: 12, color: "var(--text-secondary)", marginBottom: 4 }}>
                      豆瓣 ({doubanEntries.length}件)
                    </h4>
                    <div style={{ maxHeight: 200, overflowY: "auto", fontSize: 12 }}>
                      {doubanEntries.map((e, i) => (
                        <div key={i} style={{ padding: "2px 0", borderBottom: "1px solid var(--border)" }}>
                          <strong>{e.actor_name}</strong> → {e.character_name}
                        </div>
                      ))}
                    </div>
                  </div>
                )}
                {mdlHtmlEntries.length > 0 && (
                  <div style={{ flex: 1 }}>
                    <h4 style={{ fontSize: 12, color: "var(--text-secondary)", marginBottom: 4 }}>
                      MDL ({mdlHtmlEntries.length}件)
                    </h4>
                    <div style={{ maxHeight: 200, overflowY: "auto", fontSize: 12 }}>
                      {mdlHtmlEntries.map((e, i) => (
                        <div key={i} style={{ padding: "2px 0", borderBottom: "1px solid var(--border)" }}>
                          <strong>{e.actor_name}</strong> → {e.character_name}
                          {e.role_type && <span style={{ color: "var(--text-muted)", marginLeft: 4 }}>[{e.role_type}]</span>}
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            </div>
          )}

          {/* 俳優・登場人物をまとめる */}
          {(tmdbEntries.length > 0 || doubanEntries.length > 0 || mdlHtmlEntries.length > 0) && (
            <div className="card" style={{ marginBottom: 16 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
                <button
                  className="btn btn-primary"
                  onClick={handleMergeCast}
                  disabled={mergingCast}
                  style={{ fontSize: 13, padding: "8px 20px" }}
                >
                  {mergingCast ? <Loader2 size={16} className="spin" /> : <GitMerge size={16} />}
                  {mergingCast ? "統合中（日本語漢字生成含む）..." : mergedCast ? "俳優・登場人物をまとめる（再実行）" : "俳優・登場人物をまとめる（日本語漢字生成含む）"}
                </button>
                {mergedCast && !mergingCast && (
                  <>
                    <span style={{ fontSize: 11, color: "var(--success)" }}>統合済み {mergedCast.length}件</span>
                    <button
                      className="btn btn-secondary"
                      onClick={handleDownloadCastCsv}
                      style={{ fontSize: 12, marginLeft: 8 }}
                    >
                      <Download size={14} />
                      CSVダウンロード
                    </button>
                  </>
                )}
              </div>

              {mergedCastError && (
                <div style={{ fontSize: 12, color: "var(--error)" }}>{mergedCastError}</div>
              )}

              {mergedCast && mergedCast.length > 0 && (
                <div className="table-container" style={{ marginTop: 8 }}>
                  <table>
                    <thead>
                      <tr>
                        <th>俳優名・中国語</th>
                        <th>俳優名・英語 (Douban)</th>
                        <th>俳優名・英語 (照合)</th>
                        <th>役名・中国語</th>
                        <th>役名・英語</th>
                        <th>役名・日本語漢字</th>
                      </tr>
                    </thead>
                    <tbody>
                      {mergedCast.map((e, i) => (
                        <tr key={i}>
                          <td>{e.actor_zh || "—"}</td>
                          <td>{e.actor_en_douban || "—"}</td>
                          <td>{e.actor_en_matched || "—"}</td>
                          <td>{e.character_zh || "—"}</td>
                          <td>{e.character_en || "—"}</td>
                          <td>
                            <input
                              className="form-input"
                              value={editableJaKanji[i] ?? e.character_ja_kanji ?? ""}
                              onChange={(ev) => setEditableJaKanji(prev => ({
                                ...prev,
                                [i]: ev.target.value
                              }))}
                              onBlur={() => {
                                const val = editableJaKanji[i];
                                if (val !== undefined && val !== e.character_ja_kanji) {
                                  const oldVal = e.character_ja_kanji || "(empty)";
                                  const updated = [...mergedCast];
                                  updated[i] = {
                                    ...updated[i],
                                    character_ja_kanji: val,
                                    character_ja_kanji_source: "manual",
                                    character_ja_kanji_confidence: 1.0,
                                  };
                                  setMergedCast(updated);
                                  addLog("info", "JaKanji", `manual edit: ${oldVal} -> ${val}`);
                                }
                              }}
                              placeholder="日本語漢字"
                              style={{ width: "100%", fontSize: 12 }}
                            />
                            {(e.character_ja_kanji_source === "pending_llm" || e.character_ja_kanji_source === "rule" || !e.character_ja_kanji_source) && (
                              <span className="badge badge-pending" style={{ fontSize: 9, marginLeft: 4 }}>保留</span>
                            )}
                            {e.character_ja_kanji_source === "llm" && (
                              <span className="badge badge-llm" style={{ fontSize: 9, marginLeft: 4 }}>LLM</span>
                            )}
                            {e.character_ja_kanji_source === "manual" && (
                              <span className="badge badge-manual" style={{ fontSize: 9, marginLeft: 4 }}>手動</span>
                            )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}

            </div>
          )}

          <div style={{ marginBottom: 16 }}>
            {mergedCast && (() => {
              const pendingKanji = mergedCast.filter(
                e => e.character_ja_kanji_source === "pending_llm" || e.character_ja_kanji_source === "rule" || !e.character_ja_kanji_source
              );
              if (pendingKanji.length > 0) {
                return (
                  <div style={{ background: "var(--error-bg)", color: "var(--error)", padding: "8px 12px", borderRadius: 4, marginBottom: 12, fontSize: 13, maxWidth: 600 }}>
                    {pendingKanji.length}件のキャストにLLM漢字変換が未処理です。「俳優・登場人物をまとめる」を先に実行してください（漢字変換含む）。未処理のエントリはプロンプトから除外されます。
                  </div>
                );
              }
              return null;
            })()}
            <button
              className="btn btn-primary"
              onClick={handleBuild}
              disabled={loading || !canBuild}
              style={{ fontSize: 14, padding: "8px 20px" }}
            >
              <GitMerge size={18} />
              {loading ? "処理中..." : "プロンプトを構築"}
            </button>
          </div>

          {dict && (
            <div className="card" style={{ marginBottom: 16 }}>
              <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 12 }}>
                <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 0 }}>翻訳プロンプト</h3>
                <div style={{ display: "flex", gap: 8 }}>
                  <button className="btn btn-primary" onClick={handleCopyPrompt} style={{ fontSize: 12 }}>
                    <Copy size={14} />
                    プロンプトをコピー
                  </button>
                  <button className="btn btn-primary" onClick={handleSavePrompt} style={{ fontSize: 12 }}>
                    <Save size={14} />
                    プロンプトを保存
                  </button>
                </div>
              </div>
              <pre style={{
                background: "var(--bg-secondary)",
                border: "1px solid var(--border)",
                borderRadius: 6,
                padding: "16px 20px",
                fontSize: 13,
                lineHeight: 1.8,
                whiteSpace: "pre-wrap",
                wordBreak: "break-word",
                maxHeight: 500,
                overflowY: "auto",
                margin: 0,
              }}>
                {buildPromptText({ synopsisSummary, dict, editablePnKanji })}
              </pre>
            </div>
          )}
        </>
      )}
    </div>
  );
}

