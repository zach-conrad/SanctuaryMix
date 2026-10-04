import { PanelRightClose, PanelRightOpen, Sparkles } from "lucide-react";
import { useState } from "react";
import { ChannelStrip } from "../components/ChannelStrip";
import { InsightList, useInsights } from "../components/InsightList";
import { useMixer } from "../store/mixer";

export function MixView() {
  const strips = useMixer((s) => s.strips);
  const consoleOn = useMixer((s) => s.consoleStatus === "on");
  const setView = useMixer((s) => s.setView);
  const insights = useInsights();
  const [panelOpen, setPanelOpen] = useState(true);

  return (
    <div className="mix">
      <div className="mix-main">
        {!consoleOn && (
          <div className="banner">
            Connect a console to move faders and mutes. Meters work as soon as audio is running.
            <button className="link" onClick={() => setView("setup")}>
              Open Setup
            </button>
          </div>
        )}
        <div className="strips" role="group" aria-label="Input channels">
          {strips.map((s) => (
            <ChannelStrip key={s.index} strip={s} disabled={!consoleOn} />
          ))}
        </div>
      </div>
      <aside className={`assistant-panel ${panelOpen ? "open" : ""}`}>
        <button className="panel-toggle" onClick={() => setPanelOpen(!panelOpen)} aria-label="Toggle assistant panel">
          {panelOpen ? <PanelRightClose size={18} /> : <PanelRightOpen size={18} />}
          {!panelOpen && insights.length > 0 && <span className="count">{insights.length}</span>}
        </button>
        {panelOpen && (
          <>
            <h2>
              <Sparkles size={16} /> Assistant
            </h2>
            <InsightList insights={insights} limit={8} />
            <button className="link" onClick={() => setView("assistant")}>
              See all and auto-mix options
            </button>
          </>
        )}
      </aside>
    </div>
  );
}
