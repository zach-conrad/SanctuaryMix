import { Cable, Settings, SlidersVertical, Sparkles } from "lucide-react";
import { useMixer, type View } from "../store/mixer";

const ITEMS: { view: View; label: string; icon: typeof Cable }[] = [
  { view: "mix", label: "Mix", icon: SlidersVertical },
  { view: "assistant", label: "Assistant", icon: Sparkles },
  { view: "setup", label: "Setup", icon: Cable },
  { view: "settings", label: "Settings", icon: Settings },
];

export function Sidebar() {
  const view = useMixer((s) => s.view);
  const setView = useMixer((s) => s.setView);
  return (
    <nav className="sidebar" aria-label="Main">
      <div className="sidebar-logo" data-tauri-drag-region>
        <img src="/icon.png" alt="" width={36} height={36} />
      </div>
      {ITEMS.map(({ view: v, label, icon: Icon }) => (
        <button
          key={v}
          className={`nav-item ${view === v ? "active" : ""}`}
          onClick={() => setView(v)}
          aria-current={view === v ? "page" : undefined}
        >
          <Icon size={20} strokeWidth={1.75} />
          <span>{label}</span>
        </button>
      ))}
    </nav>
  );
}
