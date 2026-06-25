import { useState, useEffect, useRef, useCallback } from "react";
import { Play } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { useSrtStore } from "../../stores/useSrtStore";
import { useDictionaryStore } from "../../stores/useDictionaryStore";
import { useLlmStore } from "../../stores/useLlmStore";
import { useTranslationStore } from "../../stores/useTranslationStore";
import { startTranslation, saveSrtFile, getTranslationReadinessForSrt } from "../../lib/tauri";
import type { TranslationConfig, TranslationModelTier, TranslationProgress } from "../../types";
import type { ValidationIssue } from "../../types";
import { useNavigate } from "react-router-dom";

interface FileReadiness {
  srtPath: string;
  fileName: string;
  entryCount: number;
  canTranslate: boolean;
  hasAnalysis: boolean;
  hasTranslationPrompt: boolean;
  hasJpSrt: boolean;
}

interface EpTranslationState {
  running: boolean;
  progress: number;
  currentChunk: number;
  totalChunks: number;
  detail: string;
  issues: ValidationIssue[];
  savedPath: string | null;
  error: string | null;
}

export default function TranslatePanel() {
  const { files, projectBaseDir } = useSrtStore();
  const { characters, glossary } = useDictionaryStore();
  const { active } = useLlmStore();
  const { setRunning, setProgress, setDetail } = useTranslationStore();
  const navigate = useNavigate();

  const [modelTier, setModelTier] = useState<TranslationModelTier>("provider_default");
  const [fileReadinessMap, setFileReadinessMap] = useState<Map<string, FileReadiness>>(new Map());
  const [epStates, setEpStates] = useState<Map<string, EpTranslationState>>(new Map());
  const [batchCurrent, setBatchCurrent] = useState(0);
  const [batchTotal, setBatchTotal] = useState(0);
  const [batchRunning, setBatchRunning] = useState(false);
  const [batchError, setBatchError] = useState<string | null>(null);

  const unlistenRef = useRef<(() => void) | null>(null);
  const currentEpRef = useRef<string | null>(null);

  const srtLoaded = files.some(f => f.status === "loaded");

  // Listen for translation-progress events from Rust
  useEffect(() => {
    if (!batchRunning && !Array.from(epStates.values()).some(s => s.running)) {
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
        setProgress(pct, current_entry_count, total_entry_count);
        setDetail(detail);
        const ep = currentEpRef.current;
        if (ep) {
          setEpStates(prev => {
            const next = new Map(prev);
            const s = next.get(ep);
            if (s) {
              next.set(ep, {
                ...s,
                progress: pct,
                currentChunk: current_entry_count,
                totalChunks: total_entry_count,
                detail,
              });
            }
            return next;
          });
        }
      });
    })();
    return () => {
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }
    };
  }, [batchRunning, epStates]);

  // Scan translation readiness for all loaded files
  useEffect(() => {
    const scan = async () => {
      const loadedFiles = files.filter(f => f.status === "loaded");
      if (loadedFiles.length === 0) {
        setFileReadinessMap(new Map());
        return;
      }
      try {
        const results = await Promise.all(
          loadedFiles.map(f => getTranslationReadinessForSrt(f.path, projectBaseDir ?? undefined))
        );
        const map = new Map<string, FileReadiness>();
        for (let i = 0; i < loadedFiles.length; i++) {
          const r = results[i];
          map.set(r.srt_path, {
            srtPath: r.srt_path,
            fileName: loadedFiles[i].name,
            entryCount: loadedFiles[i].entries.length,
            canTranslate: r.can_translate,
            hasAnalysis: r.has_analysis,
            hasTranslationPrompt: r.has_translation_prompt,
            hasJpSrt: r.has_jp_srt,
          });
        }
        setFileReadinessMap(map);
      } catch {
        // readiness scan is best-effort
      }
    };
    scan();
  }, [files, projectBaseDir]);

  const refreshReadiness = useCallback(async (srtPath: string, fileName: string) => {
    const updated = await getTranslationReadinessForSrt(srtPath, projectBaseDir ?? undefined);
    setFileReadinessMap(prev => {
      const next = new Map(prev);
      next.set(srtPath, {
        srtPath,
        fileName,
        entryCount: next.get(srtPath)?.entryCount ?? 0,
        canTranslate: updated.can_translate,
        hasAnalysis: updated.has_analysis,
        hasTranslationPrompt: updated.has_translation_prompt,
        hasJpSrt: updated.has_jp_srt,
      });
      return next;
    });
  }, [projectBaseDir]);

  const translateOne = async (srtPath: string) => {
    const file = useSrtStore.getState().files.find(f => f.path === srtPath);
    if (!file) throw new Error("File not found");

    const translationConfig: TranslationConfig = {
      max_chars_per_line: 24,
      max_lines_per_subtitle: 2,
      style: "neutral_subtitle",
      avoid_gendered_speech: true,
      model_tier: modelTier,
    };

    const result = await startTranslation(file.entries, translationConfig, srtPath);

    const highIssues = result.issues.filter(i => i.severity === "high");
    if (highIssues.length > 0) {
      const issueTypes = [...new Set(highIssues.map(i => i.issue_type))].join(", ");
      const details = highIssues.slice(0, 5).map((i, n) => {
        const pos = i.start_time
          ? `[${i.start_time}${i.end_time ? ` --> ${i.end_time}` : ""}]`
          : `[#${n + 1}]`;
        const frag = i.detected_fragment ? ` fragment="${i.detected_fragment}"` : "";
        const text = i.translation || "";
        const preview = text.length > 0 ? ` 訳文="${text.length > 80 ? text.slice(0, 80) + "..." : text}"` : "";
        return `${pos} ${i.message}${frag}${preview}`;
      }).join("\n");
      throw new Error(
        `重大な課題が${highIssues.length}件見つかりました（内訳: ${issueTypes}）。\n${details}`
      );
    }

    useSrtStore.getState().setFileEntries(srtPath, result.entries);

    const jpPath = srtPath.replace(/_en\.srt$/i, "_jp.srt");
    await saveSrtFile(jpPath, result.entries);

    await refreshReadiness(srtPath, file.name);
    return { jpPath, issues: result.issues };
  };

  const handleTranslateEp = async (srtPath: string) => {
    currentEpRef.current = srtPath;
    setEpStates(prev => {
      const next = new Map(prev);
      next.set(srtPath, { running: true, progress: 0, currentChunk: 0, totalChunks: 0, detail: "", issues: [], savedPath: null, error: null });
      return next;
    });

    try {
      const { jpPath, issues } = await translateOne(srtPath);
      setEpStates(prev => {
        const next = new Map(prev);
        next.set(srtPath, { running: false, progress: 100, currentChunk: 0, totalChunks: 0, detail: "", issues, savedPath: jpPath, error: null });
        return next;
      });
    } catch (e) {
      setEpStates(prev => {
        const next = new Map(prev);
        next.set(srtPath, { running: false, progress: 0, currentChunk: 0, totalChunks: 0, detail: "", issues: [], savedPath: null, error: String(e) });
        return next;
      });
    } finally {
      currentEpRef.current = null;
    }
  };

  const handleBatchTranslate = async (paths: string[]) => {
    setBatchRunning(true);
    setBatchTotal(paths.length);
    setBatchError(null);
    setRunning(true);
    setProgress(0, 0, 0);

    for (let i = 0; i < paths.length; i++) {
      setBatchCurrent(i + 1);
      currentEpRef.current = paths[i];
      setEpStates(prev => {
        const next = new Map(prev);
        next.set(paths[i], { running: true, progress: 0, currentChunk: 0, totalChunks: 0, detail: "", issues: [], savedPath: null, error: null });
        return next;
      });

      try {
        const { jpPath, issues } = await translateOne(paths[i]);
        setEpStates(prev => {
          const next = new Map(prev);
          next.set(paths[i], { running: false, progress: 100, currentChunk: 0, totalChunks: 0, detail: "", issues, savedPath: jpPath, error: null });
          return next;
        });
      } catch (e) {
        const errMsg = `Ep ${i + 1}/${paths.length} でエラー: ${String(e)}`;
        setBatchError(errMsg);
        setEpStates(prev => {
          const next = new Map(prev);
          next.set(paths[i], { running: false, progress: 0, currentChunk: 0, totalChunks: 0, detail: "", issues: [], savedPath: null, error: String(e) });
          return next;
        });
        break;
      }
    }
    setBatchRunning(false);
    setBatchCurrent(0);
    setBatchTotal(0);
    setRunning(false);
    currentEpRef.current = null;
  };

  const translatablePaths = Array.from(fileReadinessMap.values())
    .filter(r => r.canTranslate)
    .map(r => r.srtPath);

  const untranslatedPaths = Array.from(fileReadinessMap.values())
    .filter(r => r.canTranslate && !r.hasJpSrt)
    .map(r => r.srtPath);

  const sortedReadiness = Array.from(fileReadinessMap.values());

  // Compute batch progress with actual per-episode progress
  let batchProgressPct = 0;
  if (batchTotal > 0 && currentEpRef.current) {
    const ep = epStates.get(currentEpRef.current);
    const epPct = ep?.progress ?? 0;
    batchProgressPct = ((batchCurrent - 1) + epPct / 100) / batchTotal * 100;
  }

  return (
    <div>
      <div className="card" style={{ maxWidth: 700 }}>
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
            {/* Header */}
            <div style={{ marginBottom: 16 }}>
              <div className="flex items-center justify-between mb-8">
                <span style={{ fontSize: 13, color: "var(--text-secondary)" }}>
                  {files.filter(f => f.status === "loaded").length}ファイル読み込み済み
                </span>
                <span style={{ fontSize: 13, color: "var(--text-secondary)" }}>
                  キャラクター: {characters.length} | 用語: {glossary.length}
                </span>
              </div>

              {/* Model selector */}
              {!batchRunning && (
                <label
                  style={{
                    display: "grid",
                    gridTemplateColumns: "120px minmax(0, 1fr)",
                    gap: 12,
                    alignItems: "center",
                    marginBottom: 14,
                  }}
                >
                  <span style={{ fontSize: 13, color: "var(--text-secondary)" }}>
                    翻訳モデル
                  </span>
                  <select
                    className="form-input"
                    value={modelTier}
                    onChange={(e) => setModelTier(e.target.value as TranslationModelTier)}
                  >
                    <option value="provider_default">設定画面の既定</option>
                    <option value="pro">Pro相当</option>
                    <option value="flash">Flash相当</option>
                  </select>
                </label>
              )}

              {/* Batch translate buttons */}
              {translatablePaths.length > 0 && (
                <div style={{ display: "flex", gap: 8, marginBottom: 14 }}>
                  <button
                    className="btn btn-primary"
                    onClick={() => handleBatchTranslate(translatablePaths)}
                    disabled={batchRunning || Array.from(epStates.values()).some(s => s.running)}
                  >
                    <Play size={16} />
                    翻訳可能な{translatablePaths.length}話を一括翻訳
                  </button>
                  {untranslatedPaths.length > 0 && untranslatedPaths.length !== translatablePaths.length && (
                    <button
                      className="btn btn-secondary"
                      onClick={() => handleBatchTranslate(untranslatedPaths)}
                      disabled={batchRunning || Array.from(epStates.values()).some(s => s.running)}
                    >
                      <Play size={16} />
                      未翻訳の{untranslatedPaths.length}話を一括翻訳
                    </button>
                  )}
                </div>
              )}

              {/* Batch progress */}
              {batchTotal > 0 && (
                <div style={{ marginBottom: 16 }}>
                  <div className="progress-bar mb-8">
                    <div
                      className="progress-bar-fill"
                      style={{ width: `${batchProgressPct}%` }}
                    />
                  </div>
                  <p style={{ fontSize: 13, color: "var(--text-secondary)" }}>
                    一括翻訳中: Ep {batchCurrent}/{batchTotal}
                  </p>
                </div>
              )}

              {batchError && (
                <div
                  style={{
                    marginBottom: 14,
                    padding: 10,
                    background: "rgba(196,43,28,0.08)",
                    border: "1px solid var(--error)",
                    borderRadius: 4,
                    fontSize: 13,
                    color: "var(--error)",
                  }}
                >
                  {batchError}
                </div>
              )}
            </div>

            {/* Episode list */}
            <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
              {sortedReadiness.length === 0 && (
                <p style={{ fontSize: 13, color: "var(--text-secondary)" }}>確認中...</p>
              )}
              {sortedReadiness.map(r => {
                const epState = epStates.get(r.srtPath);
                const isTranslating = epState?.running ?? false;
                const epError = epState?.error ?? null;
                const epIssues = epState?.issues ?? [];
                const epSaved = epState?.savedPath ?? null;

                return (
                  <div
                    key={r.srtPath}
                    style={{
                      border: `1px solid ${isTranslating ? "var(--accent)" : "var(--border)"}`,
                      borderRadius: 6,
                      padding: "10px 14px",
                      background: isTranslating ? "rgba(59,130,246,0.04)" : "var(--bg-primary)",
                    }}
                  >
                    <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
                      <div style={{ minWidth: 0, flex: 1 }}>
                        <div style={{ fontSize: 13, fontWeight: 500, color: "var(--text-primary)", marginBottom: 2, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                          {r.fileName}
                        </div>
                        <div style={{ fontSize: 11, color: "var(--text-secondary)" }}>
                          {r.entryCount}件の字幕
                          {" · "}
                          {r.hasJpSrt
                            ? <span style={{ color: "var(--success)" }}>翻訳済み・再翻訳可能</span>
                            : r.canTranslate
                              ? <span style={{ color: "var(--accent)" }}>翻訳可能</span>
                              : !r.hasAnalysis
                                ? <span style={{ color: "var(--warning)" }}>analysis.json なし</span>
                                : <span style={{ color: "var(--warning)" }}>翻訳プロンプト未生成</span>
                          }
                        </div>
                      </div>
                      {(r.canTranslate || r.hasJpSrt) && (
                        <button
                          className="btn btn-sm btn-primary"
                          style={{ flexShrink: 0, fontSize: 12 }}
                          disabled={isTranslating || batchRunning || (Array.from(epStates.values()).some(s => s.running) && !isTranslating)}
                          onClick={() => handleTranslateEp(r.srtPath)}
                        >
                          <Play size={12} />
                          {isTranslating ? "翻訳中..." : "翻訳"}
                        </button>
                      )}
                    </div>

                    {/* Inline progress for this episode */}
                    {isTranslating && (
                      <div style={{ marginTop: 8 }}>
                        <div className="progress-bar" style={{ marginBottom: 4 }}>
                          <div
                            className="progress-bar-fill"
                            style={{ width: `${epState?.progress ?? 0}%` }}
                          />
                        </div>
                        <p style={{ fontSize: 11, color: "var(--text-secondary)" }}>
                          翻訳中...{epState?.detail ? ` (${epState.detail})` : ""}
                        </p>
                      </div>
                    )}

                    {/* Completed: saved path + issues */}
                    {epSaved && !isTranslating && (
                      <div style={{ marginTop: 6, fontSize: 11, color: "var(--text-secondary)" }}>
                        <span style={{ color: "var(--success)" }}>保存: {epSaved}</span>
                        {epIssues.length > 0 && (
                          <span style={{ marginLeft: 8, color: "var(--warning)" }}>
                            {epIssues.length}件の課題
                          </span>
                        )}
                        {epIssues.length === 0 && (
                          <span style={{ marginLeft: 8, color: "var(--success)" }}>
                            課題なし
                          </span>
                        )}
                      </div>
                    )}

                    {/* Error */}
                    {epError && !isTranslating && (
                      <div style={{ marginTop: 6, fontSize: 11, color: "var(--error)" }}>
                        {epError}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
