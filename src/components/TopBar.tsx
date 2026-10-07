import { Hand, Play } from "lucide-react";
import { useEffect, useState } from "react";
import { useAiEq } from "../store/aieq";
import { useAutoMix } from "../store/automix";
import { useMixer, type Theme } from "../store/mixer";
import { RecordControl } from "./RecordControl";
import { StatusPill, type PillTone } from "./StatusPill";

const tone = (s: "off" | "connecting" | "on" | "error"): PillTone =>
  s === "on" ? "ok" : s === "connecting" ? "warn" : s === "error" ? "error" : "neutral";

/** Which wordmark to show: the dark-ground file on dark themes. */
export function useResolvedTheme(theme: Theme): "dark" | "light" {
  const query = "(prefers-color-scheme: light)";
  const [osLight, setOsLight] = useState(() => window.matchMedia?.(query).matches ?? false);
  useEffect(() => {
    const mq = window.matchMedia?.(query);
    const on = (e: MediaQueryListEvent) => setOsLight(e.matches);
    mq?.addEventListener("change", on);
    return () => mq?.removeEventListener("change", on);
  }, []);
  return theme === "system" ? (osLight ? "light" : "dark") : theme;
}

export function TopBar() {
  const { audioStatus, audio, consoleStatus, consoleModel, consoleConfig, isDemo, unconfirmed, theme, setView } =
    useMixer();
  const resolved = useResolvedTheme(theme);

  const consoleMeta =
    consoleStatus === "on"
      ? "Connected"
      : consoleStatus === "connecting"
        ? "Connecting"
        : consoleStatus === "error"
          ? "Can't reach it"
          : "Not connected";
  const danteMeta =
    audioStatus === "on" && audio
      ? `${audio.channels} ch · ${audio.sampleRate / 1000} kHz`
      : audioStatus === "connecting"
        ? "Opening"
        : audioStatus === "error"
          ? "No audio"
          : "Not listening";

  return (
    <header className="topbar" data-tauri-drag-region>
      <img
        className="wordmark"
        src={`/brand/wordmark-${resolved}.svg`}
        alt="SanctuaryMix"
        data-tauri-drag-region
      />
      <div className="topbar-pills">
        <StatusPill
          tone={tone(consoleStatus)}
          subject={consoleModel ?? (consoleConfig.model === "dlive" ? "dLive" : "Console")}
          meta={consoleMeta}
          onClick={() => setView("setup")}
        />
        <StatusPill tone={tone(audioStatus)} subject="Dante" meta={danteMeta} onClick={() => setView("setup")} />
        {unconfirmed && <StatusPill tone="warn" subject="Console" meta="Hasn't confirmed a change" />}
        {isDemo && <StatusPill tone="warn" subject="Demo" meta="Simulated" />}
        <AssistMode />
      </div>
      <div className="topbar-spacer" data-tauri-drag-region />
      <div className="topbar-actions">
        <FreezeButton />
        <RecordControl />
      </div>
    </header>
  );
}

/** The Assist or auto-mix badge, at the end of the status pills. */
function AssistMode() {
  const engaged = useAutoMix((s) => s.status?.engaged ?? false);
  const frozen = useAutoMix((s) => s.status?.frozen ?? false);
  const setView = useMixer((s) => s.setView);
  if (!engaged) {
    return (
      <button
        className="sm-assist-badge pill-button"
        title="Auto-mix is off. Turn it on in Assist."
        onClick={() => setView("assist")}
      >
        Assist · Suggest
      </button>
    );
  }
  return (
    <button className="sm-assist-badge pill-button" onClick={() => setView("assist")}>
      {frozen ? "Auto-mix · Frozen" : "Auto-mix · On"}
    </button>
  );
}

/** While auto-mix or AI EQ runs, Freeze (or Resume) sits right beside Record service, always one tap away. */
function FreezeButton() {
  const engaged = useAutoMix((s) => s.status?.engaged ?? false);
  const autoFrozen = useAutoMix((s) => s.status?.frozen ?? false);
  const eqOn = useAiEq((s) => s.status?.enabled ?? false);
  const eqFrozen = useAiEq((s) => s.status?.frozen ?? false);
  const { freeze, resume } = useAutoMix();
  if (!engaged && !eqOn) return null;
  // One freeze stops both: the core's freeze covers levels and EQ.
  const frozen = (engaged && autoFrozen) || (!engaged && eqFrozen);
  return frozen ? (
    <button className="sm-btn topbar-freeze" onClick={() => void resume()} title="Let AI move levels and EQ again">
      <Play />
      Resume
    </button>
  ) : (
    <button className="sm-btn topbar-freeze" onClick={() => void freeze()} title="Stop every AI move, levels and EQ (Esc)">
      <Hand />
      Freeze
    </button>
  );
}
