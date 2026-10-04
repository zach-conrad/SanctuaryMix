import { memo } from "react";
import { formatDb } from "../lib/levels";
import { useMixer, type Strip } from "../store/mixer";
import { Fader } from "./Fader";
import { Meter } from "./Meter";

const HUES = [250, 200, 160, 30, 340, 280];

export const ChannelStrip = memo(function ChannelStrip({ strip, disabled }: { strip: Strip; disabled: boolean }) {
  const setFader = useMixer((s) => s.setFader);
  const toggleMute = useMixer((s) => s.toggleMute);
  const hue = HUES[Math.floor(strip.index / 4) % HUES.length];
  return (
    <div className={`strip ${strip.muted ? "muted" : ""}`} style={{ "--strip-hue": hue } as React.CSSProperties}>
      <div className="strip-number">{strip.index + 1}</div>
      <div className="strip-name" title={strip.name}>
        {strip.name}
      </div>
      <div className="strip-body">
        <Meter channel={strip.index} />
        <Fader db={strip.faderDb} disabled={disabled} label={strip.name} onChange={(db) => setFader(strip.index, db)} />
      </div>
      <div className="strip-db">{formatDb(strip.faderDb)}</div>
      <button
        className={`mute ${strip.muted ? "on" : ""}`}
        disabled={disabled}
        onClick={() => toggleMute(strip.index)}
        aria-pressed={strip.muted}
      >
        Mute
      </button>
    </div>
  );
});
