import { useState, useRef, useEffect, useMemo, useCallback } from "react";
import { Trash2, ChevronDown, ChevronUp, ClipboardCopy } from "lucide-react";
import { useAppLogStore, type LogLevel } from "../../stores/useAppLogStore";

const LEVEL_COLORS: Record<LogLevel, { bg: string; text: string; label: string }> = {
  success: { bg: "rgba(16,124,16,0.12)", text: "var(--success)", label: "OK" },
  error: { bg: "var(--error-bg)", text: "var(--error)", label: "ERR" },
  warning: { bg: "rgba(178,107,0,0.1)", text: "var(--warning)", label: "WARN" },
  info: { bg: "rgba(0,120,212,0.1)", text: "var(--accent)", label: "INFO" },
  debug: { bg: "var(--bg-table-head)", text: "var(--text-muted)", label: "DBG" },
};

const MIN_HEIGHT = 100;
const MAX_HEIGHT = window.innerHeight - 200;
const DEFAULT_HEIGHT = 200;

export default function AppLogPanel() {
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [sourceFilter, setSourceFilter] = useState<string>("all");
  const [panelHeight, setPanelHeight] = useState(DEFAULT_HEIGHT);
  const [isResizing, setIsResizing] = useState(false);

  const scrollRef = useRef<HTMLDivElement>(null);
  const resizeRef = useRef<HTMLDivElement>(null);
  const isDragging = useRef(false);
  const startY = useRef(0);
  const startHeight = useRef(DEFAULT_HEIGHT);

  const logs = useAppLogStore((s) => s.logs);
  const clearLogs = useAppLogStore((s) => s.clearLogs);

  const filteredLogs = useMemo(() => {
    if (sourceFilter === "all") return logs;
    return logs.filter((e) => e.source === sourceFilter);
  }, [logs, sourceFilter]);

  const copyLogs = useCallback(() => {
    const text = filteredLogs
      .map((e) => {
        const time = new Date(e.timestamp).toLocaleTimeString();
        const level = LEVEL_COLORS[e.level].label;
        return `[${time}] [${level}] [${e.source}] ${e.message}`;
      })
      .join("\n");
    navigator.clipboard.writeText(text).catch(console.error);
  }, [filteredLogs]);

  const uniqueSources = useMemo(() => {
    return [...new Set(logs.map((e) => e.source))].sort();
  }, [logs]);

  useEffect(() => {
    if (scrollRef.current && !isCollapsed && filteredLogs.length > 0) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [filteredLogs, isCollapsed]);

  const handleResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    isDragging.current = true;
    startY.current = e.clientY;
    startHeight.current = panelHeight;
    setIsResizing(true);
  }, [panelHeight]);

  useEffect(() => {
    if (!isResizing) return;

    const handleMouseMove = (e: MouseEvent) => {
      if (!isDragging.current) return;
      const delta = startY.current - e.clientY;
      const newHeight = Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, startHeight.current + delta));
      setPanelHeight(newHeight);
    };

    const handleMouseUp = () => {
      isDragging.current = false;
      setIsResizing(false);
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);

    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [isResizing]);

  return (
    <div
      className={`app-log-panel${isCollapsed ? " collapsed" : ""}${isResizing ? " resizing" : ""}`}
      style={{ height: isCollapsed ? "auto" : panelHeight }}
    >
      <div
        ref={resizeRef}
        className="app-log-panel-resize-handle"
        onMouseDown={handleResizeStart}
      />

      <div className="app-log-panel-toolbar">
        <span className="app-log-panel-title">
          ログ{logs.length > 0 ? ` (${logs.length}件)` : ""}
        </span>
        <select
          value={sourceFilter}
          onChange={(e) => setSourceFilter(e.target.value)}
        >
          <option value="all">すべてのソース</option>
          {uniqueSources.map((s) => (
            <option key={s} value={s}>{s}</option>
          ))}
        </select>
        <button
          className="btn btn-secondary"
          onClick={copyLogs}
          style={{ fontSize: 11, padding: "1px 8px" }}
          title="ログをクリップボードにコピー"
        >
          <ClipboardCopy size={12} style={{ marginRight: 3, verticalAlign: -2 }} />
          コピー
        </button>
        <button
          className="btn btn-secondary"
          onClick={clearLogs}
          style={{ fontSize: 11, padding: "1px 8px" }}
        >
          <Trash2 size={12} style={{ marginRight: 3, verticalAlign: -2 }} />
          クリア
        </button>
        <button
          className="btn btn-secondary"
          onClick={() => setIsCollapsed(!isCollapsed)}
          style={{ fontSize: 11, padding: "1px 6px" }}
        >
          {isCollapsed ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
        </button>
      </div>

      {!isCollapsed && (
        <div className="app-log-panel-entries" ref={scrollRef}>
          {filteredLogs.length === 0 ? (
            <div className="app-log-panel-empty">ログはまだありません</div>
          ) : (
            filteredLogs.map((entry) => {
              const c = LEVEL_COLORS[entry.level];
              return (
                <div key={entry.id} className="app-log-panel-entry">
                  <span className="app-log-panel-timestamp">
                    {new Date(entry.timestamp).toLocaleTimeString()}
                  </span>
                  <span
                    className="app-log-panel-level"
                    style={{ background: c.bg, color: c.text }}
                  >
                    {c.label}
                  </span>
                  <span className="app-log-panel-source">{entry.source}</span>
                  <span className="app-log-panel-message">{entry.message}</span>
                </div>
              );
            })
          )}
        </div>
      )}
    </div>
  );
}
