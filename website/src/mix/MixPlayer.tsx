import { Pause, Play, RotateCcw, RotateCw } from "lucide-react";
import { useEffect, useMemo, useRef } from "react";
import { dbToFader, formatDb } from "../../../src/lib/levels";
import {
  SOURCE_LABEL,
  buildLanes,
  channelKey,
  channelLabel,
  describeEvents,
  formatClock,
  stateAt,
} from "../../../src/lib/recordings";
import type { RecordedEvent } from "../cloud/types";
import { Timeline } from "./Timeline";
import { usePlayback } from "./usePlayback";

interface Props {
  audioUrl: string | null;
  durationMs: number;
  /** Empty hides the moves (a share link made without them). */
  events: RecordedEvent[];
}

const SKIP_MS = 10_000;

/**
 * Playback only: listen to the service and watch its fader and mute moves.
 * Nothing here can reach a console, and there's no output to choose; the
 * browser plays through whatever the listener is using.
 */
export function MixPlayer({ audioUrl, durationMs, events }: Props) {
  const p = usePlayback(audioUrl, durationMs);
  const lanes = useMemo(() => buildLanes(events, durationMs), [events, durationMs]);
  const rows = useMemo(() => describeEvents(events), [events]);
  const channels = useMemo(() => stateAt(events, p.positionMs), [events, p.positionMs]);
  const moved = useMemo(() => new Set(lanes.map((l) => channelKey(l.id))), [lanes]);
  const showMoves = events.length > 0;

  return (
    <div className="mix-player">
      <section className="panel mix-transport" aria-label="Playback">
        <div className="mix-transport__buttons">
          <button
            className="sm-btn sm-btn--primary sm-btn--lg mix-transport__play"
            onClick={p.toggle}
            disabled={!p.ready && !p.error}
          >
            {p.playing ? <Pause aria-hidden="true" /> : <Play aria-hidden="true" />}
            {p.playing ? "Pause" : "Play"}
          </button>
          <button
            className="sm-btn sm-btn--lg mix-transport__skip"
            onClick={() => p.seek(p.positionMs - SKIP_MS)}
            aria-label="Back 10 seconds"
            title="Back 10 seconds"
          >
            <RotateCcw aria-hidden="true" />
          </button>
          <button
            className="sm-btn sm-btn--lg mix-transport__skip"
            onClick={() => p.seek(p.positionMs + SKIP_MS)}
            aria-label="Forward 10 seconds"
            title="Forward 10 seconds"
          >
            <RotateCw aria-hidden="true" />
          </button>
        </div>
        <div className="mix-transport__scrub">
          <span className="text-readout mix-transport__time">
            {formatClock(p.positionMs)} / {formatClock(durationMs)}
          </span>
          <input
            type="range"
            className="mix-seek"
            min={0}
            max={durationMs}
            step={100}
            value={Math.round(p.positionMs)}
            onChange={(e) => p.seek(Number(e.currentTarget.value))}
            aria-label="Position"
            aria-valuetext={formatClock(p.positionMs)}
            style={{ ["--pos" as string]: `${(p.positionMs / Math.max(1, durationMs)) * 100}%` }}
          />
        </div>
        {p.error ? (
          <p className="mix-transport__note text-caption" role="alert">{p.error}</p>
        ) : !audioUrl ? (
          <p className="mix-transport__note text-caption">This service has moves only, no audio. Play steps through them.</p>
        ) : null}
      </section>

      {showMoves ? (
        <>
          <section className="panel mix-timeline" aria-labelledby="moves-heading">
            <div className="panel__head">
              <h2 id="moves-heading" className="text-heading">Moves</h2>
              <span className="text-caption mix-muted">
                {rows.length} changes on {lanes.length} channels. Click the timeline to jump there.
              </span>
            </div>
            <div className="mix-timeline__scroll">
              <Timeline lanes={lanes} durationMs={durationMs} positionMs={p.positionMs} onSeek={p.seek} />
            </div>
          </section>

          <div className="mix-columns">
            <section className="panel" aria-labelledby="state-heading">
              <div className="panel__head">
                <h2 id="state-heading" className="text-heading">At {formatClock(p.positionMs)}</h2>
              </div>
              <ul className="mix-state" aria-label="Fader and mute at the playhead">
                {channels.map((c) => (
                  <li
                    key={channelKey(c.id)}
                    className="mix-state__row"
                    data-moved={moved.has(channelKey(c.id))}
                    data-muted={c.muted}
                  >
                    <span className="mix-state__name">
                      <span className="text-channel-name">{c.name}</span>
                      <span className="text-readout mix-subtle">{channelLabel(c.id)}</span>
                    </span>
                    <span className="mix-state__bar" aria-hidden="true">
                      <span style={{ width: `${dbToFader(c.faderDb) * 100}%` }} />
                      <i className="mix-state__unity" />
                    </span>
                    <span className="text-readout mix-state__db">
                      {formatDb(c.faderDb)}
                      {c.faderDb !== null && c.faderDb > -90 ? " dB" : ""}
                    </span>
                    <span className="mix-state__mute" data-on={c.muted}>
                      {c.muted ? "Muted" : "On"}
                    </span>
                  </li>
                ))}
              </ul>
            </section>

            <section className="panel" aria-labelledby="changes-heading">
              <div className="panel__head">
                <h2 id="changes-heading" className="text-heading">Changes</h2>
              </div>
              <ChangeList rows={rows} positionMs={p.positionMs} onSeek={p.seek} />
            </section>
          </div>
        </>
      ) : null}
    </div>
  );
}

function ChangeList({
  rows,
  positionMs,
  onSeek,
}: {
  rows: ReturnType<typeof describeEvents>;
  positionMs: number;
  onSeek(ms: number): void;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  let current = -1;
  rows.forEach((r, i) => {
    if (r.tMs <= positionMs) current = i;
  });

  // Keep the latest change in view while playing, without moving the page.
  useEffect(() => {
    const box = scroller.current;
    const row = box?.querySelector<HTMLElement>('tr[aria-current="true"]');
    if (!box || !row) return;
    const top = row.offsetTop - box.clientHeight / 2;
    box.scrollTo({ top: Math.max(0, top) });
  }, [current]);

  if (rows.length === 0) return <p className="mix-muted">Nothing was moved in this service.</p>;
  return (
    <div className="mix-changes" ref={scroller}>
      <table className="mix-changes__table">
        <thead>
          <tr>
            <th className="text-label">Time</th>
            <th className="text-label">Channel</th>
            <th className="text-label">Change</th>
            <th className="text-label">By</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r, i) => (
            <tr key={r.seq} aria-current={i === current}>
              <td>
                <button className="mix-changes__time text-readout" onClick={() => onSeek(r.tMs)}>
                  {formatClock(r.tMs)}
                </button>
              </td>
              <td>{r.name}</td>
              <td className="text-readout">{r.change}</td>
              <td className={r.source === "assist" ? "mix-changes__assist" : "mix-muted"}>{SOURCE_LABEL[r.source]}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
