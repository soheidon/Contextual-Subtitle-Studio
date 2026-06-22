import { useEffect, useState, useCallback } from "react";
import {
  setEnvVar,
  getEnvVar,
  checkEnvVarKeyExists,
  getServiceSettings,
  saveServiceSettings,
  testTmdbConnection,
} from "../../lib/tauri";
import { useAppLogStore } from "../../stores/useAppLogStore";

type ConnState = "unset" | "configured" | "ok" | "fail";

interface TmdbState {
  expanded: boolean;
  envVarName: string;
  apiKey: string;
  baseUrl: string;
  connState: ConnState;
  testing: boolean;
  message: { type: "ok" | "err"; text: string } | null;
}

const DEFAULT_TMDB_ENV = "TMDB_API_KEY";
const DEFAULT_TMDB_URL = "https://api.themoviedb.org";

export default function ServiceSettingsPanel() {
  const addLog = useAppLogStore((s) => s.addLog);

  const [tmdb, setTmdb] = useState<TmdbState>({
    expanded: false,
    envVarName: DEFAULT_TMDB_ENV,
    apiKey: "",
    baseUrl: DEFAULT_TMDB_URL,
    connState: "unset",
    testing: false,
    message: null,
  });

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const settings = await getServiceSettings();
        if (cancelled) return;
        const hasKey = await checkEnvVarKeyExists(settings.tmdb_env_var_name);
        setTmdb((p) => ({
          ...p,
          envVarName: settings.tmdb_env_var_name,
          baseUrl: settings.tmdb_base_url,
          connState: hasKey ? "configured" : "unset",
        }));
      } catch {
        // defaults already set
      }
    })();
    return () => { cancelled = true; };
  }, []);

  const setMsg = useCallback(
    (type: "ok" | "err", text: string) => {
      setTmdb((p) => ({ ...p, message: { type, text } }));
      setTimeout(() => setTmdb((p) => (p.message?.text === text ? { ...p, message: null } : p)), 4000);
    },
    [],
  );

  const handleSaveKey = async () => {
    const envName = tmdb.envVarName.trim().toUpperCase();
    const value = tmdb.apiKey;
    if (!envName) {
      setMsg("err", "環境変数名を入力してください。");
      return;
    }
    if (!value) {
      setMsg("err", "APIキーを入力してください。");
      return;
    }
    try {
      await setEnvVar(envName, value);
      addLog("success", "TMDb", "APIキーを保存しました");
      setMsg("ok", `${envName} を保存しました。`);
      setTmdb((p) => ({ ...p, apiKey: "", connState: "configured" }));
    } catch (e) {
      addLog("error", "TMDb", `保存失敗: ${e}`);
      setMsg("err", `エラー: ${e}`);
    }
  };

  const handleTestConnection = async () => {
    setTmdb((p) => ({ ...p, testing: true, message: null }));
    addLog("info", "TMDb", "接続テスト開始");

    try {
      const key = await getEnvVar(tmdb.envVarName);
      if (!key) {
        setTmdb((p) => ({ ...p, testing: false, connState: "unset" }));
        setMsg("err", "APIキーが未設定です。");
        addLog("warning", "TMDb", "接続テスト: APIキーが未設定です");
        return;
      }
      await testTmdbConnection(key, tmdb.baseUrl);
      setTmdb((p) => ({ ...p, testing: false, connState: "ok" }));
      setMsg("ok", "接続OK");
      addLog("success", "TMDb", "接続OK");
    } catch (e) {
      setTmdb((p) => ({ ...p, testing: false, connState: "fail" }));
      setMsg("err", `接続失敗: ${e}`);
      addLog("error", "TMDb", `接続失敗: ${e}`);
    }
  };

  const handleResetDefaults = async () => {
    setTmdb((p) => ({
      ...p,
      envVarName: DEFAULT_TMDB_ENV,
      baseUrl: DEFAULT_TMDB_URL,
    }));
    try {
      const current = await getServiceSettings();
      await saveServiceSettings({
        ...current,
        tmdb_env_var_name: DEFAULT_TMDB_ENV,
        tmdb_base_url: DEFAULT_TMDB_URL,
      });
    } catch {
      // best effort
    }
    addLog("info", "TMDb", "デフォルト設定に戻しました");
  };

  const connBadge = (state: ConnState) => {
    switch (state) {
      case "unset":
        return <span style={{ color: "var(--text-muted)", fontSize: 12 }}>未設定</span>;
      case "configured":
        return <span className="status-pill medium">設定済み（未テスト）</span>;
      case "ok":
        return <span className="status-pill success">接続OK</span>;
      case "fail":
        return <span className="status-pill high">接続失敗</span>;
    }
  };

  const isOpen = tmdb.expanded;
  const msg = tmdb.message;

  return (
    <div className="card" style={{ padding: 0, overflow: "hidden" }}>
      {/* Header */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 14,
          padding: "12px 18px",
          cursor: "pointer",
          userSelect: "none",
          borderBottom: isOpen ? "1px solid var(--border)" : "none",
        }}
        onClick={() => setTmdb((p) => ({ ...p, expanded: !p.expanded }))}
      >
        <div style={{ flex: 1 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <strong style={{ fontSize: 15 }}>TMDb</strong>
            <span style={{ fontSize: 12, color: "var(--text-muted)" }}>TMDb API</span>
          </div>
        </div>
        {connBadge(tmdb.connState)}
        <button
          className="btn btn-secondary"
          style={{ fontSize: 11, padding: "3px 10px", minHeight: 24 }}
          onClick={(e) => {
            e.stopPropagation();
            setTmdb((p) => ({ ...p, expanded: !p.expanded }));
          }}
        >
          {isOpen ? "閉じる" : "開く"}
        </button>
      </div>

      {/* Expanded body */}
      {isOpen && (
        <div style={{ padding: "18px 18px 18px 138px" }}>
          {/* Env var name */}
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "120px 1fr",
              gap: 12,
              alignItems: "center",
              marginBottom: 14,
              marginLeft: -120,
            }}
          >
            <label className="form-label" style={{ marginBottom: 0 }}>
              環境変数名
            </label>
            <input
              className="form-input"
              value={tmdb.envVarName}
              onChange={(e) => setTmdb((p) => ({ ...p, envVarName: e.target.value }))}
              style={{ fontFamily: "monospace" }}
            />
          </div>

          {/* API key + save */}
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "120px 1fr auto",
              gap: 12,
              alignItems: "center",
              marginBottom: 14,
              marginLeft: -120,
            }}
          >
            <label className="form-label" style={{ marginBottom: 0 }}>
              APIキー
            </label>
            <input
              className="form-input"
              type="password"
              value={tmdb.apiKey}
              onChange={(e) => setTmdb((p) => ({ ...p, apiKey: e.target.value }))}
              placeholder="tmdb api key..."
            />
            <button className="btn btn-primary" onClick={handleSaveKey}>
              環境変数に保存
            </button>
          </div>

          {/* Base URL */}
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "120px 1fr",
              gap: 12,
              alignItems: "center",
              marginBottom: 14,
              marginLeft: -120,
            }}
          >
            <label className="form-label" style={{ marginBottom: 0 }}>
              Base URL
            </label>
            <input
              className="form-input"
              value={tmdb.baseUrl}
              onChange={(e) => setTmdb((p) => ({ ...p, baseUrl: e.target.value }))}
              style={{ fontFamily: "monospace", fontSize: 12 }}
            />
          </div>

          {/* Actions */}
          <div style={{ display: "flex", gap: 10, alignItems: "center", marginTop: 16, marginLeft: -120 }}>
            <button
              className="btn btn-primary"
              onClick={handleTestConnection}
              disabled={tmdb.testing}
            >
              {tmdb.testing ? "テスト中..." : "接続テスト"}
            </button>
            <button className="btn btn-secondary" onClick={handleResetDefaults}>
              Defaultに設定
            </button>
          </div>

          {msg && (
            <p
              style={{
                fontSize: 12,
                color: msg.type === "ok" ? "var(--success)" : "var(--error)",
                marginTop: 10,
                marginBottom: 0,
                marginLeft: -120,
              }}
            >
              {msg.text}
            </p>
          )}

        </div>
      )}
    </div>
  );
}
