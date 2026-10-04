import { ArrowLeft, Pause, Play, RotateCcw, RotateCw, Send, Trash2, TriangleAlert } from "lucide-react";
import { memo, useEffect, useMemo, useRef, useState } from "react";
import { Dialog } from "../components/Dialog";
import { useConsoleName, useIsRecording } from "../components/RecordControl";
import { Timeline } from "../components/Timeline";
import { dbToFader, formatDb } from "../lib/levels";
import {
  SOURCE_LABEL,
  audioModeLabel,
  buildLanes,
  channelKey,
  channelLabel,
  describeEvents,
  formatBytes,
  formatClock,
  serviceDateLabel,
  stateAt,
  timeOfDay,
  type ChannelState,
  type EventRow,
  type Lane,
} from "../lib/recordings";
import type { ChannelId } from "../lib/types";
import { useMixer } from "../store/mixer";
import { useRecordings } from "../store/recordings";

export function ServiceDetail() {
  const detail = useRecordings((s) => s.detail);
  const events = useRecordings((s) => s.events);
  const error = useRecordings((s) => s.error);
  const close = useRecordings((s) => s.close);
  const positionMs = useRecordings((s) => s.playback?.positionMs ?? 0);
  const seek = useRecordings((s) => s.seek);

  const durationMs = detail?.durationMs ?? 0;
  const lanes = useMemo(() => buildLanes(events, durationMs), [events, durationMs]);
  const rows = useMemo(() => describeEvents(events), [events]);
  const state = useMemo(() => stateAt(events, positionMs), [events, positionMs]);

  return (
    <div className="page detail-page">
      <div>
        <button className="sm-btn sm-btn--ghost back-btn" onClick={() => void close()}>
          <ArrowLeft />
          All services
        </button>
      </div>
      {error && <p className="error">{error}</p>}
      {detail && (
        <>
          <DetailHeader key={detail.id} />
          <ReplayBanner />
          <Transport lanes={lanes} />
          <section className="panel timeline-panel" aria-label="Timeline">
            <div className="panel-head">
              <h2 className="text-heading">Moves</h2>
              <span className="text-caption muted">
                {rows.length} changes on {lanes.length} channels · click the timeline to jump there
              </span>
            </div>
            <Timeline lanes={lanes} durationMs={durationMs} positionMs={positionMs} onSeek={(ms) => void seek(ms)} />
          </section>
          <div className="detail-columns">
            <MixerPanel state={state} lanes={lanes} positionMs={positionMs} />
            <section className="panel" aria-label="Changes">
              <div className="panel-head">
                <h2 className="text-heading">Changes</h2>
              </div>
              <EventList rows={rows} positionMs={positionMs} onSeek={(ms) => void seek(ms)} />
            </section>
          </div>
        </>
      )}
      {!detail && !error && <p className="empty">Opening the service…</p>}
    </div>
  );
}

function DetailHeader() {
  const detail = useRecordings((s) => s.detail)!;
  const saveDetails = useRecordings((s) => s.saveDetails);
  const deleteOpen = useRecordings((s) => s.deleteOpen);
  const role = useMixer((s) => s.session?.role);
  const [title, setTitle] = useState(detail.title);
  const [notes, setNotes] = useState(detail.notes);
  const [saved, setSaved] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const canDelete = (role === "admin" || role === "engineer") && detail.status !== "recording";

  const commit = async () => {
    const t = title.trim() || detail.title;
    if (t === detail.title && notes === detail.notes) return;
    setTitle(t);
    await saveDetails(t, notes);
    setSaved(true);
  };

  return (
    <section className="panel detail-head">
      <div className="detail-meta">
        <span className="text-caption muted">
          {serviceDateLabel(detail.serviceDate)} · {timeOfDay(detail.startedAt)} ·{" "}
          <span className="text-readout">{formatClock(detail.durationMs)}</span> · {audioModeLabel(detail)} ·{" "}
          {formatBytes(detail.bytesOnDisk)} · Local only
        </span>
        {detail.status === "interrupted" && (
          <span className="sm-pill sm-pill--warn">
            <span className="sm-pill__dot" aria-hidden />
            Interrupted
            <span className="sm-pill__meta">The app stopped before Stop was pressed</span>
          </span>
        )}
      </div>
      <div className="detail-fields">
        <label className="sm-field title-field">
          <span className="sm-field__label">Title</span>
          <input
            className="sm-input"
            value={title}
            onChange={(e) => {
              setTitle(e.target.value);
              setSaved(false);
            }}
            onBlur={() => void commit()}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          />
        </label>
        <label className="sm-field notes-field">
          <span className="sm-field__label">Notes</span>
          <textarea
            className="sm-input notes-input"
            value={notes}
            rows={2}
            placeholder="Guest speaker, what went well, what to fix next week"
            onChange={(e) => {
              setNotes(e.target.value);
              setSaved(false);
            }}
            onBlur={() => void commit()}
          />
          <span className="sm-field__help">{saved ? "Saved." : "Saved when you click away."}</span>
        </label>
        {canDelete && (
          <button className="sm-btn sm-btn--danger delete-btn" onClick={() => setConfirmDelete(true)}>
            <Trash2 />
            Delete service
          </button>
        )}
      </div>
      {confirmDelete && (
        <Dialog
          title="Delete this service?"
          onClose={() => setConfirmDelete(false)}
          actions={
            <>
              <button className="sm-btn sm-btn--ghost" onClick={() => setConfirmDelete(false)} data-autofocus>
                Cancel
              </button>
              <button className="sm-btn sm-btn--danger" onClick={() => void deleteOpen()}>
                Delete service
              </button>
            </>
          }
        >
          <p>
            This deletes {detail.title} from {serviceDateLabel(detail.serviceDate)}: the audio (
            {formatBytes(detail.bytesOnDisk)}) and all {detail.eventCount} recorded changes.
          </p>
          <p className="muted">This can't be undone.</p>
        </Dialog>
      )}
    </section>
  );
}

