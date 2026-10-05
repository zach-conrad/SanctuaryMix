import { Layers } from "lucide-react";
import { PageHeader } from "../components/PageHeader";

export function ScenesView() {
  return (
    <div className="page page--narrow">
      <PageHeader title="Scenes" />
      <div className="empty-state">
        <Layers size={40} strokeWidth={1.5} aria-hidden />
        <h2 className="text-heading">Scenes are coming</h2>
        <p className="muted">Recall and save dLive scenes, with Undo.</p>
      </div>
    </div>
  );
}
