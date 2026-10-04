import { useEffect, useState } from "react";
import { computeInsights, type Insight } from "../lib/insights";
import { useMeters, useMixer } from "../store/mixer";

/** Re-evaluated twice a second so cards don't flicker with every meter frame. */
export function useInsights(): Insight[] {
  const strips = useMixer((s) => s.strips);
  const [insights, setInsights] = useState<Insight[]>([]);
  useEffect(() => {
    const tick = () => setInsights(computeInsights(useMeters.getState().frame, strips));
    tick();
    const id = setInterval(tick, 500);
    return () => clearInterval(id);
  }, [strips]);
  return insights;
}

const SEVERITY_WORD = { alert: "Clipping", warn: "Hot", info: "No signal" };

/** Assist observations. Read-only today; Apply/Undo arrive with auto-mixing. */
export function InsightList({ insights, limit }: { insights: Insight[]; limit?: number }) {
  const select = useMixer((s) => s.select);
  if (insights.length === 0) {
    return <p className="empty">Everything sounds healthy. Assist will note anything that needs a look.</p>;
  }
  return (
    <ul className="insights">
      {insights.slice(0, limit).map((i) => (
        <li key={i.id}>
          <button className="sm-assist insight" onClick={() => select(i.channel)}>
            <span className="sm-assist__head">
              <span className="sm-assist-badge">Assist</span>
              <span className="sm-assist__time">
                Ch {i.channel + 1} · {SEVERITY_WORD[i.severity]}
              </span>
            </span>
            <span className="sm-assist__title">{i.title}</span>
            <span className="sm-assist__why">{i.detail}</span>
          </button>
        </li>
      ))}
    </ul>
  );
}
