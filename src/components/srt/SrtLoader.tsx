import { useState } from "react";
import { FileUp } from "lucide-react";
import { useSrtStore } from "../../stores/useSrtStore";
import { parseSrtFile } from "../../lib/tauri";

export default function SrtLoader() {
  const { isLoaded, fileName, setEntries } = useSrtStore();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleOpenFile = async () => {
    try {
      setLoading(true);
      setError(null);
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        filters: [{ name: "SRTファイル", extensions: ["srt"] }],
      });
      if (selected) {
        const path = selected as string;
        const entries = await parseSrtFile(path);
        const name = path.split(/[/\\]/).pop() || "不明";
        setEntries(entries, name, path);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  if (isLoaded) {
    return (
      <div className="card">
        <h2 className="card-title">SRT読み込み完了</h2>
        <p style={{ color: "var(--text-secondary)", marginBottom: 12 }}>
          ファイル: <strong>{fileName}</strong>
        </p>
        <button className="btn btn-secondary" onClick={handleOpenFile}>
          <FileUp size={16} />
          別のファイルを読み込む
        </button>
      </div>
    );
  }

  return (
    <div className="card" style={{ maxWidth: 480 }}>
      <h2 className="card-title">SRTファイル読み込み</h2>
      <p style={{ color: "var(--text-secondary)", marginBottom: 16 }}>
        翻訳する英語SRTファイルを選択してください。
      </p>
      <button
        className="btn btn-primary"
        onClick={handleOpenFile}
        disabled={loading}
      >
        <FileUp size={16} />
        {loading ? "読み込み中..." : "SRTファイルを開く"}
      </button>
      {error && (
        <p style={{ color: "var(--error)", marginTop: 12, fontSize: 13 }}>
          {error}
        </p>
      )}
    </div>
  );
}
