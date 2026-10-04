import { useRef } from "react";
import { dbToFader, faderToDb, formatDb } from "../lib/levels";

const MARKS = [10, 0, -10, -20, -30, -50];

interface Props {
  db: number | null;
  disabled?: boolean;
  label: string;
  onChange(db: number | null): void;
}

/** Vertical console-style fader. Drag, arrow keys, or double-click for 0 dB. */
export function Fader({ db, disabled, label, onChange }: Props) {
  const track = useRef<HTMLDivElement>(null);
  const pos = dbToFader(db);

  const fromPointer = (clientY: number) => {
    const r = track.current!.getBoundingClientRect();
    const p = 1 - (clientY - r.top) / r.height;
    onChange(faderToDb(Math.max(0, Math.min(1, p))));
  };

  return (
    <div className={`fader ${disabled ? "disabled" : ""}`}>
      <div className="fader-scale" aria-hidden>
        {MARKS.map((m) => (
          <span key={m} style={{ bottom: `${dbToFader(m) * 100}%` }}>
            {m > 0 ? `+${m}` : m}
          </span>
        ))}
      </div>
      <div
        ref={track}
        className="fader-track"
        role="slider"
        tabIndex={disabled ? -1 : 0}
        aria-label={`${label} fader`}
        aria-valuemin={-90}
        aria-valuemax={10}
        aria-valuenow={db ?? -90}
        aria-valuetext={`${formatDb(db)} dB`}
        aria-disabled={disabled}
        onPointerDown={(e) => {
          if (disabled) return;
          e.currentTarget.setPointerCapture(e.pointerId);
          fromPointer(e.clientY);
        }}
        onPointerMove={(e) => {
          if (!disabled && e.currentTarget.hasPointerCapture(e.pointerId)) fromPointer(e.clientY);
        }}
        onDoubleClick={() => !disabled && onChange(0)}
        onKeyDown={(e) => {
          if (disabled) return;
          const step = e.shiftKey ? 0.1 : 1;
          if (e.key === "ArrowUp") onChange(Math.min(10, (db ?? -90) + step));
          else if (e.key === "ArrowDown") onChange(faderToDb(dbToFader((db ?? -90) - step)));
          else return;
          e.preventDefault();
        }}
      >
        <div className="fader-groove" />
        <div className="fader-unity" style={{ bottom: `${dbToFader(0) * 100}%` }} />
        <div className="fader-cap" style={{ bottom: `${pos * 100}%` }} />
      </div>
    </div>
  );
}
