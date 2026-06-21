import type { MergedCharacter, MatchStatus } from "../../types";

interface Props {
  characters: MergedCharacter[];
  filterStatus: MatchStatus | "All";
  onFilterChange: (s: MatchStatus | "All") => void;
  onUpdateCharacter: (index: number, updates: Partial<MergedCharacter>) => void;
}

const STATUS_CONFIG: Record<MatchStatus, { label: string; className: string }> = {
  AutoMatched: { label: "✓", className: "status-pill success" },
  Candidate: { label: "⚠", className: "status-pill medium" },
  NeedsReview: { label: "✗", className: "status-pill high" },
  UnmatchedMdl: { label: "⬜ MDL", className: "status-pill low" },
  UnmatchedCn: { label: "⬜ CN", className: "status-pill low" },
};

const FILTER_TABS: { key: MatchStatus | "All"; label: string }[] = [
  { key: "All", label: "全て" },
  { key: "AutoMatched", label: "✓ 自動" },
  { key: "Candidate", label: "⚠ 候補" },
  { key: "NeedsReview", label: "✗ 要確認" },
  { key: "UnmatchedMdl", label: "⬜ MDL" },
  { key: "UnmatchedCn", label: "⬜ CN" },
];

export default function MergedCharacterTable({
  characters,
  filterStatus,
  onFilterChange,
  onUpdateCharacter,
}: Props) {
  const filtered =
    filterStatus === "All"
      ? characters
      : characters.filter((c) => c.match_status === filterStatus);

  const handleJapaneseChange = (globalIndex: number, value: string) => {
    onUpdateCharacter(globalIndex, {
      japanese_name: {
        value,
        source: "User",
        user_edited: true,
        locked: true,
      },
    });
  };

  const formatSource = (c: MergedCharacter): string => {
    const parts: string[] = [];
    if (c.source_ids.mydramalist) parts.push("MDL");
    if (c.source_ids.tvmao) parts.push("TV");
    if (c.source_ids.douban) parts.push("DB");
    if (c.source_ids.other) parts.push("Other");
    return parts.join("+") || "—";
  };

  if (characters.length === 0) {
    return (
      <div className="card" style={{ textAlign: "center", color: "var(--text-muted)", padding: 32 }}>
        マージ結果がありません。各ソースからキャストを取得し、「マージ」をクリックしてください。
      </div>
    );
  }

  return (
    <div className="card">
      {/* Filter tabs */}
      <div style={{ display: "flex", gap: 4, marginBottom: 12, flexWrap: "wrap" }}>
        {FILTER_TABS.map((tab) => {
          const count =
            tab.key === "All"
              ? characters.length
              : characters.filter((c) => c.match_status === tab.key).length;
          return (
            <button
              key={tab.key}
              className={`btn ${filterStatus === tab.key ? "btn-primary" : "btn-secondary"}`}
              style={{ fontSize: 12, padding: "3px 10px", minHeight: 24 }}
              onClick={() => onFilterChange(tab.key)}
            >
              {tab.label}
              <span style={{ opacity: 0.7, marginLeft: 2 }}>({count})</span>
            </button>
          );
        })}
      </div>

      {/* Table */}
      <div className="table-container">
        <table>
          <thead>
            <tr>
              <th style={{ width: 50 }}>状態</th>
              <th style={{ width: 130 }}>English</th>
              <th style={{ width: 100 }}>中文</th>
              <th style={{ width: 110 }}>日本語</th>
              <th style={{ width: 110 }}>Actor</th>
              <th style={{ width: 60 }}>Role</th>
              <th style={{ width: 55 }}>信頼度</th>
              <th style={{ width: 65 }}>Source</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((c) => {
              const globalIndex = characters.indexOf(c);
              const statusCfg = STATUS_CONFIG[c.match_status];
              return (
                <tr key={globalIndex}>
                  <td>
                    <span className={statusCfg.className}>{statusCfg.label}</span>
                  </td>
                  <td>{c.english_name.value ?? "—"}</td>
                  <td>{c.chinese_name.value ?? "—"}</td>
                  <td>
                    <input
                      className="form-input"
                      value={c.japanese_name.value}
                      onChange={(e) => handleJapaneseChange(globalIndex, e.target.value)}
                      placeholder="日本語名..."
                      style={{ width: "100%" }}
                    />
                  </td>
                  <td>{c.actor_name.value ?? "—"}</td>
                  <td>{c.role_type.value ?? "—"}</td>
                  <td>
                    <span
                      style={{
                        color:
                          c.confidence >= 0.85
                            ? "var(--success)"
                            : c.confidence >= 0.6
                              ? "var(--warning)"
                              : c.confidence >= 0.3
                                ? "var(--error)"
                                : "var(--text-muted)",
                        fontWeight: 600,
                        fontSize: 12,
                      }}
                    >
                      {c.confidence.toFixed(2)}
                    </span>
                  </td>
                  <td style={{ fontSize: 11 }}>{formatSource(c)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
