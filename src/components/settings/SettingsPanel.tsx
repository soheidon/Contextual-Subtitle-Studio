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
  testOpenaiAiConfirm,
} from "../../lib/tauri";
import type { ProviderPreset } from "../../types";
import ServiceSettingsPanel from "./ServiceSettings";

// ---- Constants ----

const DEEPSEEK_PRESETS = ["deepseek-v4-flash", "deepseek-v4-pro"];
const CUSTOM_KEY = "__custom__";
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
  model: string;
  thinking: string;
  connState: ConnState;
  testing: boolean;
  message: RowMessage;
}

const DESC: Record<string, string> = {
  DEEPSEEK:
    "DeepSeek V4系モデルは Thinking / Non-Thinking の両モードに対応しています。通常は無効、高精度な推論が必要な場合は有効にしてください。",
};

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

  // Load presets on mount, then load per-provider settings
  useEffect(() => {
    let cancelled = false;
    (async () => {
      const p = await listProviderPresets();
      if (cancelled) return;
      setPresets(p);

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
            model: ps.model,
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

  const handleTestConnection = async (prefix: string) => {
    const d = getDraft(prefix);
    const tag = providerLabelStatic(prefix);
    updateDraft(prefix, { testing: true, message: null });

    // Persist settings first so overrides apply
    try {
      await saveProviderSettings(prefix, {
        base_url: d.baseUrl || null,
        model: d.model || null,
        thinking: d.thinking || null,
      });
    } catch {
      // save is best-effort for the test
    }

    const extra =
      prefix === "DEEPSEEK" ? `, thinking=${d.thinking}` : "";
    addLog("info", tag, `接続テスト開始: model=${d.model}${extra}`);

    try {
      const has = await checkEnvVarKeyExists(d.envVarName);
      if (!has) {
        updateDraft(prefix, { testing: false, connState: "unset" });
        setMsg(prefix, "err", "APIキーが未設定です。");
        addLog("warning", tag, "接続テスト: APIキーが未設定です");
        return;
      }
      await checkActiveConnection(d.envVarName);
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
        model: d.model || null,
        thinking: d.thinking || null,
      });
    } catch {
      // save is best-effort for the test
    }

    addLog("info", tag, `AI確認テスト開始: model=${d.model}`);

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
    saveProviderSettings(prefix, { base_url: null, model: null, thinking: null }).catch(() => {});
    // Show preset defaults immediately in the UI
    const preset = presets.find(([p]) => p === prefix)?.[1];
    const defaults = preset
      ? { baseUrl: preset.base_url, model: preset.model, thinking: "disabled" }
      : { baseUrl: "", model: "", thinking: "disabled" };
    updateDraft(prefix, {
      envVarName: `${prefix}_API_KEY`,
      ...defaults,
    });
    addLog("info", providerLabelStatic(prefix), "デフォルト設定に戻しました");
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
                const isDs = prefix === "DEEPSEEK";
                const isKnownPreset = isDs ? DEEPSEEK_PRESETS.includes(d.model) : false;
                const modelSelectValue = isDs && !isKnownPreset ? CUSTOM_KEY : d.model;

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

                          {/* Model */}
                          <FieldGrid2>
                            <FieldLabel>使用モデル</FieldLabel>
                            <div style={{ display: "flex", gap: 8 }}>
                              {isDs ? (
                                <>
                                  <select
                                    className="form-input"
                                    style={{ flex: 1 }}
                                    value={modelSelectValue}
                                    onChange={(e) => {
                                      const val = e.target.value;
                                      if (val === CUSTOM_KEY) {
                                        updateDraft(prefix, { model: d.model || "deepseek-v4-flash" });
                                      } else {
                                        updateDraft(prefix, { model: val });
                                      }
                                    }}
                                  >
                                    {DEEPSEEK_PRESETS.map((m) => (
                                      <option key={m} value={m}>{m}</option>
                                    ))}
                                    <option value={CUSTOM_KEY}>カスタム</option>
                                  </select>
                                </>
                              ) : (
                                <input
                                  className="form-input"
                                  value={d.model}
                                  onChange={(e) => updateDraft(prefix, { model: e.target.value })}
                                  placeholder={presets.find(([p]) => p === prefix)?.[1].model || ""}
                                  style={{ fontFamily: "monospace", fontSize: 12 }}
                                />
                              )}
                            </div>
                          </FieldGrid2>

                          {/* Custom model input (DeepSeek) */}
                          {isDs && !isKnownPreset && (
                            <FieldGrid2>
                              <FieldLabel>カスタムモデル</FieldLabel>
                              <input
                                className="form-input"
                                value={d.model}
                                onChange={(e) => updateDraft(prefix, { model: e.target.value })}
                                placeholder="deepseek-v4-flash"
                                style={{ fontFamily: "monospace", fontSize: 12 }}
                              />
                            </FieldGrid2>
                          )}

                          {/* Thinking (DeepSeek only) */}
                          {isDs && (
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
                              {DESC[prefix] && (
                                <p
                                  style={{
                                    fontSize: 11,
                                    color: "var(--text-muted)",
                                    margin: "-8px 0 14px 120px",
                                    lineHeight: 1.4,
                                  }}
                                >
                                  {DESC[prefix]}
                                </p>
                              )}
                            </>
                          )}

                          {/* Action buttons */}
                          <div style={{ display: "flex", gap: 10, alignItems: "center", marginTop: 16 }}>
                            {prefix === "OPENAI" ? (
                              <>
                                <button
                                  className="btn btn-primary"
                                  onClick={() => handleTestConnection(prefix)}
                                  disabled={d.testing}
                                >
                                  {d.testing ? "テスト中..." : "Chat接続テスト"}
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
                                  Defaultに設定
                                </button>
                              </>
                            ) : (
                              <>
                                <button
                                  className="btn btn-primary"
                                  onClick={() => handleTestConnection(prefix)}
                                  disabled={d.testing}
                                >
                                  {d.testing ? "テスト中..." : "接続テスト"}
                                </button>
                                <button
                                  className="btn btn-secondary"
                                  onClick={() => handleResetDefaults(prefix)}
                                >
                                  Defaultに設定
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
                              を呼び出し、モデルは上記の「使用モデル」設定が使われます。
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
    model: "",
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