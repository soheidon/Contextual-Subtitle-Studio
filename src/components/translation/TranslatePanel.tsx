import { useState } from "react";
import { Play, Download } from "lucide-react";
import { useSrtStore } from "../../stores/useSrtStore";
import { useDictionaryStore } from "../../stores/useDictionaryStore";
import { useLlmStore } from "../../stores/useLlmStore";
import { useTranslationStore } from "../../stores/useTranslationStore";
import { startTranslation, saveSrtFile } from "../../lib/tauri";
import type { TranslationConfig } from "../../types";
import { useNavigate } from "react-router-dom";

export default function TranslatePanel() {
  const { entries: srtEntries, isLoaded: srtLoaded } = useSrtStore();
  const { characters, glossary } = useDictionaryStore();
  const { active } = useLlmStore();
  const {
    progress,
    currentChunk,
    totalChunks,
    isRunning,
    issues,
    setProgress,
    setRunning,
    setIssues,
  } = useTranslationStore();
  const navigate = useNavigate();

  const [error, setError] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);

  const isReady = srtLoaded && active.has_key;

  const handleTranslate = async () => {
    if (!isReady) return;
    setRunning(true);
    setError(null);

    try {
      const translationConfig: TranslationConfig = {
        max_chars_per_line: 24,
        max_lines_per_subtitle: 2,
        style: "neutral_subtitle",
        avoid_gendered_speech: true,
      };

      const result = await startTranslation(translationConfig);

      const { setEntries } = useSrtStore.getState();
      setEntries(result.entries, useSrtStore.getState().fileName + " (翻訳済)");
      setIssues(result.issues);
      setProgress(100, result.entries.length, result.entries.length);
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  };

  const handleExport = async () => {
    setExporting(true);
    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({
        filters: [{ name: "SRTファイル", extensions: ["srt"] }],
      });
      if (path) {
        await saveSrtFile(path, srtEntries);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setExporting(false);
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
                    翻訳中... {progress > 0 ? `${Math.round(progress)}%` : "開始中..."}
                    {totalChunks > 0 && ` (チャンク ${currentChunk}/${totalChunks})`}
                  </p>
                </div>
              )}

              {!isRunning && progress === 100 && (
                <p style={{ color: "var(--success)", fontSize: 14, marginBottom: 12 }}>
                  翻訳完了！ {issueCount > 0 ? `${issueCount}件の課題が見つかりました。` : "課題は検出されませんでした。"}
                </p>
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
              {progress === 100 && (
                <button className="btn btn-secondary" onClick={handleExport} disabled={exporting}>
                  <Download size={16} />
                  {exporting ? "出力中..." : "SRT出力"}
                </button>
              )}
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
      </div>
    </div>
  );
}
