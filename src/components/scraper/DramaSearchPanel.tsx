import { useState, useCallback } from "react";
import { searchTmdb, searchDatabaseUrl } from "../../lib/tauri";
import { useAppLogStore } from "../../stores/useAppLogStore";
import type { TmdbSearchResult, SearchCandidate, DramaSearchQuery } from "../../types";

// ---------------------------------------------------------------------------
// Scoring (mirrors Rust score_search_candidate)
// ---------------------------------------------------------------------------

function scoreCandidate(
  query_zh: string,
  query_en: string,
  aliases: string[],
  candidateTitle: string,
  candidateYear: string | null,
  expectedYear: string | null,
): { confidence: number; reason: string } {
  const qZh = query_zh.trim();
  const qEn = query_en.trim().toLowerCase();
  const cand = candidateTitle.trim();
  const candLower = cand.toLowerCase();

  if (qZh && cand.includes(qZh)) {
    return { confidence: 1.0, reason: "title_exact_zh" };
  }

  if (qEn && (candLower === qEn || candLower.includes(qEn))) {
    const yearMatch = candidateYear && expectedYear && candidateYear === expectedYear;
    if (yearMatch) {
      return { confidence: 0.95, reason: "title_exact_en+year" };
    }
    return { confidence: 0.85, reason: "title_exact_en" };
  }

  for (const alias of aliases) {
    const a = alias.trim().toLowerCase();
    if (a && (candLower === a || candLower.includes(a))) {
      return { confidence: 0.80, reason: "alias_match" };
    }
  }

  const enWords = qEn.split(/\s+/).filter(Boolean);
  if (enWords.length >= 2) {
    const matching = enWords.filter((w) => candLower.includes(w)).length;
    if (matching >= enWords.length) {
      return { confidence: 0.70, reason: "partial_match_all_words" };
    }
    if (matching >= Math.floor(enWords.length / 2)) {
      return { confidence: 0.50, reason: "partial_match_some_words" };
    }
  }

  return { confidence: 0.0, reason: "no_match" };
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ScoredTmdbResult {
  result: TmdbSearchResult;
  confidence: number;
  reason: string;
}

interface SearchStatus {
  tmdb: "idle" | "searching" | "auto" | "manual" | "error";
  douban: "idle" | "searching" | "auto" | "error";
}

interface DramaSearchPanelProps {
  titleZh: string;
  titleEn: string;
  year: string;
  onTitleZhChange: (v: string) => void;
  onTitleEnChange: (v: string) => void;
  onYearChange: (v: string) => void;
  onTmdbAutoSelect: (result: TmdbSearchResult) => void;
  onDoubanAutoSelect: (url: string) => void;
  onTmdbCandidates: (results: TmdbSearchResult[]) => void;
}

export default function DramaSearchPanel({
  titleZh,
  titleEn,
  year,
  onTitleZhChange,
  onTitleEnChange,
  onYearChange,
  onTmdbAutoSelect,
  onDoubanAutoSelect,
  onTmdbCandidates,
}: DramaSearchPanelProps) {

  const [status, setStatus] = useState<SearchStatus>({ tmdb: "idle", douban: "idle" });
  const [tmdbBest, setTmdbBest] = useState<ScoredTmdbResult | null>(null);
  const [tmdbCandidates, setTmdbCandidates] = useState<ScoredTmdbResult[]>([]);
  const [doubanResult, setDoubanResult] = useState<SearchCandidate | null>(null);
  const [error, setError] = useState<string | null>(null);

  const addLog = useAppLogStore((s) => s.addLog);

  const buildQuery = useCallback((): DramaSearchQuery => ({
    title_zh: titleZh.trim(),
    title_en: titleEn.trim(),
    aliases: [],
    year: year.trim() || null,
  }), [titleZh, titleEn, year]);

  const handleSearch = useCallback(async () => {
    const qZh = titleZh.trim();
    const qEn = titleEn.trim();
    if (!qZh && !qEn) return;

    setError(null);
    setTmdbBest(null);
    setTmdbCandidates([]);
    setDoubanResult(null);
    setStatus({ tmdb: "searching", douban: "searching" });

    const query = buildQuery();
    const expectedYear = year.trim() || null;

    const [tmdbOutcome, doubanOutcome] = await Promise.allSettled([
      (async () => {
        const allResults: TmdbSearchResult[] = [];
        const seenIds = new Set<number>();

        for (const q of [qZh, qEn]) {
          if (!q) continue;
          try {
            const results = await searchTmdb(q);
            for (const r of results) {
              if (!seenIds.has(r.tmdb_id)) {
                seenIds.add(r.tmdb_id);
                allResults.push(r);
              }
            }
          } catch (e) {
            console.warn(`[TMDb] search failed for "${q}":`, e);
          }
        }

        const scored: ScoredTmdbResult[] = allResults.map((r) => {
          const combinedTitle = r.original_title && r.original_title !== r.title
            ? `${r.title} / ${r.original_title}`
            : r.title;
          const { confidence, reason } = scoreCandidate(
            qZh, qEn, [], combinedTitle, r.year, expectedYear,
          );
          return { result: r, confidence, reason };
        });

        scored.sort((a, b) => b.confidence - a.confidence);
        return scored;
      })(),

      searchDatabaseUrl("douban", query),
    ]);

    // Process TMDb results
    if (tmdbOutcome.status === "fulfilled") {
      const scored = tmdbOutcome.value;
      setTmdbCandidates(scored);

      if (scored.length > 0) {
        const best = scored[0];
        const secondBest = scored.length > 1 ? scored[1] : null;
        const gap = secondBest ? best.confidence - secondBest.confidence : 1.0;

        if (best.confidence >= 0.90 && gap >= 0.10) {
          setTmdbBest(best);
          setStatus((s) => ({ ...s, tmdb: "auto" }));
          addLog("info", "TMDb",
            `selected: ${best.result.title} (ID:${best.result.tmdb_id}) ` +
            `confidence=${best.confidence.toFixed(2)} reason=${best.reason}`,
          );
          onTmdbAutoSelect(best.result);
        } else {
          setStatus((s) => ({ ...s, tmdb: "manual" }));
          addLog("info", "TMDb",
            `candidates=${scored.length}, best=${scored[0]?.result.title} ` +
            `confidence=${scored[0]?.confidence.toFixed(2)} — 候補一覧を表示`,
          );
          onTmdbCandidates(scored.map((s) => s.result));
        }
      } else {
        setStatus((s) => ({ ...s, tmdb: "idle" }));
      }
    } else {
      setStatus((s) => ({ ...s, tmdb: "error" }));
      addLog("error", "TMDb", `search error: ${tmdbOutcome.reason}`);
    }

    // Process Douban results
    if (doubanOutcome.status === "fulfilled") {
      const [best, allCandidates] = doubanOutcome.value;
      setDoubanResult(best);
      addLog("info", "Douban",
        `candidates=${allCandidates.length}` +
        (best
          ? `, selected=${best.title} confidence=${best.confidence.toFixed(2)} reason=${best.reason}`
          : ", no match"),
      );

      if (best) {
        setStatus((s) => ({ ...s, douban: "auto" }));
        onDoubanAutoSelect(best.url);
      } else {
        setStatus((s) => ({ ...s, douban: "idle" }));
      }
    } else {
      setStatus((s) => ({ ...s, douban: "error" }));
      const msg = String(doubanOutcome.reason ?? "不明なエラー");
      setError(`Douban検索エラー: ${msg}`);
      addLog("error", "Douban", `search error: ${msg}`);
    }
  }, [titleZh, titleEn, year, buildQuery, addLog, onTmdbAutoSelect, onDoubanAutoSelect, onTmdbCandidates]);

  return (
    <div className="drama-search-panel">
      <div className="panel-header">
        <strong>作品検索</strong>
      </div>

      <div className="search-field-row">
        <label>
          中国語名:
          <input
            type="text"
            value={titleZh}
            onChange={(e) => onTitleZhChange(e.target.value)}
            placeholder="冰湖重生"
          />
        </label>
        <label>
          英語名:
          <input
            type="text"
            value={titleEn}
            onChange={(e) => onTitleEnChange(e.target.value)}
            placeholder="Rebirth"
          />
        </label>
        <label>
          年:
          <input
            type="text"
            value={year}
            onChange={(e) => onYearChange(e.target.value)}
            placeholder="2025"
            style={{ width: 60 }}
          />
        </label>
        <button onClick={handleSearch} disabled={status.tmdb === "searching"}>
          {status.tmdb === "searching" ? "検索中..." : "検索開始"}
        </button>
      </div>

      {/* Status display */}
      <div className="search-results">
        {/* TMDb status */}
        {status.tmdb !== "idle" && status.tmdb !== "searching" && (
          <div className={`search-status ${status.tmdb === "auto" ? "auto-selected" : "manual-select"}`}>
            <span className="badge badge-tmdb">TMDb</span>
            {tmdbBest ? (
              <>
                <span className="result-title">{tmdbBest.result.title}</span>
                {tmdbBest.result.year && <span className="result-year">({tmdbBest.result.year})</span>}
                <span className="result-type">{tmdbBest.result.media_type}</span>
                <span className="confidence-badge">
                  {tmdbBest.confidence >= 0.90 ? "自動採用" : "候補"}
                  {" "}({tmdbBest.confidence.toFixed(2)})
                </span>
              </>
            ) : (
              <span className="no-result">候補なし</span>
            )}
          </div>
        )}
        {status.tmdb === "manual" && tmdbCandidates.length > 0 && (
          <div className="tmdb-candidates">
            {tmdbCandidates.slice(0, 5).map((c, i) => (
              <div key={c.result.tmdb_id} className="candidate-item">
                <span className="candidate-rank">{i + 1}.</span>
                <span className="result-title">{c.result.title}</span>
                {c.result.year && <span className="result-year">({c.result.year})</span>}
                <span className="result-type">{c.result.media_type}</span>
                <span className="confidence-badge">{c.confidence.toFixed(2)}</span>
                <span className="result-reason">{c.reason}</span>
              </div>
            ))}
          </div>
        )}

        {/* Douban status */}
        {status.douban !== "idle" && status.douban !== "searching" && (
          <div className={`search-status ${status.douban === "auto" ? "auto-selected" : ""}`}>
            <span className="badge badge-douban">Douban</span>
            {doubanResult ? (
              <>
                <span className="result-url">{doubanResult.url}</span>
                <span className="confidence-badge">
                  {doubanResult.confidence >= 0.90 ? "自動採用" : "候補"}
                  {" "}({doubanResult.confidence.toFixed(2)})
                </span>
                <span className="result-reason">{doubanResult.reason}</span>
              </>
            ) : (
              <span className="no-result">候補なし</span>
            )}
          </div>
        )}

      </div>

      {error && <div className="search-error">{error}</div>}
    </div>
  );
}
