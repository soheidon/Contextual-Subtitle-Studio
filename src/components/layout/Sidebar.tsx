import { NavLink, useNavigate } from "react-router-dom";
import { FileText, Users, Cpu, Play, AlertCircle, Settings } from "lucide-react";
import { useSrtStore } from "../../stores/useSrtStore";

const navItems = [
  { to: "/", icon: Users, label: "ドラマ情報統合", exact: true },
  { to: "/srt", icon: FileText, label: "SRT", exact: undefined },
  { to: "/dictionaries", icon: Users, label: "辞書", exact: undefined },
  { to: "/llm", icon: Cpu, label: "LLM設定", exact: undefined },
  { to: "/translate", icon: Play, label: "翻訳", exact: undefined },
  { to: "/review", icon: AlertCircle, label: "レビュー", exact: undefined },
];

export default function Sidebar() {
  const isLoaded = useSrtStore((s) => s.isLoaded);
  const navigate = useNavigate();

  return (
    <aside className="sidebar">
      <nav className="sidebar-nav">
        {navItems.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.exact}
            className={({ isActive }) =>
              `sidebar-link${isActive ? " active" : ""}${!isLoaded && item.to !== "/" ? " disabled" : ""}`
            }
          >
            <item.icon size={18} />
            {item.label}
          </NavLink>
        ))}
      </nav>
      <div style={{ marginTop: "auto", padding: "0 8px 8px" }}>
        <button
          className="sidebar-link"
          onClick={() => navigate("/settings")}
          style={{ width: "100%" }}
        >
          <Settings size={18} />
          設定
        </button>
      </div>
    </aside>
  );
}
