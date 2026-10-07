import { X } from "lucide-react";
import { useEffect, useRef, type ReactNode } from "react";

interface Props {
  /** Shown while true; closing (Esc, backdrop, ×) calls onClose. */
  open: boolean;
  onClose: () => void;
  labelledBy: string;
  children: ReactNode;
}

/** The share dialog's modal, for any panel: native <dialog>, backdrop click closes. */
export function Dialog({ open, onClose, labelledBy, children }: Props) {
  const ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (open && !el.open) el.showModal();
    if (!open && el.open) el.close();
  }, [open]);

  return (
    <dialog
      ref={ref}
      className="dialog"
      aria-labelledby={labelledBy}
      onClose={onClose}
      onClick={(e) => {
        if (e.target === e.currentTarget) e.currentTarget.close();
      }}
    >
      {open ? children : null}
      <button className="sm-btn sm-btn--ghost dialog__close" onClick={() => ref.current?.close()} aria-label="Close" title="Close">
        <X aria-hidden="true" />
      </button>
    </dialog>
  );
}

/** Two to four choices, as the design system's SegmentedControl. */
export function Segmented<T extends string>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: { id: T; name: string }[];
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <div className="sm-seg" role="group" aria-label={label}>
      {options.map((o) => (
        <button key={o.id} type="button" aria-pressed={value === o.id} onClick={() => onChange(o.id)}>
          {o.name}
        </button>
      ))}
    </div>
  );
}
