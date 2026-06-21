import { useState } from "react";
import { FileUp, Plus, Trash2 } from "lucide-react";
import { useDictionaryStore, type Character } from "../../stores/useDictionaryStore";
import { loadCharacterDictionary } from "../../lib/tauri";

const emptyChar = (): Character => ({
  id: "",
  english_name: "",
  japanese_name: "",
  aliases: [],
  default_register: "neutral",
});

const registerLabels: Record<string, string> = {
  plain: "タメ口",
  neutral: "標準",
  polite: "丁寧語",
  formal: "敬語",
  honorific: "尊敬語/謙譲語",
  commanding: "命令口調",
  hostile: "敵対的",
  intimate: "親密",
};

export default function CharacterDict() {
  const { characters, setCharacters, charFilePath } = useDictionaryStore();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleOpenFile = async () => {
    try {
      setLoading(true);
      setError(null);
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        filters: [{ name: "辞書ファイル", extensions: ["json", "csv"] }],
      });
      if (selected) {
        const chars = await loadCharacterDictionary(selected as string);
        setCharacters(chars, selected as string);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const addRow = () => {
    setCharacters([...characters, emptyChar()], charFilePath || undefined);
  };

  const removeRow = (idx: number) => {
    const updated = characters.filter((_, i) => i !== idx);
    setCharacters(updated, charFilePath || undefined);
  };

  const updateField = (idx: number, field: keyof Character, value: unknown) => {
    const updated = characters.map((c, i) =>
      i === idx ? { ...c, [field]: value } : c
    );
    setCharacters(updated, charFilePath || undefined);
  };

  return (
    <div className="card">
      <div className="flex items-center justify-between mb-16">
        <h2 className="card-title" style={{ marginBottom: 0 }}>
          キャラクター辞書
          {charFilePath && (
            <span style={{ fontSize: 12, color: "var(--text-secondary)", marginLeft: 8 }}>
              ({charFilePath.split(/[/\\]/).pop()})
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
              <th>ID</th>
              <th>英語名</th>
              <th>日本語名</th>
              <th>別名</th>
              <th>敬語レベル</th>
              <th>役割</th>
              <th style={{ width: 40 }}></th>
            </tr>
          </thead>
          <tbody>
            {characters.length === 0 && (
              <tr>
                <td colSpan={7} style={{ textAlign: "center", color: "var(--text-secondary)" }}>
                  {`キャラクターが登録されていません。「読み込み」または「追加」をクリックしてください。`}
                </td>
              </tr>
            )}
            {characters.map((c, i) => (
              <tr key={i}>
                <td>
                  <input
                    className="form-input"
                    value={c.id}
                    onChange={(e) => updateField(i, "id", e.target.value)}
                    style={{ width: 100 }}
                  />
                </td>
                <td>
                  <input
                    className="form-input"
                    value={c.english_name}
                    onChange={(e) => updateField(i, "english_name", e.target.value)}
                    style={{ width: 120 }}
                  />
                </td>
                <td>
                  <input
                    className="form-input"
                    value={c.japanese_name}
                    onChange={(e) => updateField(i, "japanese_name", e.target.value)}
                    style={{ width: 120 }}
                  />
                </td>
                <td>
                  <input
                    className="form-input"
                    value={c.aliases.join(";")}
                    onChange={(e) =>
                      updateField(
                        i,
                        "aliases",
                        e.target.value.split(";").map((s) => s.trim()).filter(Boolean)
                      )
                    }
                    style={{ width: 120 }}
                    placeholder="alias1; alias2"
                  />
                </td>
                <td>
                  <select
                    className="form-input"
                    value={c.default_register}
                    onChange={(e) => updateField(i, "default_register", e.target.value)}
                    style={{ width: 140 }}
                  >
                    {["plain", "neutral", "polite", "formal", "honorific", "commanding", "hostile", "intimate"].map(
                      (r) => (
                        <option key={r} value={r}>{registerLabels[r] || r}</option>
                      )
                    )}
                  </select>
                </td>
                <td>
                  <input
                    className="form-input"
                    value={c.role || ""}
                    onChange={(e) => updateField(i, "role", e.target.value || undefined)}
                    style={{ width: 100 }}
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
