import { AudioLines, CircleUser, Router } from "lucide-react";
import { useMixer } from "../store/mixer";

type Tone = "ok" | "busy" | "bad" | "idle";

function Pill({ tone, icon, label, onClick }: { tone: Tone; icon: React.ReactNode; label: string; onClick?: () => void }) {
  return (
    <button className={`pill pill-${tone}`} onClick={onClick}>
      <span className="pill-dot" />
      {icon}
      <span>{label}</span>
    </button>
  );
}

const toneOf = (s: "off" | "connecting" | "on" | "error"): Tone =>
  s === "on" ? "ok" : s === "connecting" ? "busy" : s === "error" ? "bad" : "idle";

export function TopBar() {
  const { view, audioStatus, audio, consoleStatus, consoleModel, session, isDemo, setView } = useMixer();
  const titles = { mix: "Mix", assistant: "Assistant", setup: "Setup", settings: "Settings" };

  const audioLabel =
    audioStatus === "on" && audio ? `${audio.deviceName} · ${audio.channels} ch` : audioStatus === "connecting" ? "Opening audio…" : "No audio input";
  const consoleLabel =
    consoleStatus === "on" ? consoleModel ?? "Console connected" : consoleStatus === "connecting" ? "Connecting…" : consoleStatus === "error" ? "Console offline" : "No console";

  return (
    <header className="topbar" data-tauri-drag-region>
      <h1 data-tauri-drag-region>{titles[view]}</h1>
      {isDemo && <span className="demo-badge" title="Running in a browser with simulated audio and console">Demo mode</span>}
      <div className="topbar-spacer" data-tauri-drag-region />
      <Pill tone={toneOf(audioStatus)} icon={<AudioLines size={14} />} label={audioLabel} onClick={() => setView("setup")} />
      <Pill tone={toneOf(consoleStatus)} icon={<Router size={14} />} label={consoleLabel} onClick={() => setView("setup")} />
      <button className="user-chip" onClick={() => setView("settings")}>
        <CircleUser size={18} strokeWidth={1.75} />
        <span>{session?.user.displayName ?? "…"}</span>
      </button>
    </header>
  );
}
