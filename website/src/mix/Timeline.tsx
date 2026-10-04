// Same timeline as the desktop app (src/components/Timeline.tsx), read-only playback.

import { memo, useRef } from "react";
import { formatDb } from "../../../src/lib/levels";
import { SOURCE_LABEL, channelKey, channelLabel, formatClock, type Lane } from "../../../src/lib/recordings";
import type { ChangeSource } from "../../../src/lib/types";

interface Props {
  lanes: Lane[];
  durationMs: number;
  positionMs: number;
  onSeek(ms: number): void;
}

/**
 * Fader height inside a lane, as a percentage from the top, leaving room for
 * the mute bar. Each lane is scaled to its own range (at least 6 dB) so a
 * 1 dB ride is visible; the label's tooltip gives the range.
 */
function laneScale(lane: Lane): { y(db: number | null): number; range: string } {
  const values = lane.fader.map((p) => Math.max(FLOOR_DB, p.db ?? FLOOR_DB));
  const lo = values.length ? Math.min(...values) : 0;
  const hi = values.length ? Math.max(...values) : 0;
  const span = Math.max(6, hi - lo);
  const mid = (hi + lo) / 2;
  const bottom = mid - span / 2;
  return {
    y: (db) => 90 - ((Math.max(FLOOR_DB, db ?? FLOOR_DB) - bottom) / span) * 64,
    range: lo === hi ? `${formatDb(lo)} dB` : `${formatDb(lo)} to ${formatDb(hi)} dB`,
  };
}
const FLOOR_DB = -60;

function axisStep(durationMs: number): number {
  const min = durationMs / 60_000;
  return (min > 120 ? 30 : min > 40 ? 15 : min > 15 ? 5 : 1) * 60_000;
}

/**
 * One lane per channel that moved: the fader as a step line, mutes as a red bar
 * along the top, and a marker per change shaped by who made it. Click to seek.
 */
export function Timeline({ lanes, durationMs, positionMs, onSeek }: Props) {
  const tracks = useRef<HTMLDivElement>(null);
  const dur = Math.max(1, durationMs);
  const pct = (ms: number) => `${(Math.min(dur, Math.max(0, ms)) / dur) * 100}%`;
  const step = axisStep(dur);
  const ticks = Array.from({ length: Math.floor(dur / step) + 1 }, (_, i) => i * step);

  const seekFromPointer = (clientX: number) => {
    const r = tracks.current!.getBoundingClientRect();
    onSeek(Math.round(Math.max(0, Math.min(1, (clientX - r.left) / r.width)) * dur));
  };

  return (
    <div className="timeline">
      <Legend lanes={lanes} />
      <div className="tl-body">
        <div className="tl-labels">
          <div className="tl-axis-spacer" />
          {lanes.map((l) => (
            <div key={channelKey(l.id)} className="tl-label" title={`${l.name}: fader ${laneScale(l).range}`}>
              <span className="text-channel-name tl-name">{l.name}</span>
              <span className="text-readout tl-num">{channelLabel(l.id)}</span>
            </div>
          ))}
        </div>
        <div
          ref={tracks}
          className="tl-tracks"
          role="slider"
          tabIndex={0}
          aria-label="Service timeline"
          aria-valuemin={0}
          aria-valuemax={Math.round(dur / 1000)}
          aria-valuenow={Math.round(positionMs / 1000)}
          aria-valuetext={formatClock(positionMs)}
          onPointerDown={(e) => {
            e.currentTarget.setPointerCapture(e.pointerId);
            seekFromPointer(e.clientX);
          }}
          onPointerMove={(e) => {
            if (e.currentTarget.hasPointerCapture(e.pointerId)) seekFromPointer(e.clientX);
          }}
          onKeyDown={(e) => {
            const by = e.shiftKey ? 60_000 : 5_000;
            if (e.key === "ArrowRight") onSeek(Math.min(dur, positionMs + by));
            else if (e.key === "ArrowLeft") onSeek(Math.max(0, positionMs - by));
            else if (e.key === "Home") onSeek(0);
            else if (e.key === "End") onSeek(dur);
            else return;
            e.preventDefault();
          }}
        >
          <div className="tl-axis" aria-hidden>
            {ticks.map((t) => (
              <span key={t} className="tl-tick text-readout" style={{ left: pct(t) }}>
                {formatClock(t).replace(/:00$/, "")}
              </span>
            ))}
          </div>
          {ticks.map((t) => (
            <div key={t} className="tl-gridline" style={{ left: pct(t) }} aria-hidden />
          ))}
          {lanes.map((l) => (
            <LaneTrack key={channelKey(l.id)} lane={l} dur={dur} />
          ))}
          <div className="tl-playhead" style={{ left: pct(positionMs) }} aria-hidden>
            <span className="tl-playhead-time text-readout">{formatClock(positionMs)}</span>
          </div>
        </div>
      </div>
      {lanes.length === 0 && <p className="empty">Nothing was moved in this service.</p>}
    </div>
  );
}

const LaneTrack = memo(function LaneTrack({ lane, dur }: { lane: Lane; dur: number }) {
  const x = (ms: number) => (Math.min(dur, ms) / dur) * 1000;
  const { y: yPct } = laneScale(lane);
  let d = "";
  lane.fader.forEach((p, i) => {
    const y = yPct(p.db);
    d += i === 0 ? `M ${x(p.tMs)} ${y}` : ` H ${x(p.tMs)} V ${y}`;
  });
  if (lane.fader.length) d += ` H 1000`;
  const faderAt = (t: number) => {
    let db: number | null = lane.fader[0]?.db ?? 0;
    for (const p of lane.fader) {
      if (p.tMs > t) break;
      db = p.db;
    }
    return db;
  };

  return (
    <div className="tl-lane">
      {lane.mutes.map((m) => (
        <div
          key={m.fromMs}
          className="tl-mute"
          style={{ left: `${(m.fromMs / dur) * 100}%`, width: `${Math.max(0.2, ((m.toMs - m.fromMs) / dur) * 100)}%` }}
        />
      ))}
      <svg className="tl-fader" viewBox="0 0 1000 100" preserveAspectRatio="none" aria-hidden>
        <path d={d} vectorEffect="non-scaling-stroke" />
      </svg>
      {lane.marks.map((m, i) => (
        <span
          key={i}
          className={`tl-mark tl-mark--${m.source}`}
          data-kind={m.kind}
          style={{ left: `${(m.tMs / dur) * 100}%`, top: m.kind === "mute" ? "var(--space-1)" : `${yPct(faderAt(m.tMs))}%` }}
        />
      ))}
    </div>
  );
});

const LEGEND: ChangeSource[] = ["operator", "console", "assist", "replay"];

function Legend({ lanes }: { lanes: Lane[] }) {
  const present = new Set(lanes.flatMap((l) => l.marks.map((m) => m.source)));
  return (
    <ul className="tl-legend" aria-label="Key">
      <li>
        <span className="tl-swatch-line" aria-hidden />
        Fader
      </li>
      <li>
        <span className="tl-swatch-mute" aria-hidden />
        Muted
      </li>
      {LEGEND.filter((s) => present.has(s)).map((s) => (
        <li key={s}>
          <span className={`tl-mark tl-mark--${s} tl-mark--legend`} aria-hidden />
          {s === "operator" ? "Moved in the app" : s === "console" ? "Moved on the desk" : s === "assist" ? "Moved by Assist" : SOURCE_LABEL.replay}
        </li>
      ))}
    </ul>
  );
}
