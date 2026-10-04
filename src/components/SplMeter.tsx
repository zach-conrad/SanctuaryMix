import { Gauge, X } from "lucide-react";
import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type RefObject } from "react";
import { MINUS } from "../lib/levels";
import type { SplPoint, SplReading, Weighting } from "../lib/types";
import { useMixer } from "../store/mixer";
import { SPANS, useSpl } from "../store/spl";
import "../styles/spl.css";

/** "92.4", "−3.0": one decimal with a real minus sign. */
export function formatSpl(db: number): string {
  const r = Math.round(db * 10) / 10;
  return r < 0 ? `${MINUS}${Math.abs(r).toFixed(1)}` : r.toFixed(1);
}

/** Compact room level for the home screen. Opens the SPL popup. */
export function SplReadout() {
  const config = useSpl((s) => s.config);
  const reading = useSpl((s) => s.reading);
  const audioOn = useMixer((s) => s.audioStatus === "on");
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const popupId = useId();

  useEffect(() => {
    void useSpl.getState().init();
  }, []);

  const live = audioOn && config?.source != null ? reading : null;
  let note: string;
  if (!config) note = "Loading";
  else if (config.source === null) note = "Off · choose a measurement mic";
  else if (!audioOn) note = "Start Dante audio in Setup";
  else if (!live) note = "Waiting for audio";
  else note = `Slow · ${live.calibrated ? "calibrated" : "uncalibrated"}`;

  return (
    <>
      <button
        ref={button}
        className="spl-readout"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? popupId : undefined}
        onClick={() => setOpen((o) => !o)}
      >
        <span className="spl-readout__head">
          <Gauge size={20} strokeWidth={1.75} aria-hidden />
          <span className="text-label">Room level</span>
        </span>
        <span className="spl-readout__values">
          <Level value={live?.slow.a} unit="dBA" large />
          <Level value={live?.slow.c} unit="dBC" />
        </span>
        <span className={`spl-readout__note${live && !live.calibrated ? " spl-uncal" : ""}`}>{note}</span>
      </button>
      {open && <SplPopup id={popupId} anchor={button} onClose={() => setOpen(false)} />}
    </>
  );
}

function Level({ value, unit, large }: { value: number | undefined; unit: string; large?: boolean }) {
  return (
    <span className="spl-level">
      <span className={large ? "text-readout-lg" : "spl-level__small"}>{value === undefined ? "—" : formatSpl(value)}</span>
      <span className="text-caption muted">{unit}</span>
    </span>
  );
}

const POPUP_GAP = 8;

/** The small window with the SPL graph, averages and the measurement settings. */
function SplPopup({ id, anchor, onClose }: { id: string; anchor: RefObject<HTMLButtonElement>; onClose(): void }) {
  const panel = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ top: number; right: number } | null>(null);
  const close = useRef(onClose);
  close.current = onClose;
  const { reading, config, history, span, setSpan, loadHistory } = useSpl();
  const audioOn = useMixer((s) => s.audioStatus === "on");
  const titleId = useId();

  // Sits under the readout, right edges lined up, so it opens over the mixer.
  useLayoutEffect(() => {
    const place = () => {
      const r = anchor.current?.getBoundingClientRect();
      if (r) setPos({ top: r.bottom + POPUP_GAP, right: Math.max(POPUP_GAP, window.innerWidth - r.right) });
    };
    place();
    window.addEventListener("resize", place);
    return () => window.removeEventListener("resize", place);
  }, [anchor]);

  useEffect(() => {
    const trigger = anchor.current;
    panel.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        close.current();
      }
    };
    const onDown = (e: PointerEvent) => {
      const t = e.target as Node;
      if (!panel.current?.contains(t) && !trigger?.contains(t)) close.current();
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onDown, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onDown, true);
      trigger?.focus();
    };
  }, [anchor]);

  // The graph is one point a second; refreshing every two keeps it moving.
  useEffect(() => {
    void loadHistory();
    const timer = setInterval(() => void loadHistory(), 2000);
    return () => clearInterval(timer);
  }, [loadHistory, span]);

  const live = audioOn && config?.source != null ? reading : null;

  return (
    <div
      ref={panel}
      id={id}
      className="spl-popup"
      role="dialog"
      aria-labelledby={titleId}
      tabIndex={-1}
      style={pos ? { top: pos.top, right: pos.right, maxHeight: `calc(100vh - ${pos.top + POPUP_GAP}px)` } : { visibility: "hidden" }}
    >
      <div className="spl-popup__head">
        <h2 id={titleId} className="text-heading">
          Room level
        </h2>
        {live && (
          <span className={`text-caption ${live.calibrated ? "muted" : "spl-uncal"}`}>
            {live.calibrated ? "Calibrated" : "Uncalibrated estimate"}
          </span>
        )}
        <span className="spl-spacer" />
        <button className="sm-btn sm-btn--ghost sm-btn--sm spl-icon-btn" aria-label="Close" title="Close" onClick={onClose}>
          <X size={20} strokeWidth={1.75} />
        </button>
      </div>

      {live && !live.calibrated && (
        <p className="text-caption muted">
          Readings are only good for comparing louder and quieter moments until you calibrate below.
        </p>
      )}

      <div className="spl-graph-head">
        <span className="spl-legend" aria-hidden>
          <span className="spl-key spl-key--a" /> A-weighted
          <span className="spl-key spl-key--c" /> C-weighted
        </span>
        <div className="sm-seg" role="group" aria-label="Graph span">
          {SPANS.map((m) => (
            <button key={m} aria-pressed={span === m} onClick={() => setSpan(m)}>
              {m === 60 ? "1 hr" : `${m} min`}
            </button>
          ))}
        </div>
      </div>
      <SplGraph points={live ? history : []} spanSecs={span * 60} />

      {live && <SplTable reading={live} />}
      <SplSettings />
    </div>
  );
}

