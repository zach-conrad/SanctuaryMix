import { Layers } from "lucide-react";
import { PageHeader } from "../components/PageHeader";

export function ScenesView() {
  return (
    <div className="page page--narrow">
      <PageHeader title="Scenes">Recall and save the console's scenes for each part of the service.</PageHeader>
      <section className="panel">
        <div className="panel-head">
          <Layers size={20} strokeWidth={1.75} />
          <h2 className="text-heading">Scenes</h2>
          <span className="text-label muted">Coming soon</span>
        </div>
        <p className="muted">
          Recall and save dLive scenes for each part of the service, like "Sunday 9am" or "Youth night", with Undo after
          every recall.
        </p>
      </section>
    </div>
  );
}
