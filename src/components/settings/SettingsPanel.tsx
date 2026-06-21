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

export default function SettingsPanel() {
  const { active, refresh } = useLlmStore();
  const addLog = useAppLogStore((s) => s.addLog);

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

  const handleResetDefaults = (prefix: string) => {
    // Reset to preset defaults — save empty overrides so defaults apply
    saveProviderSettings(prefix, { base_url: null, model: null, thinking: null }).catch(() => {});
    updateDraft(prefix, {
      envVarName: `${prefix}_API_KEY`,
      baseUrl: "",
      model: "",
      thinking: "disabled",
    });
    addLog("info", providerLabelStatic(prefix), "デフォルト設定に戻しました");
  };

  return (
    <div style={{ maxWidth: 900 }}>
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
                              style={{ fontFamily: "monospace" }}
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
                              placeholder="sk-..."
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
                          </div>

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
          バージョン 0.1.0 MVP
        </p>
      </div>
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