const W = 480;
const H = 180;
const PAD = { l: 36, r: 20, t: 8, b: 22 };

/** One-second Leq, A and C, over the chosen span. One axis: both are dB SPL. */
export function SplGraph({ points, spanSecs }: { points: SplPoint[]; spanSecs: number }) {
  const svg = useRef<SVGSVGElement>(null);
  const [hover, setHover] = useState<number | null>(null);
  const end = points.length ? points[points.length - 1].t : 0;
  const visible = useMemo(() => points.filter((p) => p.t > end - spanSecs), [points, end, spanSecs]);

  const [lo, hi] = useMemo(() => {
    if (!visible.length) return [50, 110];
    let min = Infinity;
    let max = -Infinity;
    for (const p of visible) {
      min = Math.min(min, p.a, p.c);
      max = Math.max(max, p.a, p.c);
    }
    let l = Math.floor((min - 3) / 10) * 10;
    let h = Math.ceil((max + 3) / 10) * 10;
    if (h - l < 30) {
      const mid = (h + l) / 2;
      l = Math.floor((mid - 15) / 10) * 10;
      h = l + 30;
    }
    return [l, h];
  }, [visible]);

  const x = (t: number) => PAD.l + ((t - (end - spanSecs)) / spanSecs) * (W - PAD.l - PAD.r);
  const y = (db: number) => PAD.t + (1 - (db - lo) / (hi - lo)) * (H - PAD.t - PAD.b);
  const path = (key: "a" | "c") =>
    visible.map((p, i) => `${i ? "L" : "M"}${x(p.t).toFixed(1)},${y(p[key]).toFixed(1)}`).join("");

  const grid: number[] = [];
  for (let db = lo; db <= hi; db += 10) grid.push(db);
  const spanMin = spanSecs / 60;
  const ticks = [0, 1 / 3, 2 / 3, 1].map((f) => Math.round(spanMin * (1 - f)));
  const last = visible[visible.length - 1];
  const hovered = hover !== null ? visible[hover] : null;

  const onMove = (e: React.PointerEvent) => {
    const rect = svg.current?.getBoundingClientRect();
    if (!rect || !visible.length) return;
    const t = end - spanSecs + ((((e.clientX - rect.left) / rect.width) * W - PAD.l) / (W - PAD.l - PAD.r)) * spanSecs;
    let best = 0;
    for (let i = 1; i < visible.length; i++) if (Math.abs(visible[i].t - t) < Math.abs(visible[best].t - t)) best = i;
    setHover(best);
  };

  return (
    <div className="spl-graph">
      <svg
        ref={svg}
        viewBox={`0 0 ${W} ${H}`}
        role="img"
        aria-label={
          last
            ? `Room level over the last ${spanMin} minutes. Now ${formatSpl(last.a)} dBA, ${formatSpl(last.c)} dBC.`
            : "Room level graph, no measurements yet."
        }
        onPointerMove={onMove}
        onPointerLeave={() => setHover(null)}
      >
        {grid.map((db) => (
          <g key={db}>
            <line className="spl-grid" x1={PAD.l} x2={W - PAD.r} y1={y(db)} y2={y(db)} />
            <text className="spl-axis" x={PAD.l - 6} y={y(db) + 4} textAnchor="end">
              {db}
            </text>
          </g>
        ))}
        {ticks.map((m, i) => (
          <text
            key={i}
            className="spl-axis"
            x={PAD.l + (i / 3) * (W - PAD.l - PAD.r)}
            y={H - 6}
            textAnchor={i === 0 ? "start" : i === 3 ? "end" : "middle"}
          >
            {m === 0 ? "now" : `${MINUS}${m} min`}
          </text>
        ))}
        {visible.length > 1 && (
          <>
            <path className="spl-line spl-line--c" d={path("c")} />
            <path className="spl-line spl-line--a" d={path("a")} />
            <text className="spl-direct" x={x(last.t) + 4} y={y(last.c) + 4}>
              C
            </text>
            <text className="spl-direct" x={x(last.t) + 4} y={y(last.a) + 4}>
              A
            </text>
          </>
        )}
        {hovered && (
          <line className="spl-crosshair" x1={x(hovered.t)} x2={x(hovered.t)} y1={PAD.t} y2={H - PAD.b} />
        )}
      </svg>
      {!visible.length && <p className="spl-graph__empty text-caption muted">The graph fills in once the measurement mic has audio.</p>}
      {hovered && (
        <div
          className="spl-tooltip"
          style={{ left: `${(x(hovered.t) / W) * 100}%` }}
          data-flip={x(hovered.t) > W * 0.6 || undefined}
        >
          <span className="text-caption muted">{ago(end - hovered.t)}</span>
          <span className="text-readout">A {formatSpl(hovered.a)} dB</span>
          <span className="text-readout">C {formatSpl(hovered.c)} dB</span>
        </div>
      )}
    </div>
  );
}

