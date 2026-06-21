import { useTranslationStore } from "../../stores/useTranslationStore";
import { AlertCircle } from "lucide-react";

const severityColors: Record<string, string> = {
  high: "var(--error)",
  medium: "var(--warning)",
  low: "var(--text-secondary)",
};

const severityLabels: Record<string, string> = {
  high: "高",
  medium: "中",
  low: "低",
};

export default function IssueList() {
  const { issues } = useTranslationStore();

  if (issues.length === 0) {
    return (
      <div className="card">
        <h2 className="card-title">レビュー課題</h2>
        <p style={{ color: "var(--success)" }}>
          課題は検出されませんでした。
        </p>
      </div>
    );
  }

  const highCount = issues.filter((i) => i.severity === "high").length;
  const mediumCount = issues.filter((i) => i.severity === "medium").length;
  const lowCount = issues.filter((i) => i.severity === "low").length;

  return (
    <div className="card">
      <h2 className="card-title">
        レビュー課題
        <span style={{ fontSize: 13, color: "var(--text-secondary)", marginLeft: 8 }}>
          ({`合計${issues.length}件 (高${highCount}件 中${mediumCount}件 低${lowCount}件)`})
        </span>
      </h2>

      <div className="table-container" style={{ maxHeight: 500, overflowY: "auto" }}>
        <table>
          <thead>
            <tr>
              <th style={{ width: 60 }}>#</th>
              <th style={{ width: 80 }}>種類</th>
              <th style={{ width: 70 }}>重要度</th>
              <th>メッセージ</th>
              <th>翻訳</th>
            </tr>
          </thead>
          <tbody>
            {issues.map((issue) => (
              <tr key={`${issue.index}-${issue.issue_type}`}>
                <td>{issue.index}</td>
                <td>
                  <code style={{ fontSize: 11, background: "var(--bg-card)", padding: "2px 6px", borderRadius: 4 }}>
                    {issue.issue_type}
                  </code>
                </td>
                <td>
                  <span
                    style={{
                      color: severityColors[issue.severity] || severityColors.low,
                      fontWeight: 600,
                      fontSize: 12,
                      display: "flex",
                      alignItems: "center",
                      gap: 4,
                    }}
                  >
                    <AlertCircle size={12} />
                    {severityLabels[issue.severity] || issue.severity}
                  </span>
                </td>
                <td style={{ fontSize: 13 }}>{issue.message}</td>
                <td style={{ fontSize: 13, maxWidth: 300 }}>
                  <div>{issue.translation}</div>
                  {issue.suggestion && (
                    <div style={{ color: "var(--success)", fontSize: 12, marginTop: 4 }}>
                      修正案: {issue.suggestion}
                    </div>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
