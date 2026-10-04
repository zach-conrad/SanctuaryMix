import { Layers } from "lucide-react";
import { PageHeader } from "../components/PageHeader";

export function ScenesView() {
  return (
    <div className="page page--narrow">
      <PageHeader title="Scenes" />
      <section className="panel">
        <div className="panel-head">
          <Layers size={20} strokeWidth={1.75} />
          <h2 className="text-heading">Scenes</h2>
          <span className="text-label muted">Coming soon</span>
        </div>
        <p className="muted">Recall and save dLive scenes, with Undo.</p>
      </section>
    </div>
  );
}
