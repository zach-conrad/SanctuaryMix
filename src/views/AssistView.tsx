import { Ear, Gauge, Waves } from "lucide-react";
import { ActivityLog, AutoMixControls, ChannelPicker, RoomFeelPicker, RulesList } from "../components/AutoMix";
import { InsightList, useInsights } from "../components/InsightList";
import { PageHeader } from "../components/PageHeader";

const MODES = [
  {
    icon: Gauge,
    title: "Gain staging",
    body: "Recommends preamp gain for every input during soundcheck.",
  },
  {
    icon: Ear,
    title: "Feedback watch",
    body: "Spots ringing on open mics and suggests a narrow notch.",
  },
  {
    icon: Waves,
    title: "EQ suggestions",
    body: "Proposes gentle EQ moves you apply with one tap.",
  },
];

export function AssistView() {
  const insights = useInsights();
  return (
    <div className="page">
      <PageHeader title="Assist" />
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
    </div>
  );
}
