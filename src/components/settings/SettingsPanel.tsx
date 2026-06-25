import { Fragment, useEffect, useState } from "react";
import { useLlmStore } from "../../stores/useLlmStore";
import { useAppLogStore } from "../../stores/useAppLogStore";
import {
  setEnvVar,
  setActiveEnvVar,
  checkEnvVarKeyExists,
  checkActiveConnection,
  listProviderPresets,
  getProviderSettings,
  saveProviderSettings,
  getServiceSettings,
  saveServiceSettings,
  getLlmTaskModelSettings,
  saveLlmTaskModelSettings,
  testOpenaiAiConfirm,
} from "../../lib/tauri";
import type { LlmTaskModelSettings, ModelTier, ProviderPreset } from "../../types";
import ServiceSettingsPanel from "./ServiceSettings";

// ---- Constants ----

const THINKING_OPTIONS = [
  { value: "disabled", label: "無効" },
  { value: "enabled", label: "有効" },
  { value: "auto", label: "自動" },
];

type ConnState = "unset" | "configured" | "ok" | "fail";
type RowMessage = { type: "ok" | "err"; text: string } | null;

interface ProviderDraft {
  envVarName: string;
  apiKey: string;
  baseUrl: string;
  proModel: string;
  flashModel: string;
  defaultTier: ModelTier;
  supportsThinking: boolean;
  thinking: string;
  connState: ConnState;
  testing: boolean;
  message: RowMessage;
}

const THINKING_DESC =
  "このProviderはThinking設定に対応しています。高精度な推論が必要な処理で有効にできます。";

const DEFAULT_TASK_MODEL_SETTINGS: LlmTaskModelSettings = {
  synopsis_generation: "pro",
  scene_detection: "pro",
  scene_context_analysis: "pro",
  proper_noun_confirmation: "pro",
  subtitle_translation: "pro",
  lightweight_cleanup: "flash",
  kanji_correction: "pro",
  zh_context_disambiguation: "flash",
};

const TASK_MODEL_ROWS: Array<{
  key: keyof LlmTaskModelSettings;
  label: string;
}> = [
  { key: "synopsis_generation", label: "あらすじ生成" },
  { key: "scene_detection", label: "場面検出" },
  { key: "scene_context_analysis", label: "場面文脈分析" },
  { key: "proper_noun_confirmation", label: "固有名詞確認" },
  { key: "subtitle_translation", label: "通常翻訳" },
  { key: "lightweight_cleanup", label: "軽い補正・検証" },
  { key: "kanji_correction", label: "漢字補正" },
  { key: "zh_context_disambiguation", label: "中文候補の曖昧性判定" },
];

// ---- Component ----

const SETTINGS_TABS = [
  { id: "api-keys" as const, label: "APIキー" },
  { id: "srt" as const, label: "SRT" },
];

type TabId = (typeof SETTINGS_TABS)[number]["id"];

