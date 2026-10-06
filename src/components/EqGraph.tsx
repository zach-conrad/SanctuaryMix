import { memo, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { cloneEq, isNotchUsed, RESERVED_BAND } from "../lib/aieq";
import { curveFreqs, formatGain, formatHz, hpfResponse, response, SPECTRUM_FREQS } from "../lib/eqCurve";
import type { ChannelEq } from "../lib/types";

export type EqGraphSize = "thumb" | "compact" | "full";
/** "hpf" or a band index 0-3 (shown as H and 1-4). */
export type HandleId = "hpf" | number;

const SIZES = {
  thumb: { w: 128, h: 36, l: 0, r: 0, t: 3, b: 3, range: 12, points: 64 },
  compact: { w: 288, h: 84, l: 0, r: 0, t: 4, b: 4, range: 12, points: 120 },
  full: { w: 592, h: 248, l: 36, r: 8, t: 10, b: 22, range: 15, points: 200 },
} as const;

const FMIN = 20;
const FMAX = 20000;
const TICKS = [20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000];
const STEP = 2 ** (1 / 12);

interface Props {
  /** The console's EQ (solid line). */
  eq: ChannelEq | null;
  /** What AI EQ suggests (dashed blue line). */
  proposed?: ChannelEq | null;
  /** What the mic hears: 60 values on SPECTRUM_FREQS, dB around the average. */
  spectrum?: number[] | null;
  /** A ringing frequency (dotted amber line). */
  ringHz?: number | null;
  size: EqGraphSize;
  /** Band points can be dragged and nudged with the arrow keys. */
  editable?: boolean;
  selected?: HandleId | null;
  onSelect?(id: HandleId): void;
  /** A hand edit; `done` is true when a drag ends or a key nudges. */
  onChange?(eq: ChannelEq, done: boolean): void;
  label: string;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const roundHz = (f: number) => (f < 100 ? Math.round(f) : f < 1000 ? Math.round(f / 5) * 5 : Math.round(f / 50) * 50);

function handleName(id: HandleId) {
  return id === "hpf" ? "H" : String(id + 1);
}

function sameHandle(a: ChannelEq, b: ChannelEq, id: HandleId): boolean {
  if (id === "hpf") return a.hpf.on === b.hpf.on && a.hpf.freqHz === b.hpf.freqHz;
  const x = a.bands[id];
  const y = b.bands[id];
  return x.freqHz === y.freqHz && Math.abs(x.gainDb - y.gainDb) < 0.05 && x.width === y.width && x.kind === y.kind;
}

/** The EQ curve as an SVG: the desk solid, a suggestion dashed, the mic's spectrum as a soft fill. */
export const EqGraph = memo(function EqGraph({
  eq,
  proposed = null,
  spectrum = null,
  ringHz = null,
  size,
  editable = false,
  selected = null,
  onSelect,
  onChange,
  label,
}: Props) {
  const S = SIZES[size];
  const svg = useRef<SVGSVGElement>(null);
  const [drag, setDrag] = useState<HandleId | null>(null);
  const last = useRef<ChannelEq | null>(null);
  const compact = size !== "full";

  const plotW = S.w - S.l - S.r;
  const plotH = S.h - S.t - S.b;
  const lmin = Math.log10(FMIN);
  const lspan = Math.log10(FMAX) - lmin;
  const X = (f: number) => S.l + ((Math.log10(f) - lmin) / lspan) * plotW;
  const Y = (db: number) => S.t + ((S.range - db) / (2 * S.range)) * plotH;
  const invX = (x: number) => 10 ** (lmin + ((x - S.l) / plotW) * lspan);
  const invY = (y: number) => S.range - ((y - S.t) / plotH) * 2 * S.range;

  const freqs = useMemo(() => curveFreqs(S.points), [S.points]);
  const path = (e: ChannelEq) =>
    freqs
      .map((f, i) => `${i ? "L" : "M"}${X(f).toFixed(1)},${Y(clamp(response(e, f), -S.range, S.range)).toFixed(1)}`)
      .join("");
  const nowPath = useMemo(() => (eq ? path(eq) : null), [eq, freqs, size]); // eslint-disable-line react-hooks/exhaustive-deps
  const aiPath = useMemo(() => (proposed ? path(proposed) : null), [proposed, freqs, size]); // eslint-disable-line react-hooks/exhaustive-deps

  const specPath = useMemo(() => {
    if (!spectrum || spectrum.length === 0) return null;
    const floor = S.h - S.b;
    let d = `M${X(FMIN).toFixed(1)},${floor}`;
    SPECTRUM_FREQS.forEach((f, i) => {
      const v = spectrum[i] ?? -30;
      const y = floor - clamp((v + 30) / 42, 0, 1) * plotH * 0.85;
      d += `L${X(Math.min(f, FMAX)).toFixed(1)},${y.toFixed(1)}`;
    });
    return `${d}L${X(FMAX).toFixed(1)},${floor}Z`;
  }, [spectrum, size]); // eslint-disable-line react-hooks/exhaustive-deps

  const shown = proposed ?? eq;
  const canEdit = editable && !proposed && !!eq && !!onChange;

  function handleIds(e: ChannelEq): HandleId[] {
    const ids: HandleId[] = ["hpf", 0, 1, 2];
    if (isNotchUsed(e)) ids.push(RESERVED_BAND);
    return ids;
  }

  function handlePos(e: ChannelEq, id: HandleId) {
    if (id === "hpf") {
      const f = e.hpf.freqHz;
      return { x: X(f), y: Y(clamp(e.hpf.on ? hpfResponse(e.hpf, f) : 0, -S.range + 1, S.range - 1)) };
    }
    const f = e.bands[id].freqHz;
    return { x: X(f), y: Y(clamp(response(e, f), -S.range + 1, S.range - 1)) };
  }

  /** The EQ with one handle moved to an SVG point. */
  function moveTo(base: ChannelEq, id: HandleId, x: number, y: number): ChannelEq {
    const next = cloneEq(base);
    const f = roundHz(clamp(invX(clamp(x, S.l, S.w - S.r)), FMIN, FMAX));
    if (id === "hpf") {
      next.hpf = { on: true, freqHz: clamp(f, 20, 2000) };
      return next;
    }
    const band = next.bands[id];
    band.freqHz = f;
    if (band.kind !== "lowPass" && band.kind !== "highPass") {
      band.gainDb = Math.round(clamp(invY(y), -S.range, S.range) * 2) / 2;
    }
    return next;
  }

  function nudge(base: ChannelEq, id: HandleId, key: string, shift: boolean): ChannelEq | null {
    const next = cloneEq(base);
    if (id === "hpf") {
      if (key !== "ArrowLeft" && key !== "ArrowRight") return null;
      const f = next.hpf.freqHz * (key === "ArrowRight" ? STEP : 1 / STEP);
      next.hpf = { on: true, freqHz: roundHz(clamp(f, 20, 2000)) };
      return next;
    }
    const band = next.bands[id];
    if (key === "ArrowLeft" || key === "ArrowRight") {
      band.freqHz = roundHz(clamp(band.freqHz * (key === "ArrowRight" ? STEP : 1 / STEP), FMIN, FMAX));
    } else if (shift) {
      // Shift + Up narrows (higher Q), Shift + Down widens.
      band.width = Math.round(clamp(band.width * (key === "ArrowUp" ? 0.8 : 1.25), 1 / 9, 1.5) * 1000) / 1000;
    } else {
      band.gainDb = clamp(band.gainDb + (key === "ArrowUp" ? 0.5 : -0.5), -S.range, S.range);
    }
    return next;
  }

  function point(e: PointerEvent) {
    const r = svg.current!.getBoundingClientRect();
    return { x: ((e.clientX - r.left) * S.w) / r.width, y: ((e.clientY - r.top) * S.h) / r.height };
  }

  function onHandleDown(e: PointerEvent, id: HandleId) {
    onSelect?.(id);
    if (!canEdit) return;
    e.preventDefault();
    svg.current?.setPointerCapture(e.pointerId);
    setDrag(id);
  }

  function onMove(e: PointerEvent) {
    if (drag === null || !eq || !onChange) return;
    const p = point(e);
    const next = moveTo(eq, drag, p.x, p.y);
    last.current = next;
    onChange(next, false);
  }

  function onUp(e: PointerEvent) {
    if (drag === null) return;
    svg.current?.releasePointerCapture?.(e.pointerId);
    setDrag(null);
    if (last.current && onChange) onChange(last.current, true);
    last.current = null;
  }

  function onKey(e: KeyboardEvent, id: HandleId) {
    if (!canEdit || !eq || !onChange) return;
    if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(e.key)) return;
    e.preventDefault();
    const next = nudge(eq, id, e.key, e.shiftKey);
    if (next) onChange(next, true);
  }

  function valueText(e: ChannelEq, id: HandleId) {
    if (id === "hpf") return e.hpf.on ? `Low cut ${formatHz(e.hpf.freqHz)}` : "Low cut off";
    const b = e.bands[id];
    return `${formatHz(b.freqHz)}, ${formatGain(b.gainDb)}`;
  }

  return (
    <svg
      ref={svg}
      className={`eq-graph${drag !== null ? " is-dragging" : ""}`}
      viewBox={`0 0 ${S.w} ${S.h}`}
      role={size === "full" ? "group" : "img"}
      aria-label={label}
      onPointerMove={onMove}
      onPointerUp={onUp}
      onPointerCancel={onUp}
    >
      {TICKS.map((f) => (
        <line key={f} className="grid" x1={X(f)} x2={X(f)} y1={S.t} y2={S.h - S.b} />
      ))}
      {(compact ? [-6, 6] : [-12, -6, 6, 12]).map((d) => (
        <line key={d} className="grid" x1={S.l} x2={S.w - S.r} y1={Y(d)} y2={Y(d)} />
      ))}
      <line className="grid-zero" x1={S.l} x2={S.w - S.r} y1={Y(0)} y2={Y(0)} />
      {!compact && (
        <g aria-hidden>
          {TICKS.map((f) => (
            <text
              key={f}
              className="axis"
              x={X(f)}
              y={S.h - 6}
              textAnchor={f === FMIN ? "start" : f === FMAX ? "end" : "middle"}
            >
              {f >= 1000 ? `${f / 1000}k` : f}
            </text>
          ))}
          {[-12, -6, 0, 6, 12].map((d) => (
            <text key={d} className="axis" x={S.l - 6} y={Y(d) + 4} textAnchor="end">
              {d > 0 ? `+${d}` : d < 0 ? `−${-d}` : "0"}
            </text>
          ))}
        </g>
      )}
      {specPath && <path className="spectrum" d={specPath} />}
      {ringHz && <line className="ring" x1={X(ringHz)} x2={X(ringHz)} y1={S.t} y2={S.h - S.b} />}
      {nowPath && <path className="curve-now" d={nowPath} />}
      {aiPath && <path className="curve-ai" d={aiPath} />}
      {size === "full" &&
        shown &&
        handleIds(shown).map((id) => {
          const { x, y } = handlePos(shown, id);
          const isAi = !!proposed && !!eq && !sameHandle(eq, proposed, id);
          const reserved = id === RESERVED_BAND;
          const cls = ["handle", isAi && "is-ai", selected === id && "is-selected", reserved && "is-reserved", !canEdit && "is-static"]
            .filter(Boolean)
            .join(" ");
          return (
            <g
              key={String(id)}
              className={cls}
              tabIndex={0}
              role="slider"
              aria-label={id === "hpf" ? "Low cut" : `Band ${id + 1}`}
              aria-valuemin={-S.range}
              aria-valuemax={S.range}
              aria-valuenow={id === "hpf" ? shown.hpf.freqHz : shown.bands[id].gainDb}
              aria-valuetext={valueText(shown, id)}
              aria-readonly={!canEdit}
              onPointerDown={(e) => onHandleDown(e, id)}
              onFocus={() => onSelect?.(id)}
              onKeyDown={(e) => onKey(e, id)}
            >
              <circle className="handle-hit" cx={x} cy={y} r={22} />
              <circle className="dot" cx={x} cy={y} r={11} />
              <text x={x} y={y + 0.5}>
                {handleName(id)}
              </text>
            </g>
          );
        })}
    </svg>
  );
});
