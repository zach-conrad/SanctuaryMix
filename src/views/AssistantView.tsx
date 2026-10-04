import { Ear, Gauge, MessageSquareText, Waves } from "lucide-react";
import { InsightList, useInsights } from "../components/InsightList";

const MODES = [
  { icon: Gauge, title: "Gain staging", body: "Listens during soundcheck and recommends preamp gain for every input so nothing clips and nothing hides in the noise." },
  { icon: Ear, title: "Feedback watch", body: "Spots ringing frequencies on open mics before the congregation hears them and suggests a narrow notch." },
  { icon: Waves, title: "EQ suggestions", body: "Compares each voice and instrument to a reference profile and proposes gentle EQ moves you approve with one tap." },
  { icon: MessageSquareText, title: "Speech-to-music balance", body: "Keeps the pastor's mic intelligible over the band and rides levels between sermon, worship and video segments." },
];

export function AssistantView() {
  const insights = useInsights();
  return (
    <div className="page">
      <section className="card">
        <h2>Live observations</h2>
        <p className="muted">Checks running on every input right now. The AI engine will build on these.</p>
        <InsightList insights={insights} />
      </section>
      <section>
        <h2 className="section-title">Auto-mix</h2>
        <div className="mode-grid">
          {MODES.map(({ icon: Icon, title, body }) => (
            <div key={title} className="card mode">
              <div className="mode-head">
                <Icon size={20} />
                <h3>{title}</h3>
                <span className="soon">Coming soon</span>
              </div>
              <p>{body}</p>
              <label className="switch disabled">
                <input type="checkbox" disabled />
                <span>Enable</span>
              </label>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