function Transport({ lanes }: { lanes: Lane[] }) {
  const playback = useRecordings((s) => s.playback);
  const devices = useRecordings((s) => s.outputDevices);
  const detail = useRecordings((s) => s.detail)!;
  const { play, pause, seek, setOutputDevice } = useRecordings();
  const pos = playback?.positionMs ?? 0;
  const dur = playback?.durationMs ?? detail.durationMs;

  return (
    <section className="panel transport" aria-label="Playback">
      <div className="transport-row">
        <button
          className="sm-btn sm-btn--lg sm-btn--primary play-btn"
          disabled={!playback}
          onClick={() => void (playback?.playing ? pause() : play())}
        >
          {playback?.playing ? <Pause /> : <Play />}
          {playback?.playing ? "Pause" : "Play"}
        </button>
        <button
          className="sm-btn sm-btn--lg icon-btn"
          aria-label="Back 10 seconds"
          title="Back 10 seconds"
          disabled={!playback}
          onClick={() => void seek(Math.max(0, pos - 10_000))}
        >
          <RotateCcw />
        </button>
        <button
          className="sm-btn sm-btn--lg icon-btn"
          aria-label="Forward 10 seconds"
          title="Forward 10 seconds"
          disabled={!playback}
          onClick={() => void seek(Math.min(dur, pos + 10_000))}
        >
          <RotateCw />
        </button>
        <span className="text-readout transport-time">
          {formatClock(pos)} <span className="muted">/ {formatClock(dur)}</span>
        </span>
        <input
          className="seek"
          type="range"
          min={0}
          max={dur}
          step={1000}
          value={pos}
          disabled={!playback}
          aria-label="Position"
          aria-valuetext={formatClock(pos)}
          onChange={(e) => void seek(Number(e.target.value))}
        />
        <label className="sm-field play-through">
          <span className="sm-field__label">Play through</span>
          <select
            className="sm-input"
            value={playback?.device ?? ""}
            disabled={!playback?.hasAudio}
            onChange={(e) => void setOutputDevice(e.target.value)}
          >
            {devices.map((d) => (
              <option key={d.name} value={d.name}>
                {d.isDante ? `${d.name} (to the PA)` : d.name}
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="transport-foot">
        <span className="text-caption muted">
          {playback && !playback.hasAudio
            ? "No audio in this recording. The timeline and the mixer below still play."
            : "To play it in the room, choose Dante Virtual Soundcard and patch it to the PA."}
        </span>
        <SendMoves lanes={lanes} />
      </div>
    </section>
  );
}

/** The guarded way to put recorded moves back on the console. */
function SendMoves({ lanes }: { lanes: Lane[] }) {
  const role = useMixer((s) => s.session?.role);
  const consoleName = useConsoleName();
  const recording = useIsRecording();
  const running = useRecordings((s) => s.replay?.state === "running");
  const [open, setOpen] = useState(false);

  if (role === "volunteer") {
    return <span className="text-caption muted">Engineers and admins can send these moves to the console.</span>;
  }
  const reason = recording
    ? "Stop recording first. Moves can't be sent while a service is being recorded."
    : !consoleName
      ? "Connect to your console in Setup to send these moves to it."
      : running
        ? "Moves are being sent now."
        : null;

  return (
    <div className="send-moves">
      {reason && <span className="text-caption muted">{reason}</span>}
      <button className="sm-btn sm-btn--lg" disabled={!!reason} onClick={() => setOpen(true)}>
        <Send />
        Send moves to console
      </button>
      {open && consoleName && <ReplayDialog consoleName={consoleName} lanes={lanes} onClose={() => setOpen(false)} />}
    </div>
  );
}

export function ReplayDialog({ consoleName, lanes, onClose }: { consoleName: string; lanes: Lane[]; onClose(): void }) {
  const positionMs = useRecordings((s) => s.playback?.positionMs ?? 0);
  const startReplay = useRecordings((s) => s.startReplay);
  const [scope, setScope] = useState<"all" | "some">("all");
  const [picked, setPicked] = useState<Set<string>>(() => new Set(lanes.map((l) => channelKey(l.id))));
  const chosen: ChannelId[] = lanes.filter((l) => picked.has(channelKey(l.id))).map((l) => l.id);
  const blocked = scope === "some" && chosen.length === 0;

  return (
    <Dialog
      title={`Send moves to ${consoleName}?`}
      onClose={onClose}
      actions={
        <>
          <button className="sm-btn sm-btn--ghost" onClick={onClose}>
            Cancel
          </button>
          <button
            className="sm-btn sm-btn--primary"
            disabled={blocked}
            onClick={() => {
              onClose();
              void startReplay(scope === "all" ? null : chosen);
            }}
          >
            <Send />
            Send moves
          </button>
        </>
      }
    >
      <p>
        From <span className="text-readout">{formatClock(positionMs)}</span>, SanctuaryMix will move the faders and
        mutes on {consoleName} the way they moved in this service, as it plays.
      </p>
      <p className="replay-stop-note">
        <TriangleAlert size={20} strokeWidth={1.75} aria-hidden />
        <span>
          Touching the desk or changing anything in SanctuaryMix stops it at once. Auto-mix will be switched off while
          this runs.
        </span>
      </p>
      <p className="muted">The console's current faders and mutes are saved first, so you can restore them after.</p>
      <div className="sm-seg" role="group" aria-label="Channels to send">
        <button aria-pressed={scope === "all"} onClick={() => setScope("all")} data-autofocus>
          All channels
        </button>
        <button aria-pressed={scope === "some"} onClick={() => setScope("some")}>
          Only the ones I pick
        </button>
      </div>
      {scope === "some" && (
        <fieldset className="channel-picks">
          <legend className="visually-hidden">Channels</legend>
          {lanes.map((l) => {
            const key = channelKey(l.id);
            return (
              <label key={key} className="channel-pick">
                <input
                  type="checkbox"
                  checked={picked.has(key)}
                  onChange={(e) =>
                    setPicked((cur) => {
                      const next = new Set(cur);
                      if (e.target.checked) next.add(key);
                      else next.delete(key);
                      return next;
                    })
                  }
                />
                <span className="text-readout muted">{channelLabel(l.id)}</span>
                <span>{l.name}</span>
              </label>
            );
          })}
        </fieldset>
      )}
      {blocked && <p className="text-caption muted">Pick at least one channel.</p>}
    </Dialog>
  );
}

function ReplayBanner() {
  const replay = useRecordings((s) => s.replay);
  const replayError = useRecordings((s) => s.replayError);
  const openId = useRecordings((s) => s.openId);
  const consoleName = useConsoleName() ?? "the console";
  const { stopReplay, restore, clearReplayMessage } = useRecordings();

  if (replayError) {
    return (
      <div className="replay-banner" data-state="error" role="alert">
        <span className="text-body-strong">Couldn't send moves.</span>
        <span className="replay-text">{replayError}</span>
        <button className="sm-btn sm-btn--ghost" onClick={clearReplayMessage}>
          Dismiss
        </button>
      </div>
    );
  }
  if (!replay || replay.recordingId !== openId) return null;
  if (replay.state === "idle" && !replay.message) return null;
  if ((replay.state === "stopped" || replay.state === "takenOver") && !replay.message && !replay.canRestore) return null;

  const running = replay.state === "running";
  return (
    <div className="replay-banner" data-state={replay.state} role="status">
      <span className="text-body-strong">
        {running
          ? `Sending moves to ${consoleName}`
          : replay.state === "takenOver"
            ? "You took over"
            : replay.state === "stopped"
              ? "Replay stopped"
              : "Done"}
      </span>
      <span className="replay-text">
        {running
          ? `${replay.sentCount} changes sent${replay.channels ? ` on ${replay.channels.length} channels` : ""}. Touch the desk or the app to take over.`
          : replay.message}
        {running && replay.message ? ` ${replay.message}` : ""}
      </span>
      {running ? (
        <button className="sm-btn sm-btn--lg" onClick={() => void stopReplay()}>
          Stop sending
        </button>
      ) : (
        <>
          {replay.canRestore && (
            <button className="sm-btn sm-btn--lg" onClick={() => void restore()}>
              <RotateCcw />
              Restore console
            </button>
          )}
          <button className="sm-btn sm-btn--ghost sm-btn--lg" onClick={clearReplayMessage}>
            Dismiss
          </button>
        </>
      )}
    </div>
  );
}

function MixerPanel({ state, lanes, positionMs }: { state: ChannelState[]; lanes: Lane[]; positionMs: number }) {
  const [all, setAll] = useState(false);
  return (
    <section className="panel" aria-label="Mixer at the playhead">
      <div className="panel-head mini-head">
        <h2 className="text-heading">Mixer at {formatClock(positionMs)}</h2>
        <div className="sm-seg" role="group" aria-label="Channels to show">
          <button aria-pressed={!all} onClick={() => setAll(false)}>
            Channels that moved
          </button>
          <button aria-pressed={all} onClick={() => setAll(true)}>
            All inputs
          </button>
        </div>
      </div>
      <p className="text-caption muted">Read only. Nothing here goes to the console.</p>
      <MiniMixer state={state} moved={lanes} all={all} />
    </section>
  );
}

const MiniMixer = memo(function MiniMixer({ state, moved, all }: { state: ChannelState[]; moved: Lane[]; all: boolean }) {
  const movedKeys = useMemo(() => new Set(moved.map((l) => channelKey(l.id))), [moved]);
  const shown = all ? state : state.filter((s) => movedKeys.has(channelKey(s.id)));
  if (shown.length === 0) return <p className="empty">No channels to show.</p>;
  return (
    <div className="mini-bay" role="list">
      {shown.map((s) => (
        <div
          key={channelKey(s.id)}
          role="listitem"
          className="sm-strip mini-strip"
          data-muted={s.muted}
          data-moved={movedKeys.has(channelKey(s.id))}
          aria-label={`${s.name}, ${formatDb(s.faderDb)} dB${s.muted ? ", muted" : ""}`}
        >
          <div className="sm-strip__name" title={s.name}>
            <span className="sm-strip__num">{s.id.index + 1}</span>
            <span className="sm-strip__label">{s.name}</span>
          </div>
          <div className="mini-fader sm-fader" style={{ "--pos": dbToFader(s.faderDb) } as React.CSSProperties} aria-hidden>
            <div className="sm-fader-track" />
            <div className="sm-fader-unity" style={{ bottom: "75%" }} />
            <div className="sm-fader-cap" />
          </div>
          <div className="sm-strip__value">{formatDb(s.faderDb)}</div>
          <span className="mini-mute" data-on={s.muted}>
            {s.muted ? "Muted" : "Mute"}
          </span>
        </div>
      ))}
    </div>
  );
});

function EventList({ rows, positionMs, onSeek }: { rows: EventRow[]; positionMs: number; onSeek(ms: number): void }) {
  const box = useRef<HTMLDivElement>(null);
  let current = -1;
  for (let i = 0; i < rows.length && rows[i].tMs <= positionMs; i++) current = i;

  useEffect(() => {
    const el = box.current;
    const row = el?.querySelector<HTMLElement>(`[data-row="${current}"]`);
    if (!el || !row) return;
    const top = row.offsetTop - el.clientHeight / 2;
    if (row.offsetTop < el.scrollTop || row.offsetTop > el.scrollTop + el.clientHeight - row.offsetHeight) {
      el.scrollTop = Math.max(0, top);
    }
  }, [current]);

  if (rows.length === 0) return <p className="empty">No changes after the start.</p>;
  return (
    <div className="event-scroll" ref={box}>
      <table className="event-table">
        <thead>
          <tr className="text-label">
            <th>Time</th>
            <th>Channel</th>
            <th>Change</th>
            <th>By</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r, i) => (
            <tr key={r.seq} data-row={i} aria-current={i === current ? "true" : undefined} data-source={r.source}>
              <td>
                <button className="event-time text-readout" onClick={() => onSeek(r.tMs)} aria-label={`Go to ${formatClock(r.tMs)}`}>
                  {formatClock(r.tMs)}
                </button>
              </td>
              <td>
                <span className="event-channel">{r.name}</span>{" "}
                {r.id && <span className="text-readout muted">{channelLabel(r.id)}</span>}
              </td>
              <td className={r.kind === "fader" ? "text-readout" : "text-caption"}>
                {r.kind === "mute" ? (
                  <span className={r.change === "Muted" ? "event-muted" : undefined}>{r.change}</span>
                ) : (
                  r.change
                )}
              </td>
              <td className="text-caption">
                {r.source === "assist" ? <span className="sm-assist-badge">Assist</span> : SOURCE_LABEL[r.source]}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
