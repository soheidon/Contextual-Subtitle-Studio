import { useState } from "react";
import { Globe, Loader2, AlertTriangle, CheckCircle2, ClipboardPaste } from "lucide-react";
import { scrapeUrl } from "../../lib/tauri";
import type { ScrapeResult, ScrapeSource } from "../../types";

interface Props {
  title: string;
  subtitle: string;
  source: ScrapeSource;
  result: ScrapeResult | null;
  onResult: (r: ScrapeResult | null) => void;
  /** If true, shows a manual paste textarea instead of URL input + fetch. */
  allowManualPaste?: boolean;
}

export default function ScraperColumn({
  title,
  subtitle,
  source,
  result,
  onResult,
  allowManualPaste,
}: Props) {
  const [url, setUrl] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<"url" | "paste">("url");

  const handleFetch = async () => {
    if (!url.trim()) return;
    try {
      setLoading(true);
      setError(null);
      const r = await scrapeUrl(url.trim(), source);
      onResult(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const handlePaste = async () => {
    try {
      const text = await navigator.clipboard.readText();
      if (!text.trim()) return;
      setLoading(true);
      setError(null);
      // Create a manual ScrapeResult from pasted content
      const lines = text
        .split(/\n/)
        .map((l) => l.trim())
        .filter((l) => l.length > 0);
      const manual: ScrapeResult = {
        source: { Other: "manual_paste" },
        url: "",
        page_title: null,
        drama_title: null,
        synopsis: null,
        characters: lines.map((line, i) => ({
          source_id: `manual_${String(i).padStart(3, "0")}`,
          character_name: line,
          actor_name: null,
          role_type: null,
          aliases: [],
        })),
        saved_html_path: null,
      };
      onResult(manual);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const sourceLabel =
    typeof source === "object" && "Other" in source
      ? source.Other
      : source;

  return (
    <div className="card" style={{ minWidth: 280, flex: 1 }}>
      <div style={{ marginBottom: 12 }}>
        <h3 style={{ fontSize: 14, fontWeight: 600 }}>{title}</h3>
        <p style={{ fontSize: 12, color: "var(--text-secondary)" }}>
          {subtitle}
        </p>
      </div>

      {allowManualPaste && (
        <div style={{ display: "flex", gap: 4, marginBottom: 8 }}>
          <button
            className={`btn ${mode === "url" ? "btn-primary" : "btn-secondary"}`}
            style={{ fontSize: 12, padding: "3px 10px", minHeight: 24 }}
            onClick={() => setMode("url")}
          >
            <Globe size={12} />
            URL
          </button>
          <button
            className={`btn ${mode === "paste" ? "btn-primary" : "btn-secondary"}`}
            style={{ fontSize: 12, padding: "3px 10px", minHeight: 24 }}
            onClick={() => setMode("paste")}
          >
            <ClipboardPaste size={12} />
            手動貼付
          </button>
        </div>
      )}

      {mode === "url" ? (
        <div style={{ display: "flex", gap: 6, marginBottom: 8 }}>
          <input
            className="form-input"
            placeholder={`${sourceLabel} のURLを入力...`}
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleFetch()}
            style={{ flex: 1 }}
          />
          <button
            className="btn btn-primary"
            onClick={handleFetch}
            disabled={loading || !url.trim()}
            style={{ whiteSpace: "nowrap" }}
          >
            {loading ? <Loader2 size={14} className="spin" /> : <Globe size={14} />}
            取得
          </button>
        </div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 6, marginBottom: 8 }}>
          <textarea
            className="form-input"
            placeholder="キャスト一覧を貼り付け..."
            rows={6}
            style={{ resize: "vertical", fontFamily: "inherit" }}
            onChange={(_e) => {
              // Manual text entry
            }}
          />
          <div style={{ display: "flex", gap: 6 }}>
            <button
              className="btn btn-primary"
              onClick={handlePaste}
              disabled={loading}
              style={{ fontSize: 12 }}
            >
              <ClipboardPaste size={12} />
              クリップボードから貼付
            </button>
          </div>
        </div>
      )}

      {error && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            padding: "6px 10px",
            background: "var(--error-bg)",
            borderRadius: 3,
            marginBottom: 8,
            fontSize: 12,
            color: "var(--error)",
          }}
        >
          <AlertTriangle size={14} />
          {error}
        </div>
      )}

      {result && (
        <div
          style={{
            borderTop: "1px solid var(--border)",
            paddingTop: 8,
            marginTop: 4,
          }}
        >
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 6,
              marginBottom: 6,
              fontSize: 12,
              color: "var(--success)",
              fontWeight: 600,
            }}
          >
            <CheckCircle2 size={14} />
            {result.characters.length} 件取得
          </div>
          {result.drama_title && (
            <p style={{ fontSize: 12, color: "var(--text-secondary)", marginBottom: 4 }}>
              {result.drama_title}
            </p>
          )}
          <ul
            style={{
              fontSize: 12,
              listStyle: "none",
              maxHeight: 240,
              overflowY: "auto",
            }}
          >
            {result.characters.map((c) => (
              <li
                key={c.source_id}
                style={{
                  padding: "3px 6px",
                  borderBottom: "1px solid var(--border)",
                  display: "flex",
                  justifyContent: "space-between",
                }}
              >
                <span>{c.character_name}</span>
                {c.actor_name && (
                  <span style={{ color: "var(--text-muted)" }}>
                    {c.actor_name}
                  </span>
                )}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
