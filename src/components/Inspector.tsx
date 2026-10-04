import { formatDb } from "../lib/levels";
import { MODE_DETAIL, ROLE_LABEL, useAutoMix, useChannelAuto } from "../store/automix";
import { useMixer } from "../store/mixer";
import { ChannelActions, FaderChange, ModeWord } from "./AutoMix";
import { InsightList, useInsights } from "./InsightList";
import { MuteKey } from "./MuteKey";
import { SplReadout } from "./SplMeter";

/** Right panel: the selected channel's detail, then Assist. */
export function Inspector() {
  const strip = useMixer((s) => s.strips[s.selected]);
  const consoleOn = useMixer((s) => s.consoleStatus === "on");
  const insights = useInsights();
  const auto = useChannelAuto(strip?.index ?? -1);
  const undo = useAutoMix((s) => s.undo);
  const resume = useAutoMix((s) => s.resumeChannel);
  return (
    <aside className="inspector" aria-label="Inspector">
      <section className="inspector-section" aria-label="Room level">
        <SplReadout />
      </section>
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
      {auto && auto.mode !== "off" && (
        <section className="inspector-section inspector-auto" aria-label="Auto-mix on this channel">
          <span className="inspector-auto-head">
            <span className="sm-assist-badge">Auto</span>
            <span className="text-caption muted">{ROLE_LABEL[auto.role]}</span>
            <ModeWord status={auto} />
          </span>
          <span className="text-readout">
            <FaderChange status={auto} />
          </span>
          <p className="text-caption muted">{MODE_DETAIL[auto.mode]}</p>
          <ChannelActions status={auto} undo={undo} resume={resume} />
        </section>
      )}
      <section className="inspector-section">
        <h2 className="text-heading">Assist</h2>
        <InsightList insights={insights} limit={6} />
      </section>
    </aside>
  );
}