function ago(secs: number): string {
  if (secs < 1) return "Now";
  const m = Math.floor(secs / 60);
  const s = Math.round(secs % 60);
  return m ? `${m} min ${s} s ago` : `${s} s ago`;
}

function SplTable({ reading }: { reading: SplReading }) {
  const rows: [string, number, number][] = [
    ["Fast", reading.fast.a, reading.fast.c],
    ["Slow", reading.slow.a, reading.slow.c],
    ["Leq 1 min", reading.leq1m.a, reading.leq1m.c],
    ["Leq 15 min", reading.leq15m.a, reading.leq15m.c],
    [`Leq since reset (${Math.floor(reading.seconds / 60)} min)`, reading.leqTotal.a, reading.leqTotal.c],
  ];
  return (
    <table className="spl-table">
      <thead>
        <tr>
          <th>Level</th>
          <th>dBA</th>
          <th>dBC</th>
        </tr>
      </thead>
      <tbody>
        {rows.map(([label, a, c]) => (
          <tr key={label}>
            <td>{label}</td>
            <td className="text-readout">{formatSpl(a)}</td>
            <td className="text-readout">{formatSpl(c)}</td>
          </tr>
        ))}
        <tr>
          <td>Loudest (A fast max) and C peak</td>
          <td className="text-readout">{formatSpl(reading.aMax)}</td>
          <td className="text-readout">{formatSpl(reading.cPeak)}</td>
        </tr>
      </tbody>
    </table>
  );
}

