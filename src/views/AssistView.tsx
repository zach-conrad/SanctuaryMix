import { Ear, Gauge, MessageSquareText, Waves } from "lucide-react";
import { InsightList, useInsights } from "../components/InsightList";

const MODES = [
  { icon: Gauge, title: "Gain staging", body: "Listens during soundcheck and recommends preamp gain for every input so nothing clips and nothing hides in the noise." },
  { icon: Ear, title: "Feedback watch", body: "Spots ringing frequencies on open mics before the room hears them and suggests a narrow notch." },
  { icon: Waves, title: "EQ suggestions", body: "Compares each voice and instrument to a reference and proposes gentle EQ moves you apply with one tap." },
  { icon: MessageSquareText, title: "Speech and music balance", body: "Keeps the pastor's mic clear over the band and rides levels between sermon, worship and video." },
];

export function AssistView() {
  const insights = useInsights();
  return (
    <div className="page">
      <section className="panel">
        <h2 className="text-heading">Right now</h2>
        <p className="muted">Assist checks every input while you mix. Select a note to jump to that channel.</p>
        <InsightList insights={insights} />
      </section>
      <section>
        <h2 className="text-heading section-title">Auto-mixing</h2>
        <div className="mode-grid">
          {MODES.map(({ icon: Icon, title, body }) => (
            <div key={title} className="panel">
              <div className="panel-head">
                <Icon size={20} strokeWidth={1.75} />
                <h3 className="text-body-strong">{title}</h3>
                <span className="text-label muted">Coming soon</span>
              </div>
              <p className="muted">{body}</p>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
