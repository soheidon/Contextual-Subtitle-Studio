import { useState } from "react";
import { AlertTriangle, Check, Info, X } from "lucide-react";
import type { QualityReport, DuplicateInfo } from "../../types";

interface Props {
  report: QualityReport;
  onFilterChange: (filter: "All" | "high" | "medium" | "low") => void;
  currentFilter: "All" | "high" | "medium" | "low";
}

export default function CharacterDictQuality({ report, onFilterChange, currentFilter }: Props) {
  const { total_entries, missing_actor_cn, missing_actor_en, missing_role_cn, missing_role_en, missing_role_jp_kana, confidence_breakdown, duplicates } = report;

  const totalMissing = missing_actor_cn + missing_actor_en + missing_role_cn + missing_role_en + missing_role_jp_kana;
  const hasIssues = totalMissing > 0 || duplicates.length > 0;

  return (
    <div className="card" style={{ marginBottom: 16 }}>
      <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 12, display: "flex", alignItems: "center", gap: 8 }}>
        {hasIssues ? <AlertTriangle size={16} color="var(--warning)" /> : <Check size={16} color="var(--success)" />}
        品質チェック ({total_entries}件)
      </h3>

      {/* Missing fields */}
      <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginBottom: 12 }}>
        <StatBadge label="俳優(中)" missing={missing_actor_cn} total={total_entries} />
        <StatBadge label="俳優(英)" missing={missing_actor_en} total={total_entries} />
        <StatBadge label="役名(中)" missing={missing_role_cn} total={total_entries} />
        <StatBadge label="役名(英)" missing={missing_role_en} total={total_entries} />
        <StatBadge label="日本語" missing={missing_role_jp_kana} total={total_entries} />
      </div>

      {/* Confidence breakdown */}
      <div style={{ marginBottom: 12 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
          <span style={{ fontSize: 12, color: "var(--text-secondary)" }}>信頼度分布:</span>
          <div style={{ display: "flex", gap: 2, height: 20, flex: 1, borderRadius: 4, overflow: "hidden" }}>
            {confidence_breakdown.high > 0 && (
              <div
                style={{
                  flex: confidence_breakdown.high,
                  backgroundColor: "var(--success)",
                  minWidth: 4,
                }}
                title={`高信頼: ${confidence_breakdown.high}件`}
              />
            )}
            {confidence_breakdown.medium > 0 && (
              <div
                style={{
                  flex: confidence_breakdown.medium,
                  backgroundColor: "var(--warning)",
                  minWidth: 4,
                }}
                title={`中信頼: ${confidence_breakdown.medium}件`}
              />
            )}
            {confidence_breakdown.low > 0 && (
              <div
                style={{
                  flex: confidence_breakdown.low,
                  backgroundColor: "var(--error)",
                  minWidth: 4,
                }}
                title={`低信頼: ${confidence_breakdown.low}件`}
              />
            )}
          </div>
        </div>
        <div style={{ display: "flex", gap: 16, fontSize: 11, color: "var(--text-muted)" }}>
          <span>🟢 高信頼 (≧0.85): {confidence_breakdown.high}件</span>
          <span>🟡 中信頼 (0.60-0.84): {confidence_breakdown.medium}件</span>
          <span>🔴 低信頼 (&lt;0.60): {confidence_breakdown.low}件</span>
        </div>
      </div>

      {/* Filter tabs */}
      <div style={{ display: "flex", gap: 4, marginBottom: 12 }}>
        {(["All", "high", "medium", "low"] as const).map((f) => (
          <button
            key={f}
            className={`btn ${currentFilter === f ? "btn-primary" : "btn-secondary"}`}
            onClick={() => onFilterChange(f)}
            style={{ fontSize: 11, padding: "2px 10px" }}
          >
            {f === "All" ? "全て" : f === "high" ? "🟢 高信頼" : f === "medium" ? "🟡 中信頼" : "🔴 低信頼"}
            {f !== "All" && (
              <span style={{ marginLeft: 4, opacity: 0.7 }}>
                ({f === "high" ? confidence_breakdown.high : f === "medium" ? confidence_breakdown.medium : confidence_breakdown.low})
              </span>
            )}
          </button>
        ))}
      </div>

      {/* Duplicates */}
      {duplicates.length > 0 && (
        <div>
          <h4 style={{ fontSize: 13, fontWeight: 600, marginBottom: 8, color: "var(--warning)" }}>
            ⚠ 重複検出 ({duplicates.length}件)
          </h4>
          <div style={{ maxHeight: 200, overflowY: "auto", fontSize: 12 }}>
            {duplicates.map((d, i) => (
              <DuplicateCard key={i} dup={d} />
            ))}
          </div>
        </div>
      )}

      {!hasIssues && (
        <p style={{ fontSize: 12, color: "var(--success)" }}>
          すべての項目が正常です。問題は見つかりませんでした。
        </p>
      )}
    </div>
  );
}

function StatBadge({ label, missing, total }: { label: string; missing: number; total: number }) {
  const ok = total - missing;
  const hasMissing = missing > 0;
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 4,
        padding: "2px 8px",
        borderRadius: 4,
        backgroundColor: hasMissing ? "var(--error-bg)" : "var(--success-bg)",
        fontSize: 11,
      }}
    >
      {hasMissing ? <X size={12} color="var(--error)" /> : <Check size={12} color="var(--success)" />}
      <span style={{ color: "var(--text-secondary)" }}>{label}:</span>
      <strong style={{ color: hasMissing ? "var(--error)" : "var(--success)" }}>
        {ok}/{total}
      </strong>
    </div>
  );
}

function DuplicateCard({ dup }: { dup: DuplicateInfo }) {
  const [expanded, setExpanded] = useState(false);
  return (
    <div style={{ marginBottom: 4, border: "1px solid var(--border)", borderRadius: 4, padding: "4px 8px" }}>
      <div
        style={{ display: "flex", alignItems: "center", gap: 6, cursor: "pointer" }}
        onClick={() => setExpanded(!expanded)}
      >
        <Info size={12} color="var(--warning)" />
        <code style={{ fontSize: 11 }}>{dup.field}</code>
        <span style={{ fontSize: 11, fontWeight: 600 }}>"{dup.value}"</span>
        <span style={{ fontSize: 10, color: "var(--text-muted)" }}>×{dup.keys.length}</span>
        <span style={{ marginLeft: "auto", fontSize: 10, color: "var(--text-muted)" }}>
          {expanded ? "▲" : "▼"}
        </span>
      </div>
      {expanded && (
        <div style={{ marginTop: 4, fontSize: 11, color: "var(--text-secondary)" }}>
          {dup.keys.map((k) => (
            <code key={k} style={{ display: "inline-block", margin: "1px 4px", padding: "1px 4px", backgroundColor: "var(--bg-secondary)", borderRadius: 2 }}>
              {k}
            </code>
          ))}
        </div>
      )}
    </div>
  );
}
