import { useRef } from "react";
import { dbToFader, faderToDb, formatDb } from "../lib/levels";

interface Props {
  db: number | null;
  disabled?: boolean;
  label: string;
  /** Auto-mix moved it: outline the cap in the Assist hue. */
  assist?: boolean;
  onChange(db: number | null): void;
}

/** Vertical fader, unity at 75%. Drag, arrow keys (Shift for 0.1 dB), or double-click for 0 dB. */
export function Fader({ db, disabled, label, assist, onChange }: Props) {
  const track = useRef<HTMLDivElement>(null);

  const fromPointer = (clientY: number) => {
    const r = track.current!.getBoundingClientRect();
    onChange(faderToDb(Math.max(0, Math.min(1, 1 - (clientY - r.top) / r.height))));
  };

  return (
    <div
      ref={track}
      className={`sm-fader ${disabled ? "is-disabled" : ""}`}
      style={{ "--pos": dbToFader(db) } as React.CSSProperties}
      onPointerDown={(e) => {
        if (disabled) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        fromPointer(e.clientY);
      }}
      onPointerMove={(e) => {
        if (!disabled && e.currentTarget.hasPointerCapture(e.pointerId)) fromPointer(e.clientY);
      }}
      onDoubleClick={() => !disabled && onChange(0)}
    >
      <div className="sm-fader-track" />
      <div className="sm-fader-unity" style={{ bottom: "75%" }} />
      <div
        className="sm-fader-cap"
        data-assist={assist || undefined}
        role="slider"
        tabIndex={disabled ? -1 : 0}
        aria-label={`${label} fader`}
        aria-valuemin={-90}
        aria-valuemax={10}
        aria-valuenow={db ?? -90}
        aria-valuetext={`${formatDb(db)} dB`}
        aria-disabled={disabled}
        onKeyDown={(e) => {
          if (disabled) return;
          const step = e.shiftKey ? 0.1 : 1;
          const cur = db ?? -90;
          if (e.key === "ArrowUp") onChange(Math.min(10, cur + step));
          else if (e.key === "ArrowDown") onChange(cur - step <= -90 ? null : cur - step);
          else return;
          e.preventDefault();
        }}
      />
    </div>
  );
}
