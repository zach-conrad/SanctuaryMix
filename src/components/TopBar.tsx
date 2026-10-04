import { useEffect, useState } from "react";
import { useMixer, type Theme } from "../store/mixer";
import { StatusPill, type PillTone } from "./StatusPill";

const tone = (s: "off" | "connecting" | "on" | "error"): PillTone =>
  s === "on" ? "ok" : s === "connecting" ? "warn" : s === "error" ? "error" : "neutral";

/** Which wordmark to show: the dark-ground file on dark themes. */
function useResolvedTheme(theme: Theme): "dark" | "light" {
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
        {isDemo && <StatusPill tone="warn" subject="Demo" meta="Simulated audio and console" />}
      </div>
      <div className="topbar-spacer" data-tauri-drag-region />
      <span className="sm-assist-badge" title="Assist only suggests changes. It never moves the console by itself.">
        Assist · Suggest
      </span>
    </header>
  );
}
