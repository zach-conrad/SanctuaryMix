import { Cable, Layers, Settings, SlidersVertical, Sparkles } from "lucide-react";
import { useMixer, type View } from "../store/mixer";

const ITEMS: { view: View; label: string; icon: typeof Cable }[] = [
  { view: "mixer", label: "Mixer", icon: SlidersVertical },
  { view: "scenes", label: "Scenes", icon: Layers },
  { view: "assist", label: "Assist", icon: Sparkles },
  { view: "setup", label: "Setup", icon: Cable },
];

function NavItem({ view, label, icon: Icon }: (typeof ITEMS)[number]) {
  const current = useMixer((s) => s.view);
  const setView = useMixer((s) => s.setView);
  return (
    <button
      className={`nav-item ${current === view ? "active" : ""}`}
      onClick={() => setView(view)}
      aria-current={current === view ? "page" : undefined}
    >
      <Icon size={20} strokeWidth={1.75} />
      <span>{label}</span>
    </button>
  );
}

export function Sidebar() {
  return (
    <nav className="rail" aria-label="Main">
      {ITEMS.map((item) => (
        <NavItem key={item.view} {...item} />
      ))}
      <div className="rail-spacer" />
      <NavItem view="settings" label="Settings" icon={Settings} />
    </nav>
  );
}
