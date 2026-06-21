import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useLlmStore } from "../../stores/useLlmStore";

export default function ProviderConfig() {
  const navigate = useNavigate();
  const { refresh, active } = useLlmStore();

  useEffect(() => {
    refresh();
  }, [refresh]);

  return (
    <div className="card" style={{ maxWidth: 560 }}>
      <h2 className="card-title">LLM</h2>
      {active.name ? (
        <div style={{ marginBottom: 12 }}>
          <p style={{ color: "var(--text-secondary)", marginBottom: 8 }}>
            現在: <strong>{active.provider || "カスタム"}</strong> · <code>{active.name}</code> · {active.has_key ? "設定済" : "未設定"}
          </p>
        </div>
      ) : (
        <p style={{ color: "var(--text-secondary)", marginBottom: 12 }}>
          LLMが未設定です。設定画面で環境変数名とAPIキーを保存してください。
        </p>
      )}
      <button className="btn btn-primary" onClick={() => navigate("/settings")}>
        設定を開く
      </button>
    </div>
  );
}