export default function SettingsPanel() {
  const { active, refresh } = useLlmStore();
  const addLog = useAppLogStore((s) => s.addLog);
  const [activeTab, setActiveTab] = useState<TabId>("api-keys");

  const [presets, setPresets] = useState<[string, ProviderPreset][]>([]);
  const [hasKey, setHasKey] = useState<Record<string, boolean>>({});
  const [expanded, setExpanded] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<Record<string, ProviderDraft>>({});
  const [taskModels, setTaskModels] = useState<LlmTaskModelSettings>(DEFAULT_TASK_MODEL_SETTINGS);

  // Load presets on mount, then load per-provider settings
  useEffect(() => {
    let cancelled = false;
    (async () => {
      const p = await listProviderPresets();
      if (cancelled) return;
      setPresets(p);

      try {
        const savedTaskModels = await getLlmTaskModelSettings();
        if (!cancelled) setTaskModels(savedTaskModels);
      } catch {
        if (!cancelled) setTaskModels(DEFAULT_TASK_MODEL_SETTINGS);
      }

      // Load key existence for each preset
      const results = await Promise.all(
        p.map(async ([prefix]) => {
          const env = `${prefix}_API_KEY`;
          const exists = await checkEnvVarKeyExists(env);
          return [env, exists] as const;
        }),
      );
      if (cancelled) return;
      const map: Record<string, boolean> = {};
      for (const [env, exists] of results) map[env] = exists;
      setHasKey(map);

      // Load saved per-provider settings for each
      const draftsMap: Record<string, ProviderDraft> = {};
      for (const [prefix] of p) {
        try {
          const ps = await getProviderSettings(prefix);
          draftsMap[prefix] = {
            envVarName: `${prefix}_API_KEY`,
            apiKey: "",
            baseUrl: ps.base_url,
            proModel: ps.pro_model,
            flashModel: ps.flash_model,
            defaultTier: ps.default_tier,
            supportsThinking: ps.supports_thinking,
            thinking: ps.thinking,
            connState: map[`${prefix}_API_KEY`] ? "configured" : "unset",
            testing: false,
            message: null,
          };
        } catch {
          draftsMap[prefix] = emptyDraft(prefix, map[`${prefix}_API_KEY`] ?? false);
        }
      }
      setDrafts(draftsMap);
      refresh();
    })();
    return () => {
      cancelled = true;
    };
  }, [refresh]);

  // ---- helpers ----

  const getDraft = (prefix: string): ProviderDraft =>
    drafts[prefix] || emptyDraft(prefix, !!hasKey[`${prefix}_API_KEY`]);

  const updateDraft = (prefix: string, patch: Partial<ProviderDraft>) => {
    setDrafts((d) => ({
      ...d,
      [prefix]: { ...getDraft(prefix), ...patch },
    }));
  };

  const setMsg = (prefix: string, type: "ok" | "err", text: string) => {
    updateDraft(prefix, { message: { type, text } });
    setTimeout(() => {
      setDrafts((d) => {
        const cur = d[prefix];
        if (cur?.message?.text === text) {
          return { ...d, [prefix]: { ...cur, message: null } };
        }
        return d;
      });
    }, 4000);
  };

  // ---- actions ----

  const handleSelect = async (prefix: string) => {
    const envName = `${prefix}_API_KEY`;
    try {
      await setActiveEnvVar(envName);
      await refresh();
    } catch {
      // ignore
    }
  };

  const toggleRow = (prefix: string) => {
    setExpanded((cur) => (cur === prefix ? null : prefix));
  };

  const handleSaveKey = async (prefix: string) => {
    const d = getDraft(prefix);
    const envName = d.envVarName.trim().toUpperCase();
    const value = d.apiKey;
    if (!envName) {
      setMsg(prefix, "err", "環境変数名を入力してください。");
      return;
    }
    if (!value) {
      setMsg(prefix, "err", "APIキーを入力してください。");
      return;
    }
    try {
      await setEnvVar(envName, value);
      addLog("success", providerLabelStatic(prefix), "APIキーを保存しました");
      setMsg(prefix, "ok", `${envName} を保存しました。`);
      setDrafts((d) => ({
        ...d,
        [prefix]: { ...getDraft(prefix), apiKey: "", connState: "configured" },
      }));
      setHasKey((h) => ({ ...h, [envName]: true }));
    } catch (e) {
      addLog("error", providerLabelStatic(prefix), `保存失敗: ${e}`);
      setMsg(prefix, "err", `エラー: ${e}`);
    }
  };

  const handleTestConnection = async (prefix: string, modelTier?: ModelTier) => {
    const d = getDraft(prefix);
    const tag = providerLabelStatic(prefix);
    updateDraft(prefix, { testing: true, message: null });

    // Persist settings first so overrides apply
    try {
      await saveProviderSettings(prefix, {
        base_url: d.baseUrl || null,
        pro_model: d.proModel || null,
        flash_model: d.flashModel || null,
        default_tier: d.defaultTier || null,
        supports_thinking: d.supportsThinking,
        thinking: d.thinking || null,
      });
    } catch {
      // save is best-effort for the test
    }

    const tier = modelTier ?? d.defaultTier;
    const model = tier === "pro" ? d.proModel : d.flashModel;
    const extra = d.supportsThinking ? `, thinking=${d.thinking}` : "";
    addLog("info", tag, `接続テスト開始: tier=${tier}, model=${model}${extra}`);

    try {
      const has = await checkEnvVarKeyExists(d.envVarName);
      if (!has) {
        updateDraft(prefix, { testing: false, connState: "unset" });
        setMsg(prefix, "err", "APIキーが未設定です。");
        addLog("warning", tag, "接続テスト: APIキーが未設定です");
        return;
      }
      await checkActiveConnection(d.envVarName, modelTier);
      updateDraft(prefix, { testing: false, connState: "ok" });
      setMsg(prefix, "ok", "接続OK");
      addLog("success", tag, "接続OK");
    } catch (e) {
      updateDraft(prefix, { testing: false, connState: "fail" });
      setMsg(prefix, "err", `接続失敗: ${e}`);
      addLog("error", tag, `接続失敗: ${e}`);
    }
  };

  const handleTestAiConfirm = async (prefix: string) => {
    const d = getDraft(prefix);
    const tag = providerLabelStatic(prefix);
    updateDraft(prefix, { testing: true, message: null });

    // Persist settings first so overrides apply
    try {
      await saveProviderSettings(prefix, {
        base_url: d.baseUrl || null,
        pro_model: d.proModel || null,
        flash_model: d.flashModel || null,
        default_tier: d.defaultTier || null,
        supports_thinking: d.supportsThinking,
        thinking: d.thinking || null,
      });
    } catch {
      // save is best-effort for the test
    }

    addLog("info", tag, `AI確認テスト開始: model=${d.proModel}`);

    try {
      const has = await checkEnvVarKeyExists(d.envVarName);
      if (!has) {
        updateDraft(prefix, { testing: false, connState: "unset" });
        setMsg(prefix, "err", "APIキーが未設定です。");
        addLog("warning", tag, "AI確認テスト: APIキーが未設定です");
        return;
      }
      await testOpenaiAiConfirm();
      updateDraft(prefix, { testing: false, connState: "ok" });
      setMsg(prefix, "ok", "AI確認テストOK (Responses API)");
      addLog("success", tag, "AI確認テストOK (Responses API)");
    } catch (e) {
      updateDraft(prefix, { testing: false, connState: "fail" });
      setMsg(prefix, "err", `AI確認テスト失敗: ${e}`);
      addLog("error", tag, `AI確認テスト失敗: ${e}`);
    }
  };

  const handleResetDefaults = (prefix: string) => {
    // Reset to preset defaults — save empty overrides so defaults apply
    saveProviderSettings(prefix, {
      base_url: null,
      model: null,
      pro_model: null,
      flash_model: null,
      default_tier: null,
      supports_thinking: null,
      thinking: null,
    }).catch(() => {});
    // Show preset defaults immediately in the UI
    const preset = presets.find(([p]) => p === prefix)?.[1];
    const defaults = preset
      ? {
          baseUrl: preset.base_url,
          proModel: preset.pro_model,
          flashModel: preset.flash_model,
          defaultTier: preset.default_tier,
          supportsThinking: preset.supports_thinking,
          thinking: preset.default_thinking,
        }
      : {
          baseUrl: "",
          proModel: "",
          flashModel: "",
          defaultTier: "pro" as ModelTier,
          supportsThinking: false,
          thinking: "disabled",
        };
    updateDraft(prefix, {
      envVarName: `${prefix}_API_KEY`,
      ...defaults,
    });
    addLog("info", providerLabelStatic(prefix), "デフォルト設定に戻しました");
  };

  const persistTaskModels = async (models: LlmTaskModelSettings) => {
    try {
      await saveLlmTaskModelSettings(models);
    } catch (e) {
      addLog("error", "LLM", `作業別モデル設定の保存失敗: ${e}`);
    }
  };

  const updateTaskModel = (key: keyof LlmTaskModelSettings, value: ModelTier) => {
    setTaskModels((current) => {
      const next = { ...current, [key]: value };
      persistTaskModels(next);
      return next;
    });
  };

  const resetTaskModels = () => {
    setTaskModels(DEFAULT_TASK_MODEL_SETTINGS);
    persistTaskModels(DEFAULT_TASK_MODEL_SETTINGS);
  };

  return (
    <div style={{ maxWidth: 900 }}>
      {/* ---- Tab bar ---- */}
      <div style={{ display: "flex", gap: 4, marginBottom: 16, borderBottom: "1px solid var(--border)" }}>
        {SETTINGS_TABS.map((tab) => (
          <button
            key={tab.id}
            className={`btn ${activeTab === tab.id ? "btn-primary" : "btn-secondary"}`}
            onClick={() => setActiveTab(tab.id)}
            style={{
              fontSize: 13,
              padding: "6px 16px",
              borderRadius: "6px 6px 0 0",
              borderBottom: activeTab === tab.id ? "2px solid var(--accent)" : undefined,
            }}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {/* ---- API keys tab ---- */}
      {activeTab === "api-keys" && <>
      {/* ---- LLM providers ---- */}
      <div className="card" style={{ padding: 0 }}>
        <h2 className="card-title" style={{ padding: "18px 18px 12px 18px" }}>
          APIキー
        </h2>
        <div className="table-container" style={{ border: "none", borderRadius: 0 }}>
          <table>
            <thead>
              <tr>
                <th style={{ width: 32 }}></th>
                <th style={{ width: "22%" }}>Provider</th>
                <th style={{ width: "24%" }}>Env Var</th>
                <th style={{ width: "18%" }}>Status</th>
                <th style={{ width: "18%", textAlign: "right" }}>Action</th>
              </tr>
            </thead>
            <tbody>
              {presets.map(([prefix]) => {
                const envName = `${prefix}_API_KEY`;
                const isOpen = expanded === prefix;
                const configured = !!hasKey[envName];
                const isActive = active.name === envName;
                const d = getDraft(prefix);
                const msg = d.message;

                return (
                  <Fragment key={prefix}>
                    {/* ---- Header row ---- */}
                    <tr style={isActive ? { backgroundColor: "var(--bg-elevated)" } : undefined}>
                      <td style={{ textAlign: "center" }}>
                        <input
                          type="radio"
                          name="active-provider"
                          checked={isActive}
                          onChange={() => handleSelect(prefix)}
                          style={{ cursor: "pointer" }}
                        />
                      </td>
                      <td>
                        <strong>{providerLabelStatic(prefix)}</strong>
                        {isActive && (
                          <span
                            style={{
                              fontSize: 10,
                              color: "var(--accent)",
                              marginLeft: 6,
                              fontWeight: 600,
                            }}
                          >
                            使用中
                          </span>
                        )}
                      </td>
                      <td
                        style={{
                          fontFamily: "monospace",
                          fontSize: 12,
                          color: "var(--text-secondary)",
                        }}
                      >
                        {envName}
                      </td>
                      <td>{connBadge(d.connState, configured)}</td>
                      <td style={{ textAlign: "right" }}>
                        <button
                          className="btn btn-secondary"
                          onClick={() => toggleRow(prefix)}
                        >
                          {isOpen ? "閉じる" : "編集"}
                        </button>
                      </td>
                    </tr>

                    {/* ---- Expanded detail row ---- */}
                    {isOpen && (
                      <tr>
                        <td
                          colSpan={5}
                          style={{
                            backgroundColor: "var(--bg-elevated)",
                            padding: "14px 18px",
                          }}
                        >
                          {/* Env var name */}
                          <FieldGrid>
                            <FieldLabel>環境変数名</FieldLabel>
                            <input
                              className="form-input"
                              value={d.envVarName}
                              onChange={(e) => updateDraft(prefix, { envVarName: e.target.value })}
                              style={{ fontFamily: "monospace", fontSize: 13, padding: "8px 10px", lineHeight: 1.4, overflow: "visible" }}
                            />
                            <div />
                          </FieldGrid>

                          {/* API key + save */}
                          <FieldGrid>
                            <FieldLabel>APIキー</FieldLabel>
                            <input
                              className="form-input"
                              type="password"
                              value={d.apiKey}
                              onChange={(e) => updateDraft(prefix, { apiKey: e.target.value })}
                              placeholder={prefix === "GEMINI" ? "AIza..." : "sk-..."}
                            />
                            <button
                              className="btn btn-primary"
                              onClick={() => handleSaveKey(prefix)}
                            >
                              環境変数に保存
                            </button>
                          </FieldGrid>

                          {/* Base URL */}
                          <FieldGrid2>
                            <FieldLabel>Base URL</FieldLabel>
                            <input
                              className="form-input"
                              value={d.baseUrl}
                              onChange={(e) => updateDraft(prefix, { baseUrl: e.target.value })}
                              style={{ fontFamily: "monospace", fontSize: 12 }}
                            />
                          </FieldGrid2>

                          {/* Model tiers */}
                          <FieldGrid2>
                            <FieldLabel>Pro相当モデル</FieldLabel>
                            <input
                              className="form-input"
                              value={d.proModel}
                              onChange={(e) => updateDraft(prefix, { proModel: e.target.value })}
                              placeholder={presets.find(([p]) => p === prefix)?.[1].pro_model || ""}
                              style={{ fontFamily: "monospace", fontSize: 12 }}
                            />
                          </FieldGrid2>

                          <FieldGrid2>
                            <FieldLabel>Flash相当モデル</FieldLabel>
                            <input
                              className="form-input"
                              value={d.flashModel}
                              onChange={(e) => updateDraft(prefix, { flashModel: e.target.value })}
                              placeholder={presets.find(([p]) => p === prefix)?.[1].flash_model || ""}
                              style={{ fontFamily: "monospace", fontSize: 12 }}
                            />
                          </FieldGrid2>

                          <FieldGrid2>
                            <FieldLabel>デフォルト使用</FieldLabel>
                            <select
                              className="form-input"
                              value={d.defaultTier}
                              onChange={(e) => updateDraft(prefix, { defaultTier: e.target.value as ModelTier })}
                            >
                              <option value="pro">Pro相当</option>
                              <option value="flash">Flash相当</option>
                            </select>
                          </FieldGrid2>

                          {/* Thinking */}
                          {d.supportsThinking && (
                            <>
                              <FieldGrid2>
                                <FieldLabel>Thinking</FieldLabel>
                                <select
                                  className="form-input"
                                  value={d.thinking}
                                  onChange={(e) => updateDraft(prefix, { thinking: e.target.value })}
                                >
                                  {THINKING_OPTIONS.map((o) => (
                                    <option key={o.value} value={o.value}>{o.label}</option>
                                  ))}
                                </select>
                              </FieldGrid2>
                              <p
                                style={{
                                  fontSize: 11,
                                  color: "var(--text-muted)",
                                  margin: "-8px 0 14px 120px",
                                  lineHeight: 1.4,
                                }}
                              >
                                {THINKING_DESC}
                              </p>
                            </>
                          )}

                          {/* Action buttons */}
                          <div style={{ display: "flex", gap: 10, alignItems: "center", marginTop: 16 }}>
                            {prefix === "OPENAI" ? (
                              <>
                                <button
                                  className="btn btn-primary"
                                  onClick={() => handleTestConnection(prefix, "pro")}
                                  disabled={d.testing}
                                >
                                  {d.testing ? "テスト中..." : "Pro接続テスト"}
                                </button>
                                <button
                                  className="btn btn-primary"
                                  onClick={() => handleTestConnection(prefix, "flash")}
                                  disabled={d.testing}
                                >
                                  {d.testing ? "テスト中..." : "Flash接続テスト"}
                                </button>
                                <button
                                  className="btn btn-primary"
                                  onClick={() => handleTestAiConfirm(prefix)}
                                  disabled={d.testing}
                                >
                                  {d.testing ? "テスト中..." : "AI確認テスト"}
                                </button>
                                <button
                                  className="btn btn-secondary"
                                  onClick={() => handleResetDefaults(prefix)}
                                >
                                  標準設定に戻す
                                </button>
                              </>
                            ) : (
                              <>
                                <button
                                  className="btn btn-primary"
                                  onClick={() => handleTestConnection(prefix, "pro")}
                                  disabled={d.testing}
                                >
                                  {d.testing ? "テスト中..." : "Pro接続テスト"}
                                </button>
                                <button
                                  className="btn btn-primary"
                                  onClick={() => handleTestConnection(prefix, "flash")}
                                  disabled={d.testing}
                                >
                                  {d.testing ? "テスト中..." : "Flash接続テスト"}
                                </button>
                                <button
                                  className="btn btn-secondary"
                                  onClick={() => handleResetDefaults(prefix)}
                                >
                                  標準設定に戻す
                                </button>
                              </>
                            )}
                          </div>

                          {/* OpenAI AI確認 info */}
                          {prefix === "OPENAI" && (
                            <p
                              style={{
                                fontSize: 11,
                                color: "var(--text-muted)",
                                marginTop: 12,
                                lineHeight: 1.5,
                              }}
                            >
                              AI確認では OpenAI Responses API + web_search tool を使用します。
                              <code style={{ fontSize: 11 }}>POST https://api.openai.com/v1/responses</code>{" "}
                              を呼び出し、モデルは上記の「Pro相当モデル」設定が使われます。
                              Chat互換APIとは異なるエンドポイントです。
                            </p>
                          )}

                          {/* Gemini AI確認 info */}
                          {prefix === "GEMINI" && (
                            <p
                              style={{
                                fontSize: 11,
                                color: "var(--text-muted)",
                                marginTop: 12,
                                lineHeight: 1.5,
                              }}
                            >
                              AI確認では Gemini native API + Google Search grounding を使用します。
                              Chat互換Base URLとは別に{" "}
                              <code style={{ fontSize: 11 }}>https://generativelanguage.googleapis.com/v1beta/models/&#123;model&#125;:generateContent</code>{" "}
                              を呼び出します。
                            </p>
                          )}

                          {/* Message */}
                          {msg && (
                            <p
                              style={{
                                fontSize: 12,
                                color: msg.type === "ok" ? "var(--success)" : "var(--error)",
                                marginTop: 10,
                                marginBottom: 0,
                              }}
                            >
                              {msg.text}
                            </p>
                          )}
                        </td>
                      </tr>
                    )}
                  </Fragment>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>

      <div className="card">
        <h2 className="card-title">作業別モデル</h2>
        <div style={{ display: "grid", gap: 10 }}>
          {TASK_MODEL_ROWS.map((row) => (
            <div
              key={row.key}
              style={{
                display: "grid",
                gridTemplateColumns: "180px minmax(0, 1fr)",
                gap: 12,
                alignItems: "center",
              }}
            >
              <label className="form-label" style={{ marginBottom: 0 }}>
                {row.label}
              </label>
              <select
                className="form-input"
                value={taskModels[row.key]}
                onChange={(e) => updateTaskModel(row.key, e.target.value as ModelTier)}
              >
                <option value="pro">Pro相当モデル</option>
                <option value="flash">Flash相当モデル</option>
              </select>
            </div>
          ))}
        </div>

        <div style={{ marginTop: 16 }}>
          <button className="btn btn-secondary" onClick={resetTaskModels}>
            標準値に戻す
          </button>
        </div>
      </div>

      {/* ---- TMDb settings ---- */}
      <ServiceSettingsPanel />

      {/* ---- About ---- */}
      <div className="card">
        <h2 className="card-title">このアプリについて</h2>
        <p
          style={{
            fontSize: 13,
            color: "var(--text-secondary)",
            lineHeight: 1.6,
          }}
        >
          Contextual Subtitle Studio — ドラマ特化の英語→日本語字幕翻訳ツール
          <br />
          バージョン 0.2.1
        </p>
      </div>
      </>}

      {/* ---- SRT tab ---- */}
      {activeTab === "srt" && <SrtSettingsTab />}
    </div>
  );
}

// ---- small render helpers ----

function connBadge(state: ConnState, configured: boolean) {
  if (state === "ok") return <span className="status-pill success">接続OK</span>;
  if (state === "fail") return <span className="status-pill high">接続失敗</span>;
  if (state === "configured" || configured) return <span className="status-pill medium">設定済み（未テスト）</span>;
  return <span style={{ color: "var(--text-muted)", fontSize: 12 }}>未設定</span>;
}

function FieldLabel({ children }: { children: string }) {
  return <label className="form-label" style={{ marginBottom: 0 }}>{children}</label>;
}

function FieldGrid({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "120px 1fr auto",
        gap: 12,
        alignItems: "center",
        marginBottom: 12,
      }}
    >
      {children}
    </div>
  );
}

function FieldGrid2({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "120px 1fr",
        gap: 12,
        alignItems: "center",
        marginBottom: 12,
      }}
    >
      {children}
    </div>
  );
}

// ---- provider helpers ----

function providerLabelStatic(prefix: string): string {
  const map: Record<string, string> = {
    DEEPSEEK: "DeepSeek",
    OPENAI: "OpenAI",
    ANTHROPIC: "Anthropic",
    CLAUDE: "Anthropic",
    GEMINI: "Gemini",
    GOOGLE: "Gemini",
    MINIMAX: "MiniMax",
    MOONSHOT: "Kimi / Moonshot",
    KIMI: "Kimi / Moonshot",
  };
  return map[prefix] || prefix;
}

function emptyDraft(prefix: string, configured: boolean): ProviderDraft {
  return {
    envVarName: `${prefix}_API_KEY`,
    apiKey: "",
    baseUrl: "",
    proModel: "",
    flashModel: "",
    defaultTier: "pro",
    supportsThinking: false,
    thinking: "disabled",
    connState: configured ? "configured" : "unset",
    testing: false,
    message: null,
  };
}

// ---- SRT settings tab ----

const DEFAULT_SRT_EN_PATTERN = "_en\\.srt$";

function SrtSettingsTab() {
  const [pattern, setPattern] = useState(DEFAULT_SRT_EN_PATTERN);
  const [message, setMessage] = useState<{ type: "ok" | "err"; text: string } | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const settings = await getServiceSettings();
        if (cancelled) return;
        setPattern(settings.srt_en_pattern || DEFAULT_SRT_EN_PATTERN);
        setLoaded(true);
      } catch {
        setLoaded(true);
      }
    })();
    return () => { cancelled = true; };
  }, []);

  const handleSave = async () => {
    try {
      const current = await getServiceSettings();
      await saveServiceSettings({ ...current, srt_en_pattern: pattern });
      setMessage({ type: "ok", "text": "保存しました。" });
    } catch (e) {
      setMessage({ type: "err", text: `保存に失敗しました: ${e}` });
    }
    setTimeout(() => setMessage(null), 4000);
  };

  const handleReset = () => {
    setPattern(DEFAULT_SRT_EN_PATTERN);
  };

  return (
    <div className="card">
      <h2 className="card-title">字幕読み込み</h2>
      <p
        style={{
          fontSize: 13,
          color: "var(--text-secondary)",
          marginBottom: 16,
          lineHeight: 1.6,
        }}
      >
        SRT読み込み画面でフォルダを選択したとき、どのファイルを英語字幕として検出するかを正規表現で設定します。
      </p>

      <FieldGrid>
        <FieldLabel>検出パターン</FieldLabel>
        <input
          className="form-input"
          value={pattern}
          onChange={(e) => setPattern(e.target.value)}
          placeholder={DEFAULT_SRT_EN_PATTERN}
          style={{ fontFamily: "monospace", fontSize: 13 }}
        />
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn btn-primary" onClick={handleSave}>
            保存
          </button>
          <button className="btn btn-secondary" onClick={handleReset}>
            デフォルト
          </button>
        </div>
      </FieldGrid>

      <p
        style={{
          fontSize: 12,
          color: "var(--text-muted)",
          lineHeight: 1.6,
          marginTop: -4,
          marginBottom: 12,
        }}
      >
        ファイル名がこの正規表現に一致する <code>.srt</code> ファイルが英語字幕として検出されます。
        <br />
        例: <code>_en\.srt$</code> → ファイル名に <code>_en</code> を含み <code>.srt</code> で終わるファイル
        <br />
        例: <code>English\.srt$</code> → ファイル名に <code>English</code> を含むファイル
      </p>

      {message && (
        <p
          style={{
            fontSize: 12,
            color: message.type === "ok" ? "var(--success)" : "var(--error)",
          }}
        >
          {message.text}
        </p>
      )}

      {!loaded && (
        <p style={{ fontSize: 12, color: "var(--text-muted)" }}>読み込み中...</p>
      )}
    </div>
  );
}
