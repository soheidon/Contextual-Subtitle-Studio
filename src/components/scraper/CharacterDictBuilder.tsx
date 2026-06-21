import { useState, useRef } from "react";
import { ClipboardPaste, GitMerge, Download, Globe, Loader2, ShieldCheck, FolderOpen, Search } from "lucide-react";
import { parseDoubanPaste, parseTmdbPaste, searchTmdb, scrapeTmdbCredits, buildCharacterDict, scrapeUrl, verifyCharacterDict, saveDramaInfo, loadDramaInfo } from "../../lib/tauri";
import CharacterDictQuality from "./CharacterDictQuality";
import type { PastedEntry, CharacterDict, QualityReport, DramaInfo, TmdbSearchResult } from "../../types";
import { useAppLogStore } from "../../stores/useAppLogStore";

const CONFIDENCE_HIGH = 0.85;
const CONFIDENCE_MEDIUM = 0.60;

type ConfidenceFilter = "All" | "high" | "medium" | "low";

type Tab = "douban" | "imdb" | "merge";

export default function CharacterDictBuilder() {
  const [activeTab, setActiveTab] = useState<Tab>("douban");
  const [tmdbText, setTmdbText] = useState("");
  const [doubanText, setDoubanText] = useState("");
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
  const [doubanEntries, setDoubanEntries] = useState<PastedEntry[]>([]);
  const [dict, setDict] = useState<CharacterDict | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editableDict, setEditableDict] = useState<Record<string, { kanji: string; reading: string }>>({});
  const [workDir, setWorkDir] = useState<string>("");
  const [qualityReport, setQualityReport] = useState<QualityReport | null>(null);
  const [confidenceFilter, setConfidenceFilter] = useState<ConfidenceFilter>("All");
  const addLog = useAppLogStore((s) => s.addLog);

  const restoringRef = useRef(false);

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

  const restoreFromWorkDir = async (dir: string) => {
    restoringRef.current = true;
    try {
      const bundle = await loadDramaInfo(dir);
      if (bundle.metadata?.douban_url) setDoubanUrl(bundle.metadata.douban_url);
      const title = bundle.metadata?.drama_title || "";
      if (bundle.synopsis_douban) setDoubanDrama({ title, synopsis: bundle.synopsis_douban, source: "douban" });
      if (bundle.synopsis_tmdb) setTmdbDrama({ title, synopsis: bundle.synopsis_tmdb, source: "tmdb" });
      if (bundle.cast_douban) setDoubanEntries(bundle.cast_douban);
      if (bundle.cast_tmdb) setTmdbEntries(bundle.cast_tmdb);
      if (bundle.character_dict) {
        setDict(bundle.character_dict);
        const report = await verifyCharacterDict(bundle.character_dict);
        setQualityReport(report);
      }
    } catch {
      // drama_info not found — new project, nothing to restore
    } finally {
      // Allow React to batch all setState calls before re-enabling auto-save
      setTimeout(() => { restoringRef.current = false; }, 500);
    }
  };


  const handleParseTmdb = async () => {
    if (!tmdbText.trim()) return;
    try {
      setLoading(true);
      setError(null);
      const entries = await parseTmdbPaste(tmdbText);
      setTmdbEntries(entries);
      if (workDir) {
        saveDramaInfo(workDir, { metadata: null, synopsis_douban: null, synopsis_tmdb: null, cast_douban: null, cast_tmdb: entries, character_dict: null })
          .catch((e) => console.error("保存エラー:", e));
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleParseDouban = async () => {
    if (!doubanText.trim()) return;
    try {
      setLoading(true);
      setError(null);
      const entries = await parseDoubanPaste(doubanText);
      setDoubanEntries(entries);
      if (workDir) {
        saveDramaInfo(workDir, { metadata: null, synopsis_douban: null, synopsis_tmdb: null, cast_douban: entries, cast_tmdb: null, character_dict: null })
          .catch((e) => console.error("保存エラー:", e));
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleBuild = async () => {
    try {
      setLoading(true);
      setError(null);
      const result = await buildCharacterDict(tmdbEntries, doubanEntries);
      setDict(result);
      const report = await verifyCharacterDict(result);
      setQualityReport(report);
      setConfidenceFilter("All");
      setActiveTab("merge");
      if (workDir) {
        saveDramaInfo(workDir, { metadata: null, synopsis_douban: null, synopsis_tmdb: null, cast_douban: null, cast_tmdb: null, character_dict: result })
          .catch((e) => console.error("保存エラー:", e));
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleDownload = async () => {
    if (!dict) return;
    const merged: CharacterDict = {};
    for (const [key, entry] of Object.entries(dict)) {
      const edit = editableDict[key];
      merged[key] = {
        ...entry,
        role: {
          ...entry.role,
          japanese_kanji: edit?.kanji || entry.role.japanese_kanji,
          japanese_reading: edit?.reading || entry.role.japanese_reading,
        },
      };
    }
    const json = JSON.stringify(merged, null, 2);
    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const { writeTextFile } = await import("@tauri-apps/plugin-fs");
      const defaultPath = workDir ? `${workDir}/characters.json` : undefined;
      const path = await save({
        title: "辞書JSONを保存",
        defaultPath,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (path) {
        await writeTextFile(path, json);
      }
    } catch {
      // fallback: browser download
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "characters.json";
      a.click();
      URL.revokeObjectURL(url);
    }
  };

  const handleDoubanFetch = async () => {
    if (!doubanUrl.trim()) return;
    try {
      setDoubanFetching(true);
      setError(null);
      const result = await scrapeUrl(doubanUrl.trim(), "Douban");
      const title = result.drama_title || result.page_title || "";
      if (result.synopsis) {
        setDoubanDrama({ title, synopsis: result.synopsis, source: "douban" });
      }
      const lines: string[] = [];
      for (const c of result.characters) {
        const actorParts: string[] = [];
        if (c.aliases && c.aliases.length > 0) actorParts.push(c.aliases[0]);
        if (c.actor_name) actorParts.push(c.actor_name);
        lines.push(actorParts.join(" "));
        if (c.character_name) lines.push(`饰 ${c.character_name}`);
      }
      setDoubanText(lines.join("\n"));
      const entries = await parseDoubanPaste(lines.join("\n"));
      setDoubanEntries(entries);
      addLog("success", "豆瓣", `成功 — ${entries.length}件のキャスト取得`);
      // 即時保存
      if (workDir) {
        saveDramaInfo(workDir, {
          metadata: { drama_title: title, douban_url: doubanUrl.trim(), tmdb_url: null, updated_at: new Date().toISOString() },
          synopsis_douban: result.synopsis || null,
          synopsis_tmdb: null,
          cast_douban: entries,
          cast_tmdb: null,
          character_dict: null,
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
      const entries: PastedEntry[] = result.characters.map((c) => ({
        actor_name: c.actor_name || "",
        character_name: c.character_name,
        role_type: c.role_type,
        source: "Tmdb" as const,
      }));
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
          metadata: { drama_title: title, douban_url: null, tmdb_url: null, updated_at: new Date().toISOString() },
          synopsis_douban: null,
          synopsis_tmdb: result.synopsis || null,
          cast_douban: null,
          cast_tmdb: entries,
          character_dict: null,
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
  const dictEntries = dict ? Object.entries(dict) : [];

  const filteredEntries = dictEntries.filter(([_, entry]) => {
    if (confidenceFilter === "All") return true;
    if (confidenceFilter === "high") return entry.confidence >= CONFIDENCE_HIGH;
    if (confidenceFilter === "medium") return entry.confidence >= CONFIDENCE_MEDIUM && entry.confidence < CONFIDENCE_HIGH;
    if (confidenceFilter === "low") return entry.confidence < CONFIDENCE_MEDIUM;
    return true;
  });

  const getConfidenceClass = (conf: number): string => {
    if (conf >= CONFIDENCE_HIGH) return "conf-high";
    if (conf >= CONFIDENCE_MEDIUM) return "conf-medium";
    return "conf-low";
  };

  const getMatchDetailLabel = (detail: string): string => {
    switch (detail) {
      case "ExactPinyin": return "完全一致";
      case "PartialPinyin": return "部分一致";
      case "SingleSource": return "単一ソース";
      case "Inferred": return "推定";
      case "NameVariantExact": return "名前一致";
      case "NameVariantReversed": return "姓名反転一致";
      default: return detail;
    }
  };

  const tabs: { key: Tab; label: string }[] = [
    { key: "douban", label: "豆瓣" },
    { key: "imdb", label: "TMDb" },
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
            豆瓣 (Celebrities Page)
          </h3>
          <div style={{ display: "flex", gap: 6, marginBottom: 8 }}>
            <input
              className="form-input"
              placeholder="https://movie.douban.com/subject/XXXXX/celebrities"
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
          {doubanDrama?.synopsis && (
            <div className="synopsis-card">
              <div className="synopsis-label">あらすじ (豆瓣){doubanDrama.title ? ` — ${doubanDrama.title}` : ""}</div>
              <p>{doubanDrama.synopsis}</p>
            </div>
          )}
          <p style={{ fontSize: 11, color: "var(--text-muted)", marginBottom: 8 }}>
            または、下のテキストエリアに手動で貼り付け
          </p>
          <textarea
            className="form-input"
            rows={14}
            placeholder={"赵丽颖\n饰 楚乔\n林更新\n饰 宇文玥\n..."}
            value={doubanText}
            onChange={(e) => setDoubanText(e.target.value)}
            style={{ resize: "vertical", fontFamily: "inherit", fontSize: 13, marginBottom: 8 }}
          />
          <button
            className="btn btn-primary"
            onClick={handleParseDouban}
            disabled={loading || !doubanText.trim()}
            style={{ fontSize: 12 }}
          >
            <ClipboardPaste size={14} />
            Parse 豆瓣
          </button>
          {doubanEntries.length > 0 && (
            <div style={{ marginTop: 8, fontSize: 12, color: "var(--success)" }}>
              {doubanEntries.length} 件パース成功
            </div>
          )}
          {doubanEntries.length > 0 && (
            <div style={{ marginTop: 8, maxHeight: 200, overflowY: "auto", fontSize: 12 }}>
              {doubanEntries.map((e, i) => (
                <div key={i} style={{ padding: "2px 0", borderBottom: "1px solid var(--border)" }}>
                  <strong>{e.actor_name}</strong> → {e.character_name}
                </div>
              ))}
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

          {/* Paste fallback */}
          <details style={{ marginTop: 8 }}>
            <summary style={{ fontSize: 12, cursor: "pointer", color: "var(--text-secondary)" }}>
              または、手動で貼り付け（IMDbページからコピー）
            </summary>
            <textarea
              className="form-input"
              rows={10}
              placeholder={`Yunrui Li / Zhuge Yue\nMeng Xia / Bai Lu...`}
              value={tmdbText}
              onChange={(e) => setTmdbText(e.target.value)}
              style={{ resize: "vertical", fontFamily: "inherit", fontSize: 13, marginTop: 8, marginBottom: 8 }}
            />
            <button
              className="btn btn-primary"
              onClick={handleParseTmdb}
              disabled={loading || !tmdbText.trim()}
              style={{ fontSize: 12 }}
            >
              <ClipboardPaste size={14} />
              Parse TMDb
            </button>
            {tmdbEntries.length > 0 && !tmdbText && (
              <div style={{ marginTop: 8, fontSize: 12, color: "var(--success)" }}>
                {tmdbEntries.length} 件パース成功
              </div>
            )}
          </details>
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
            </div>
          )}

          {(tmdbEntries.length > 0 || doubanEntries.length > 0) && (
            <div className="card" style={{ marginBottom: 16 }}>
              <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 8 }}>パース結果</h3>
              <div style={{ display: "flex", gap: 16 }}>
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
              </div>
            </div>
          )}

          <div style={{ marginBottom: 16 }}>
            <button
              className="btn btn-primary"
              onClick={handleBuild}
              disabled={loading || !canBuild}
              style={{ fontSize: 14, padding: "8px 20px" }}
            >
              <GitMerge size={18} />
              {loading ? "処理中..." : "辞書を構築"}
            </button>
          </div>

          {dictEntries.length > 0 && (
            <>
              {qualityReport && (
                <CharacterDictQuality
                  report={qualityReport}
                  onFilterChange={setConfidenceFilter}
                  currentFilter={confidenceFilter}
                />
              )}

              <div className="card">
                <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 12 }}>
                  <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 0 }}>
                    構築された辞書 ({filteredEntries.length}/{dictEntries.length}件)
                    {confidenceFilter !== "All" && (
                      <span style={{ fontSize: 11, color: "var(--text-muted)", marginLeft: 8 }}>
                        (フィルター: {confidenceFilter === "high" ? "高信頼" : confidenceFilter === "medium" ? "中信頼" : "低信頼"})
                      </span>
                    )}
                  </h3>
                  <div style={{ display: "flex", gap: 8 }}>
                    <span style={{ fontSize: 11, color: "var(--text-muted)", display: "flex", alignItems: "center" }}>
                      <ShieldCheck size={12} style={{ marginRight: 4 }} />
                      ソース & 信頼度付き
                    </span>
                    <button className="btn btn-primary" onClick={handleDownload} style={{ fontSize: 12 }}>
                      <Download size={14} />
                      JSONダウンロード
                    </button>
                  </div>
                </div>
                <div className="table-container">
                  <table>
                    <thead>
                      <tr>
                        <th>Key</th>
                        <th>Actor (EN)</th>
                        <th>Actor (CN)</th>
                        <th>Role (EN)</th>
                        <th>Role (CN)</th>
                        <th>日本語 (漢字)</th>
                        <th>カタカナ</th>
                        <th>信頼度</th>
                        <th>ソース</th>
                      </tr>
                    </thead>
                    <tbody>
                      {filteredEntries.map(([key, entry]) => {
                        const edit = editableDict[key] || { kanji: entry.role.japanese_kanji, reading: entry.role.japanese_reading };
                        return (
                          <tr key={key}>
                            <td><code>{key}</code></td>
                            <td>{entry.actor.english}</td>
                            <td>{entry.actor.chinese ?? "—"}</td>
                            <td>{entry.role.english ?? "—"}</td>
                            <td>{entry.role.chinese ?? "—"}</td>
                            <td>
                              <input
                                className="form-input"
                                value={edit.kanji}
                                onChange={(e) => setEditableDict(prev => ({
                                  ...prev,
                                  [key]: { ...prev[key] || { kanji: "", reading: "" }, kanji: e.target.value }
                                }))}
                                placeholder="日本語漢字"
                                style={{ width: "100%", fontSize: 12 }}
                              />
                            </td>
                            <td>
                              <input
                                className="form-input"
                                value={edit.reading}
                                onChange={(e) => setEditableDict(prev => ({
                                  ...prev,
                                  [key]: { ...prev[key] || { kanji: "", reading: "" }, reading: e.target.value }
                                }))}
                                placeholder="カタカナ"
                                style={{ width: "100%", fontSize: 12 }}
                              />
                            </td>
                            <td>
                              <span
                                className={getConfidenceClass(entry.confidence)}
                                title={`${getMatchDetailLabel(entry.match_detail)}`}
                                style={{ fontWeight: 600, fontSize: 12 }}
                              >
                                {(entry.confidence * 100).toFixed(0)}%
                              </span>
                            </td>
                            <td>
                              <SourceBadges flags={entry.source_flags} />
                            </td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              </div>
            </>
          )}
        </>
      )}
    </div>
  );
}

function SourceBadges({ flags }: { flags: { douban: boolean; tvmao: boolean; d_addicts: boolean; mdl_paste: boolean; tmdb?: boolean; imdb?: boolean } }) {
  return (
    <div style={{ display: "flex", gap: 2, flexWrap: "wrap" }}>
      {flags.douban && <span className="badge badge-douban" title="豆瓣">豆</span>}
      {(flags.tmdb || flags.imdb) && <span className="badge badge-tmdb" title="TMDb">T</span>}
      {flags.mdl_paste && <span className="badge badge-mdl" title="MDL">M</span>}
      {flags.tvmao && <span className="badge badge-tvmao" title="TVMao">M</span>}
      {flags.d_addicts && <span className="badge badge-daddicts" title="d-addicts">D</span>}
    </div>
  );
}
