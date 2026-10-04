import { meterPosition } from "../lib/levels";
import { useMeters } from "../store/mixer";

export function Meter({ channel }: { channel: number }) {
  const m = useMeters((s) => s.frame?.channels[channel]);
  const rms = m ? meterPosition(m.rmsDb) : 0;
  const peak = m ? meterPosition(m.peakDb) : 0;
  return (
    <div className={`meter ${m ? "" : "meter-off"}`} aria-hidden>
      <div className={`meter-clip ${m?.clipped ? "on" : ""}`} />
      <div className="meter-track">
        <div className="meter-rms" style={{ clipPath: `inset(${(1 - rms) * 100}% 0 0 0)` }} />
        <div className="meter-peak" style={{ bottom: `${peak * 100}%`, opacity: peak > 0 ? 1 : 0 }} />
      </div>
    </div>
  );
}
