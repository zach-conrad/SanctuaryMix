import { AlertTriangle, Info, OctagonAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { computeInsights, type Insight } from "../lib/insights";
import { useMeters, useMixer } from "../store/mixer";

const ICONS = { alert: OctagonAlert, warn: AlertTriangle, info: Info };

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

export function InsightList({ insights, limit }: { insights: Insight[]; limit?: number }) {
  if (insights.length === 0) {
    return <p className="empty">Everything looks healthy. Observations will appear here during the service.</p>;
  }
  return (
    <ul className="insights">
      {insights.slice(0, limit).map((i) => {
        const Icon = ICONS[i.severity];
        return (
          <li key={i.id} className={`insight insight-${i.severity}`}>
            <Icon size={16} />
            <div>
              <strong>{i.title}</strong>
              <p>{i.detail}</p>
            </div>
          </li>
        );
      })}
    </ul>
  );
}
