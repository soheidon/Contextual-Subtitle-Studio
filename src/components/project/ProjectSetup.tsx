import { useState } from "react";
import { FolderOpen, Plus } from "lucide-react";
import { useProjectStore } from "../../stores/useProjectStore";

export default function ProjectSetup() {
  const { isOpen, projectName, setProject } = useProjectStore();
  const [name, setName] = useState(projectName || "");
  const [baseDir, setBaseDir] = useState("");

  const handleCreate = async () => {
    setProject(name, baseDir);
  };

  const handleOpen = async () => {
    setProject("opened-project", "/some/path");
  };

  return (
    <div className="card" style={{ maxWidth: 480 }}>
      <h2 className="card-title">プロジェクト設定</h2>

      {isOpen ? (
        <div>
          <p style={{ color: "var(--text-secondary)", marginBottom: 16 }}>
            プロジェクト <strong>{projectName}</strong> が開かれています。
          </p>
          <p style={{ color: "var(--text-secondary)", fontSize: 13 }}>
            サイドバーから操作を進めてください：SRT読み込み → 辞書設定 → LLM設定 → 翻訳 → レビュー
          </p>
        </div>
      ) : (
        <div>
          <div className="form-group">
            <label className="form-label">プロジェクト名</label>
            <input
              className="form-input"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="例: Frozen Awakening S01E01"
            />
          </div>

          <div className="form-group">
            <label className="form-label">作業フォルダ</label>
            <div style={{ display: "flex", gap: 8 }}>
              <input
                className="form-input"
                value={baseDir}
                onChange={(e) => setBaseDir(e.target.value)}
                placeholder="フォルダを選択..."
                readOnly
              />
              <button className="btn btn-secondary" onClick={handleOpen}>
                <FolderOpen size={16} />
                参照
              </button>
            </div>
          </div>

          <button
            className="btn btn-primary"
            onClick={handleCreate}
            disabled={!name || !baseDir}
          >
            <Plus size={16} />
            プロジェクト作成
          </button>

          <div style={{ marginTop: 24 }}>
            <button className="btn btn-secondary" onClick={handleOpen}>
              <FolderOpen size={16} />
              既存プロジェクトを開く
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
