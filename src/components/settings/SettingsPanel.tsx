import { Fragment, useEffect, useState } from "react";
import { useLlmStore } from "../../stores/useLlmStore";
import {
  setEnvVar,
  setActiveEnvVar,
  checkEnvVarKeyExists,
  listProviderPresets,
} from "../../lib/tauri";
import type { ProviderPreset } from "../../types";

type RowMessage = { type: "ok" | "err"; text: string } | null;

export default function SettingsPanel() {
  const { active, refresh } = useLlmStore();

  const [presets, setPresets] = useState<[string, ProviderPreset][]>([]);
  const [hasKey, setHasKey] = useState<Record<string, boolean>>({});
  const [expanded, setExpanded] = useState<string | null>(null);
  const [nameDraft, setNameDraft] = useState<Record<string, string>>({});
  const [keyDraft, setKeyDraft] = useState<Record<string, string>>({});
  const [msg, setMsg] = useState<Record<string, RowMessage>>({});

  // Load presets and status for each on mount
  useEffect(() => {
    let cancelled = false;
    (async () => {
      const p = await listProviderPresets();
      if (cancelled) return;
      setPresets(p);
      const initialNames: Record<string, string> = {};
      for (const [prefix] of p) {
        initialNames[prefix] = `${prefix}_API_KEY`;
      }
      setNameDraft(initialNames);
      // Check key existence for each preset
      const results = await Promise.all(
        p.map(async ([prefix]) => {
          const env = `${prefix}_API_KEY`;
          const exists = await checkEnvVarKeyExists(env);
          return [env, exists] as const;
        })
      );
      if (cancelled) return;
      const map: Record<string, boolean> = {};
      for (const [env, exists] of results) map[env] = exists;
      setHasKey(map);
      refresh();
    })();
    return () => {
      cancelled = true;
    };
  }, [refresh]);

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
    setMsg((m) => ({ ...m, [prefix]: null }));
  };

  const handleSaveKey = async (prefix: string) => {
    const name = (nameDraft[prefix] || "").trim().toUpperCase();
    const value = keyDraft[prefix] || "";
    if (!name) {
      setMsg((m) => ({ ...m, [prefix]: { type: "err", text: "環境変数名を入力してください。" } }));
      return;
    }
    if (!value) {
      setMsg((m) => ({ ...m, [prefix]: { type: "err", text: "APIキーを入力してください。" } }));
      return;
    }
    try {
      await setEnvVar(name, value);
      setMsg((m) => ({ ...m, [prefix]: { type: "ok", text: `${name} を保存しました。` } }));
      setKeyDraft((k) => ({ ...k, [prefix]: "" }));
      setHasKey((h) => ({ ...h, [name]: true }));
      setTimeout(() => setMsg((m) => ({ ...m, [prefix]: null })), 2500);
    } catch (e) {
      setMsg((m) => ({ ...m, [prefix]: { type: "err", text: `エラー: ${e}` } }));
    }
  };

  return (
    <div style={{ maxWidth: 900 }}>
      <div className="card" style={{ padding: 0 }}>
        <h2 className="card-title" style={{ padding: "18px 18px 12px 18px" }}>
          APIキー
        </h2>
        <div className="table-container" style={{ border: "none", borderRadius: 0 }}>
          <table>
            <thead>
              <tr>
                <th style={{ width: 32 }}></th>
                <th style={{ width: "25%" }}>Provider</th>
                <th style={{ width: "25%" }}>Env Var</th>
                <th style={{ width: "20%" }}>Status</th>
                <th style={{ width: "20%", textAlign: "right" }}>Action</th>
              </tr>
            </thead>
            <tbody>
              {presets.map(([prefix, preset]) => {
                const envName = `${prefix}_API_KEY`;
                const isOpen = expanded === prefix;
                const configured = !!hasKey[envName];
                const isActive = active.name === envName;
                const message = msg[prefix];
                return (
                  <Fragment key={prefix}>
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
                        <strong>{preset.provider}</strong>
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
                      <td style={{ fontFamily: "monospace", fontSize: 12, color: "var(--text-secondary)" }}>
                        {envName}
                      </td>
                      <td>
                        {configured ? (
                          <span className="status-pill success">設定済</span>
                        ) : (
                          <span style={{ color: "var(--text-muted)", fontSize: 12 }}>未設定</span>
                        )}
                      </td>
                      <td style={{ textAlign: "right" }}>
                        <button
                          className="btn btn-secondary"
                          onClick={() => toggleRow(prefix)}
                        >
                          {isOpen ? "閉じる" : "編集"}
                        </button>
                      </td>
                    </tr>
                    {isOpen && (
                      <tr>
                        <td
                          colSpan={5}
                          style={{
                            backgroundColor: "var(--bg-elevated)",
                            padding: "14px 18px",
                          }}
                        >
                          <div
                            style={{
                              display: "grid",
                              gridTemplateColumns: "120px 1fr",
                              gap: 12,
                              alignItems: "center",
                              marginBottom: 12,
                            }}
                          >
                            <label className="form-label" style={{ marginBottom: 0 }}>
                              環境変数名
                            </label>
                            <input
                              className="form-input"
                              value={nameDraft[prefix] || ""}
                              onChange={(e) =>
                                setNameDraft((d) => ({ ...d, [prefix]: e.target.value }))
                              }
                              style={{ fontFamily: "monospace" }}
                            />
                          </div>
                          <div
                            style={{
                              display: "grid",
                              gridTemplateColumns: "120px 1fr auto",
                              gap: 12,
                              alignItems: "center",
                            }}
                          >
                            <label className="form-label" style={{ marginBottom: 0 }}>
                              APIキー
                            </label>
                            <input
                              className="form-input"
                              type="password"
                              value={keyDraft[prefix] || ""}
                              onChange={(e) =>
                                setKeyDraft((d) => ({ ...d, [prefix]: e.target.value }))
                              }
                              placeholder="sk-..."
                            />
                            <button
                              className="btn btn-primary"
                              onClick={() => handleSaveKey(prefix)}
                            >
                              キーを保存
                            </button>
                          </div>
                          {message && (
                            <p
                              style={{
                                fontSize: 12,
                                color:
                                  message.type === "ok"
                                    ? "var(--success)"
                                    : "var(--error)",
                                marginTop: 10,
                                marginBottom: 0,
                              }}
                            >
                              {message.text}
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
