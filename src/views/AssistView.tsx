import { Gauge } from "lucide-react";
import { AssistEqTab } from "../components/AssistEqTab";
import { ActivityLog, AutoMixControls, ChannelPicker, RoomFeelPicker, RulesList } from "../components/AutoMix";
import { InsightList, useInsights } from "../components/InsightList";
import { PageHeader } from "../components/PageHeader";
import { useAiEq } from "../store/aieq";

const MODES = [
  {
    icon: Gauge,
    title: "Gain staging",
    body: "Recommends preamp gain for every input during soundcheck.",
  },
];

export function AssistView() {
  const tab = useAiEq((s) => s.assistTab);
  const setTab = useAiEq((s) => s.setAssistTab);
  return (
    <div className="page">
      <PageHeader
        title="Assist"
        actions={
          <div className="sm-seg" role="group" aria-label="Assist area">
            <button aria-pressed={tab === "levels"} onClick={() => setTab("levels")}>
              Levels
            </button>
            <button aria-pressed={tab === "eq"} onClick={() => setTab("eq")}>
              EQ
            </button>
          </div>
        }
      />
      {tab === "levels" ? <LevelsTab /> : <AssistEqTab />}
    </div>
  );
}

/** Today's auto-mix page, unchanged. */
function LevelsTab() {
  const insights = useInsights();
  return (
    <>
      <AutoMixControls />
      <div className="page-split">
        <div className="page-main">
          <ChannelPicker />
        </div>
        <aside className="page-side">
          <RoomFeelPicker />
          <section className="section" aria-labelledby="now-title">
            <div className="section-head">
              <h2 id="now-title">Right now</h2>
            </div>
            <InsightList insights={insights} />
          </section>
          <ActivityLog />
          <RulesList />
          <section className="section" aria-labelledby="coming-title">
            <div className="section-head">
              <h2 id="coming-title">Coming next</h2>
            </div>
            <ul className="group coming-list">
              {MODES.map(({ icon: Icon, title, body }) => (
                <li key={title} className="row" title={body}>
                  <Icon size={20} strokeWidth={1.75} aria-hidden />
                  <span className="row-text">{title}</span>
                </li>
              ))}
            </ul>
          </section>
        </aside>
      </div>
    </>
  );
}
