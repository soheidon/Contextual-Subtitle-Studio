import { useState } from "react";
import { FileUp, Plus, Trash2 } from "lucide-react";
import { useDictionaryStore, type GlossaryEntry } from "../../stores/useDictionaryStore";
import { loadGlossaryDictionary } from "../../lib/tauri";

const emptyEntry = (): GlossaryEntry => ({
  source: "",
  target: "",
  type: "character",
});

const typeLabels: Record<string, string> = {
  character: "キャラクター",
  title: "称号",
  place: "場所",
  organization: "組織",
  weapon: "武器",
  technique: "技",
  other: "その他",
};

export default function GlossaryTable() {
  const { glossary, setGlossary, glossaryFilePath } = useDictionaryStore();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleOpenFile = async () => {
    try {
      setLoading(true);
      setError(null);
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        filters: [{ name: "用語集ファイル", extensions: ["json", "csv"] }],
      });
      if (selected) {
        const entries = await loadGlossaryDictionary(selected as string);
        setGlossary(entries, selected as string);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const addRow = () => {
    setGlossary([...glossary, emptyEntry()], glossaryFilePath || undefined);
  };

  const removeRow = (idx: number) => {
    const updated = glossary.filter((_, i) => i !== idx);
    setGlossary(updated, glossaryFilePath || undefined);
  };

  const updateField = (idx: number, field: keyof GlossaryEntry, value: string) => {
    const updated = glossary.map((e, i) =>
      i === idx ? { ...e, [field]: value } : e
    );
    setGlossary(updated, glossaryFilePath || undefined);
  };

  return (
    <div className="card">
      <div className="flex items-center justify-between mb-16">
        <h2 className="card-title" style={{ marginBottom: 0 }}>
          用語集
          {glossaryFilePath && (
            <span style={{ fontSize: 12, color: "var(--text-secondary)", marginLeft: 8 }}>
              ({glossaryFilePath.split(/[/\\]/).pop()})
            </span>
          )}
        </h2>
        <div className="flex gap-8">
          <button className="btn btn-secondary" onClick={handleOpenFile} disabled={loading}>
            <FileUp size={16} />
            読み込み
          </button>
          <button className="btn btn-primary" onClick={addRow}>
            <Plus size={16} />
            追加
          </button>
        </div>
      </div>

      {error && (
        <p style={{ color: "var(--error)", marginBottom: 12, fontSize: 13 }}>{error}</p>
      )}

      <div className="table-container" style={{ maxHeight: 400, overflowY: "auto" }}>
        <table>
          <thead>
            <tr>
              <th>原文</th>
              <th>訳文</th>
              <th>種類</th>
              <th>備考</th>
              <th style={{ width: 40 }}></th>
            </tr>
          </thead>
          <tbody>
            {glossary.length === 0 && (
              <tr>
                <td colSpan={5} style={{ textAlign: "center", color: "var(--text-secondary)" }}>
                  {`用語が登録されていません。「読み込み」または「追加」をクリックしてください。`}
                </td>
              </tr>
            )}
            {glossary.map((e, i) => (
              <tr key={i}>
                <td>
                  <input
                    className="form-input"
                    value={e.source}
                    onChange={(ev) => updateField(i, "source", ev.target.value)}
                    style={{ width: 150 }}
                  />
                </td>
                <td>
                  <input
                    className="form-input"
                    value={e.target}
                    onChange={(ev) => updateField(i, "target", ev.target.value)}
                    style={{ width: 150 }}
                  />
                </td>
                <td>
                  <select
                    className="form-input"
                    value={e.type}
                    onChange={(ev) => updateField(i, "type", ev.target.value)}
                    style={{ width: 140 }}
                  >
                    {["character", "title", "place", "organization", "weapon", "technique", "other"].map(
                      (t) => (
                        <option key={t} value={t}>{typeLabels[t] || t}</option>
                      )
                    )}
                  </select>
                </td>
                <td>
                  <input
                    className="form-input"
                    value={e.notes || ""}
                    onChange={(ev) => updateField(i, "notes", ev.target.value)}
                    style={{ width: 200 }}
                  />
                </td>
                <td>
                  <button
                    className="btn btn-secondary"
                    onClick={() => removeRow(i)}
                    style={{ padding: "4px 8px" }}
                  >
                    <Trash2 size={14} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
