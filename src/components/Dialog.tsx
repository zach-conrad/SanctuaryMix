import { useEffect, useId, useRef, type ReactNode } from "react";

interface Props {
  title: string;
  children: ReactNode;
  /** The buttons, rightmost is the main action. */
  actions: ReactNode;
  onClose(): void;
}

/** Modal dialog on a scrim. Esc and the scrim close it; focus starts inside. */
export function Dialog({ title, children, actions, onClose }: Props) {
  const titleId = useId();
  const panel = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  close.current = onClose;

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    panel.current?.querySelector<HTMLElement>("[data-autofocus], button, input, select, textarea")?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        close.current();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      previous?.focus?.();
    };
  }, []);

  return (
    <div className="dialog-scrim" onPointerDown={(e) => e.target === e.currentTarget && onClose()}>
      <div ref={panel} className="dialog" role="dialog" aria-modal="true" aria-labelledby={titleId}>
        <h2 id={titleId} className="text-heading">
          {title}
        </h2>
        <div className="dialog-body">{children}</div>
        <div className="dialog-actions">{actions}</div>
      </div>
    </div>
  );
}
