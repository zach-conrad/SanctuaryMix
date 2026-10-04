import { useEffect, useRef } from "react";
import { meterPosition } from "../lib/levels";
import { useMeters } from "../store/mixer";

const PEAK_HOLD_MS = 1500;

/**
 * dBFS meter. Levels are written straight to CSS variables on every animation
 * frame (no React state, no easing) so 32+ meters stay cheap and honest.
 * The clip latch holds until the meter is clicked.
 */
export function Meter({ channel }: { channel: number }) {
  const el = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let raf = 0;
    let hold = 0;
    let heldAt = 0;
    const tick = (now: number) => {
      const node = el.current;
      const m = useMeters.getState().frame?.channels[channel];
      if (node) {
        const level = m ? meterPosition(m.rmsDb) : 0;
        const peak = m ? meterPosition(m.peakDb) : 0;
        if (peak >= hold || now - heldAt > PEAK_HOLD_MS) {
          hold = peak;
          heldAt = now;
        }
        node.style.setProperty("--level", level.toFixed(4));
        node.style.setProperty("--peak", hold.toFixed(4));
        node.style.setProperty("--peak-display", hold > 0 ? "block" : "none");
        if (m?.clipped) node.dataset.clipped = "true";
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [channel]);

  return (
    <div
      ref={el}
      className="sm-meter"
      title="Click to clear the clip light"
      onClick={() => el.current && delete el.current.dataset.clipped}
    >
      <div className="sm-meter__clip" />
      <div className="sm-meter__fill" />
      <div className="sm-meter__peak" />
    </div>
  );
}
