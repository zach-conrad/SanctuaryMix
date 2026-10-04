import { formatDb } from "../lib/levels";
import { useMixer } from "../store/mixer";
import { InsightList, useInsights } from "./InsightList";
import { MuteKey } from "./MuteKey";

/** Right panel: the selected channel's detail, then Assist. */
export function Inspector() {
  const strip = useMixer((s) => s.strips[s.selected]);
  const consoleOn = useMixer((s) => s.consoleStatus === "on");
  const insights = useInsights();
  return (
    <aside className="inspector" aria-label="Inspector">
      {strip && (
        <section className="inspector-section">
          <span className="text-label muted">Ch {strip.index + 1}</span>
          <h2 className="text-heading">{strip.name}</h2>
          <div className="text-readout-lg">
            {formatDb(strip.faderDb)} <span className="text-caption muted">dB</span>
          </div>
          <MuteKey index={strip.index} disabled={!consoleOn} large />
        </section>
      )}
      <section className="inspector-section">
        <h2 className="text-heading">Assist</h2>
        <InsightList insights={insights} limit={6} />
      </section>
    </aside>
  );
}