/** Source, calibration and reset. */
function SplSettings() {
  const { config, error, setSource, setOffset, calibrate, reset, clearError, dismissCalibration } = useSpl();
  const listening = useSpl((s) => s.calibration?.state === "listening");
  const strips = useMixer((s) => s.strips);
  const audio = useMixer((s) => s.audio);
  const [reference, setReference] = useState("");
  const [weighting, setWeighting] = useState<Weighting>("c");
  const [offset, setOffsetText] = useState("");

  useEffect(() => {
    if (config) setOffsetText(config.offsetDb.toFixed(1));
  }, [config]);

  if (!config) return null;
  const inputs = audio?.channels ?? strips.length;
  const name = (i: number) => strips[i]?.name;

  const commitOffset = () => {
    const v = Number(offset.replace(MINUS, "-"));
    if (Number.isFinite(v) && Math.abs(v - config.offsetDb) >= 0.05) void setOffset(v);
    else setOffsetText(config.offsetDb.toFixed(1));
  };

  return (
    <div className="spl-settings">
      <label className="sm-field">
        <span className="sm-field__label">Measurement input</span>
        <select
          className="sm-input"
          value={config.source ?? ""}
          onChange={(e) => void setSource(e.target.value === "" ? null : Number(e.target.value))}
        >
          <option value="">Off</option>
          {Array.from({ length: Math.max(inputs, (config.source ?? -1) + 1) }, (_, i) => (
            <option key={i} value={i}>
              {`Input ${i + 1}${name(i) ? ` · ${name(i)}` : ""}`}
            </option>
          ))}
        </select>
        <span className="sm-field__help">
          A measurement mic out in the room, not a stage mic. Changing it starts the averages over.
        </span>
      </label>

      <fieldset className="spl-calibrate" disabled={config.source === null || listening}>
        <legend className="sm-field__label">Calibrate</legend>
        <CalibrationSummary />
        <ol className="spl-steps text-caption muted">
          <li>Hold an SPL meter set to slow next to the measurement mic, or fit a 94 dB calibrator on it.</li>
          <li>Play steady pink noise through the system (not needed with a calibrator) and keep the room quiet.</li>
          <li>Enter what the meter reads and press Calibrate. SanctuaryMix waits for 5 seconds of steady level and averages it.</li>
        </ol>
        <div className="spl-calibrate__row">
          <label className="sm-field spl-calibrate__ref">
            <span className="sm-field__label">Reference meter reads</span>
            <input
              className="sm-input"
              inputMode="decimal"
              placeholder="94.0"
              value={reference}
              onChange={(e) => {
                setReference(e.target.value);
                clearError();
                dismissCalibration();
              }}
            />
          </label>
          <div className="sm-seg" role="group" aria-label="Reference meter weighting">
            {(["a", "c"] as const).map((w) => (
              <button key={w} type="button" aria-pressed={weighting === w} onClick={() => setWeighting(w)}>
                dB{w.toUpperCase()}
              </button>
            ))}
          </div>
          <button
            className="sm-btn"
            disabled={reference.trim() === ""}
            onClick={() => void calibrate(weighting, Number(reference.replace(MINUS, "-")))}
          >
            Calibrate
          </button>
        </div>
      </fieldset>
      <CalibrationProgress />

      <label className="sm-field">
        <span className="sm-field__label">Offset (dB)</span>
        <input
          className="sm-input spl-offset"
          inputMode="decimal"
          value={offset}
          onChange={(e) => setOffsetText(e.target.value)}
          onBlur={commitOffset}
          onKeyDown={(e) => e.key === "Enter" && commitOffset()}
        />
        <span className="sm-field__help">
          Added to the input's dBFS level. Typing one marks readings uncalibrated. Recalibrate after changing this
          input's gain on the console.
        </span>
      </label>

      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}

      <div className="spl-settings__actions">
        <button className="sm-btn sm-btn--ghost" disabled={config.source === null} onClick={() => void reset()}>
          Reset averages
        </button>
        <span className="text-caption muted">Clears the graph, Leq, loudest and peak. Do it before the service starts.</span>
      </div>
    </div>
  );
}

const dateTime = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });

/** What the saved calibration was made against, and whether it still applies. */
function CalibrationSummary() {
  const config = useSpl((s) => s.config);
  if (!config) return null;
  const record = config.calibration;
  if (!record) {
    return <p className="text-caption spl-uncal">Not calibrated. Readings are estimates.</p>;
  }
  const what = `${formatSpl(record.referenceDb)} dB${record.weighting.toUpperCase()} on Input ${record.source + 1}, ${record.device}`;
  return config.calibrated ? (
    <p className="text-caption muted">
      Calibrated {dateTime.format(record.atMs)} to {what}.
    </p>
  ) : (
    <p className="text-caption spl-uncal">
      The last calibration was for {what} at {record.sampleRate / 1000} kHz. Something has changed since, so calibrate
      again.
    </p>
  );
}

/** Live progress while calibrating, then how it went. */
function CalibrationProgress() {
  const calibration = useSpl((s) => s.calibration);
  const cancel = useSpl((s) => s.cancelCalibration);
  if (!calibration) return null;
  if (calibration.state === "done") {
    return (
      <p className="spl-cal-result text-caption" role="status">
        Calibrated. Offset is now {formatSpl(calibration.offsetDb)} dB.
      </p>
    );
  }
  if (calibration.state === "failed") {
    return (
      <p className="error" role="alert">
        {calibration.reason}
      </p>
    );
  }
  const { steadySecs, neededSecs, levelDbfs, hold } = calibration;
  return (
    <div className="spl-cal-progress" role="status">
      <div className="spl-cal-progress__head">
        <span className="text-body-strong">Listening</span>
        <span className="text-readout">
          {levelDbfs <= -139 ? "—" : `${formatSpl(levelDbfs)} dBFS`}
        </span>
        <span className="spl-spacer" />
        <button className="sm-btn sm-btn--sm" onClick={() => void cancel()}>
          Cancel
        </button>
      </div>
      <div
        className="spl-cal-bar"
        role="progressbar"
        aria-label="Steady signal"
        aria-valuemin={0}
        aria-valuemax={neededSecs}
        aria-valuenow={Math.min(steadySecs, neededSecs)}
      >
        <span style={{ width: `${Math.min(100, (steadySecs / neededSecs) * 100)}%` }} />
      </div>
      <span className="text-caption muted">
        {hold ?? `Steady for ${steadySecs.toFixed(1)} of ${neededSecs.toFixed(0)} seconds`}
      </span>
    </div>
  );
}
