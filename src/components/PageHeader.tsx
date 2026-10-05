import type { ReactNode } from "react";

/** The title and one-line description every page opens with, with its page-level actions at the right. */
export function PageHeader({ title, children, actions }: { title: string; children?: ReactNode; actions?: ReactNode }) {
  return (
    <header className="page-head">
      <div className="page-head-text">
        <h1 className="text-title">{title}</h1>
        {children && <p className="muted">{children}</p>}
      </div>
      {actions && <div className="page-head-actions">{actions}</div>}
    </header>
  );
}
