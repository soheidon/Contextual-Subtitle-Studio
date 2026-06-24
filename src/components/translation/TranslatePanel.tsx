import { useState, useEffect, useRef } from "react";
import { Play } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { useSrtStore } from "../../stores/useSrtStore";
import { useDictionaryStore } from "../../stores/useDictionaryStore";
import { useLlmStore } from "../../stores/useLlmStore";
import { useTranslationStore } from "../../stores/useTranslationStore";
import { startTranslation, saveSrtFile } from "../../lib/tauri";
import type { TranslationConfig, TranslationProgress } from "../../types";
import { useNavigate } from "react-router-dom";

export default function TranslatePanel() {
  const { entries: srtEntries, isLoaded: srtLoaded, filePath } = useSrtStore();
  const { characters, glossary } = useDictionaryStore();
  const { active } = useLlmStore();
  const {
    progress,
    currentChunk,
    totalChunks,
    isRunning,
    issues,
    detail,
    setProgress,
    setRunning,
    setIssues,
  } = useTranslationStore();
  const navigate = useNavigate();

  const [error, setError] = useState<string | null>(null);
  const [savedPath, setSavedPath] = useState<string | null>(null);

  const unlistenRef = useRef<(() => void) | null>(null);

  // Listen for translation-progress events from Rust
  useEffect(() => {
    if (!isRunning) {
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }
      return;
    }
    (async () => {
      unlistenRef.current = await listen<TranslationProgress>("translation-progress", (event) => {
        const { current_entry_count, total_entry_count, detail } = event.payload;
        const pct = total_entry_count > 0 ? (current_entry_count / total_entry_count) * 100 : 0;
        useTranslationStore.getState().setProgress(pct, current_entry_count, total_entry_count);
        useTranslationStore.getState().setDetail(detail);
      });
    })();
    return () => {
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }
    };
  }, [isRunning]);

  const isReady = srtLoaded && active.has_key;

  const handleTranslate = async () => {
    if (!isReady) return;
    setRunning(true);
    setError(null);
    setSavedPath(null);

    try {
      const translationConfig: TranslationConfig = {
        max_chars_per_line: 24,
        max_lines_per_subtitle: 2,
        style: "neutral_subtitle",
        avoid_gendered_speech: true,
      };

      const result = await startTranslation(srtEntries, translationConfig);

      setIssues(result.issues);
      setProgress(100, result.entries.length, result.entries.length);

      const highIssues = result.issues.filter((i) => i.severity === "high");
      const hasHighIssues = highIssues.length > 0;

      if (hasHighIssues) {
        const issueTypes = [...new Set(highIssues.map((i) => i.issue_type))].join(", ");
        setError(
          `重大な課題が${highIssues.length}件見つかったため、画面反映と自動保存をスキップしました（内訳: ${issueTypes}）。課題を確認してください。`
        );
        return;
      }

      // Only update store entries when no high-severity issues exist
      const { setEntries } = useSrtStore.getState();
      setEntries(result.entries, useSrtStore.getState().fileName + " (翻訳済)");

      if (filePath) {
        const jpPath = filePath.replace(/_en\.srt$/i, "_jp.srt");
        try {
          await saveSrtFile(jpPath, result.entries);
          setSavedPath(jpPath);
        } catch (saveErr) {
          console.error("Auto-save failed:", saveErr);
        }
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  };

  const issueCount = issues.length;

  return (
    <div>
      <div className="card" style={{ maxWidth: 600 }}>
        <h2 className="card-title">翻訳</h2>

        {!srtLoaded ? (
          <p style={{ color: "var(--text-secondary)" }}>
            先にSRTタブでSRTファイルを読み込んでください。
          </p>
        ) : !active.has_key ? (
          <div>
            <p style={{ color: "var(--text-secondary)", marginBottom: 12 }}>
              LLMが未設定です。設定画面で環境変数名とAPIキーを保存してください。
            </p>
            {active.name && (
              <p style={{ color: "var(--warning)", fontSize: 12 }}>
                環境変数 <code>{active.name}</code> は選択されていますが値が未設定です。
              </p>
            )}
            <button className="btn btn-primary mt-8" onClick={() => navigate("/settings")}>
              設定を開く
            </button>
          </div>
        ) : (
          <>
            <div style={{ marginBottom: 16 }}>
              <div className="flex items-center justify-between mb-8">
                <span style={{ fontSize: 13, color: "var(--text-secondary)" }}>
                  {srtEntries.length}件の字幕が翻訳待ちです
                </span>
                <span style={{ fontSize: 13, color: "var(--text-secondary)" }}>
                  キャラクター: {characters.length} | 用語: {glossary.length}
                </span>
              </div>

              {isRunning && (
                <div style={{ marginBottom: 16 }}>
                  <div className="progress-bar mb-8">
                    <div
                      className="progress-bar-fill"
                      style={{ width: `${progress}%` }}
                    />
                  </div>
                  <p style={{ fontSize: 13, color: "var(--text-secondary)" }}>
                    {detail || (progress > 0 ? `翻訳中... ${Math.round(progress)}%` : "開始中...")}
                    {totalChunks > 0 && ` (チャンク ${currentChunk}/${totalChunks})`}
                  </p>
                </div>
              )}

              {!isRunning && progress === 100 && (
                <div style={{ marginBottom: 12 }}>
                  <p style={{ color: "var(--success)", fontSize: 14, marginBottom: 4 }}>
                    翻訳完了！ {issueCount > 0 ? `${issueCount}件の課題が見つかりました。` : "課題は検出されませんでした。"}
                  </p>
                  {savedPath && (
                    <p style={{ color: "var(--text-secondary)", fontSize: 12 }}>
                      保存先: {savedPath}
                    </p>
                  )}
                </div>
              )}
            </div>

            <div className="flex gap-8">
              <button
                className="btn btn-primary"
                onClick={handleTranslate}
                disabled={isRunning}
              >
                <Play size={16} />
                {isRunning ? "翻訳中..." : "翻訳開始"}
              </button>
            </div>
          </>
        )}

        {error && (
          <div
            style={{
              marginTop: 16,
              padding: 12,
              background: "rgba(196,43,28,0.08)",
              border: "1px solid var(--error)",
              borderRadius: 4,
              fontSize: 13,
              color: "var(--error)",
            }}
          >
            {error}
          </div>
        )}

        {issues.length > 0 && (
          <div style={{ marginTop: 16 }}>
            <h3 style={{ fontSize: 14, marginBottom: 8, color: "var(--text-primary)" }}>
              課題一覧 ({issues.length}件)
            </h3>
            <div style={{ maxHeight: 320, overflowY: "auto", fontSize: 12 }}>
              <table style={{ width: "100%", borderCollapse: "collapse" }}>
                <thead>
                  <tr style={{ background: "var(--bg-secondary)", position: "sticky", top: 0 }}>
                    <th style={thStyle}>#</th>
                    <th style={thStyle}>重大度</th>
                    <th style={thStyle}>種別</th>
                    <th style={thStyle}>内容</th>
                    <th style={thStyle}>翻訳文</th>
                  </tr>
                </thead>
                <tbody>
                  {issues.map((issue, i) => (
                    <tr key={i} style={{ borderBottom: "1px solid var(--border)" }}>
                      <td style={tdStyle}>{issue.index === 0 ? "—" : issue.index}</td>
                      <td style={{ ...tdStyle, color: issue.severity === "high" ? "var(--error)" : "var(--warning)" }}>
                        {issue.severity === "high" ? "高" : "中"}
                      </td>
                      <td style={{ ...tdStyle, fontFamily: "monospace", fontSize: 11 }}>
                        {issue.issue_type}
                      </td>
                      <td style={tdStyle}>{issue.message}</td>
                      <td style={{ ...tdStyle, maxWidth: 200, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                        {issue.translation}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

const thStyle: React.CSSProperties = {
  padding: "6px 8px",
  textAlign: "left",
  fontSize: 11,
  fontWeight: 600,
  color: "var(--text-secondary)",
  borderBottom: "1px solid var(--border)",
};

const tdStyle: React.CSSProperties = {
  padding: "4px 8px",
  verticalAlign: "top",
  color: "var(--text-primary)",
};
