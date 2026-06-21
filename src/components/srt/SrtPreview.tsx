import { useSrtStore } from "../../stores/useSrtStore";

export default function SrtPreview() {
  const { entries, isLoaded, fileName } = useSrtStore();

  if (!isLoaded) {
    return (
      <div className="card">
        <h2 className="card-title">SRTプレビュー</h2>
        <p style={{ color: "var(--text-secondary)" }}>
          SRTファイルが読み込まれていません。上部の読み込み機能でファイルを開いてください。
        </p>
      </div>
    );
  }

  return (
    <div className="card">
      <h2 className="card-title">
        SRTプレビュー — {fileName}
        <span style={{ fontSize: 13, color: "var(--text-secondary)", marginLeft: 8 }}>
          ({entries.length}件)
        </span>
      </h2>
      <div className="table-container" style={{ maxHeight: 400, overflowY: "auto" }}>
        <table>
          <thead>
            <tr>
              <th style={{ width: 60 }}>#</th>
              <th style={{ width: 120 }}>開始</th>
              <th style={{ width: 120 }}>終了</th>
              <th>テキスト</th>
            </tr>
          </thead>
          <tbody>
            {entries.map((e) => (
              <tr key={e.index}>
                <td>{e.index}</td>
                <td style={{ fontFamily: "monospace", fontSize: 12 }}>{e.start}</td>
                <td style={{ fontFamily: "monospace", fontSize: 12 }}>{e.end}</td>
                <td>{e.text}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
