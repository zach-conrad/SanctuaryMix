import { Hand, Play, PowerOff, RotateCcw, Undo2 } from "lucide-react";
import { getBackend } from "../lib/backend";
import { formatDb } from "../lib/levels";
import type { Adjustment, ChannelRole, ChannelStatus, Nudges } from "../lib/types";
import { MODE_DETAIL, MODE_LABEL, ROLE_LABEL, useAutoMix } from "../store/automix";
import { useMixer } from "../store/mixer";

const ROLES = Object.keys(ROLE_LABEL) as ChannelRole[];
const VOCAL_ROLES: ChannelRole[] = ["speech", "leadVocal", "backingVocal", "choir"];

const signed = (db: number) => `${formatDb(db)} dB`;

function time(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit", second: "2-digit" });
}

/** Start, stop, freeze and undo-all, with the reason when it can't start. */
export function AutoMixControls() {
  const status = useAutoMix((s) => s.status);
  const config = useAutoMix((s) => s.config);
  const error = useAutoMix((s) => s.error);
  const { engage, freeze, resume, undoAll } = useAutoMix();
  const consoleOn = useMixer((s) => s.consoleStatus === "on");
  const audioOn = useMixer((s) => s.audioStatus === "on");

  const engaged = status?.engaged ?? false;
  const frozen = status?.frozen ?? false;
  const count = config?.channels.length ?? 0;
  const blocker = !consoleOn
    ? "Connect to the console in Setup first."
    : !audioOn
      ? "Start Dante audio in Setup first."
      : count === 0
        ? "Pick at least one channel below."
        : null;

  const state = !engaged
    ? "Off. Nothing moves until you start it."
    : frozen
      ? "Frozen. Nothing moves until you resume."
      : `Riding ${count} ${count === 1 ? "channel" : "channels"}. Move any of them yourself and it lets go of that one. Press Esc to freeze everything.`;

  return (
    <section className="panel" aria-labelledby="automix-title">
      <div className="panel-head">
        <h2 id="automix-title" className="text-heading">
          Auto-mix
        </h2>
        {engaged && <span className="sm-assist-badge">{frozen ? "Frozen" : "Auto"}</span>}
      </div>
      <p className="muted">
        Keeps the channels you pick at a steady, balanced level every week. It only moves those faders, in small
        steps, and never mutes or unmutes anything.
      </p>
      <div className="automix-actions">
        {!engaged ? (
          <button className="sm-btn sm-btn--primary sm-btn--lg" disabled={!!blocker} onClick={() => void engage(true)}>
            <Play />
            Start auto-mix
          </button>
        ) : (
          <>
            {frozen ? (
              <button className="sm-btn sm-btn--primary sm-btn--lg" onClick={() => void resume()}>
                <Play />
                Resume
              </button>
            ) : (
              <button className="sm-btn sm-btn--lg" onClick={() => void freeze()}>
                <Hand />
                Freeze
              </button>
            )}
            <button className="sm-btn sm-btn--lg" onClick={() => void undoAll()}>
              <Undo2 />
              Undo all
            </button>
            <button className="sm-btn sm-btn--ghost sm-btn--lg" onClick={() => void engage(false)}>
              <PowerOff />
              Stop auto-mix
            </button>
          </>
        )}
        <span className="muted automix-state" role="status">
          {!engaged && blocker ? blocker : state}
        </span>
      </div>
      {error && <p className="error">{error}</p>}
    </section>
  );
}

/** The six room feels from the research, as cards. */
export function RoomFeelPicker() {
  const presets = useAutoMix((s) => s.presets);
  const feel = useAutoMix((s) => s.config?.feel);
  const setFeel = useAutoMix((s) => s.setFeel);
  const isAdmin = useMixer((s) => s.session?.role === "admin");

  return (
    <section className="panel" aria-labelledby="feel-title">
      <h2 id="feel-title" className="text-heading">
        How should the room feel?
      </h2>
      <div className="feel-grid" role="radiogroup" aria-labelledby="feel-title">
        {presets.map((p) => {
          const locked = p.adminOnly && !isAdmin;
          return (
            <button
              key={p.feel}
              role="radio"
              aria-checked={feel === p.feel}
              className="feel-card"
              disabled={locked}
              onClick={() => void setFeel(p.feel)}
            >
              <span className="text-body-strong">{p.name}</span>
              <span className="text-caption muted">{p.description}</span>
              <span className="text-caption muted">
                Room level {p.roomLevel}
                {p.adminOnly ? " · Admins only" : ""}
              </span>
              {p.warning && feel === p.feel && <span className="text-caption feel-warning">{p.warning}</span>}
            </button>
          );
        })}
      </div>
      <NudgeSliders />
      <p className="text-caption muted">
        Room levels are a guide. Without a measurement mic, auto-mix keeps the balance between channels and leaves
        the main fader to you.
      </p>
    </section>
  );
}

const NUDGES: { key: keyof Nudges; label: string; help: string }[] = [
  { key: "loudnessDb", label: "Loudness", help: "Moves the whole mix up or down." },
  { key: "lowEndDb", label: "Low end", help: "More or less kick and bass." },
  { key: "vocalPresenceDb", label: "Vocal presence", help: "Lifts the voices over the band." },
];

function NudgeSliders() {
  const nudges = useAutoMix((s) => s.config?.nudges);
  const setNudge = useAutoMix((s) => s.setNudge);
  if (!nudges) return null;
  return (
    <div className="nudges">
      {NUDGES.map(({ key, label, help }) => (
        <label key={key} className="nudge">
          <span className="nudge-head">
            <span className="text-body-strong">{label}</span>
            <span className="text-readout">{signed(nudges[key])}</span>
          </span>
          <input
            type="range"
            min={-3}
            max={3}
            step={0.5}
            value={nudges[key]}
            aria-valuetext={signed(nudges[key])}
            onChange={(e) => void setNudge(key, Number(e.target.value))}
          />
          <span className="text-caption muted">{help}</span>
        </label>
      ))}
    </div>
  );
}

/** Which channels auto-mix may ride, and what each one is. */
export function ChannelPicker() {
  const strips = useMixer((s) => s.strips);
  const managed = useAutoMix((s) => s.config?.channels ?? []);
  const statuses = useAutoMix((s) => s.status?.channels ?? []);
  const { setManaged, setRole, undo, resumeChannel } = useAutoMix();

  const roleOf = new Map(managed.map((c) => [c.channel, c.role]));
  const statusOf = new Map(statuses.map((c) => [c.channel, c]));

  async function pickVoices() {
    const roles = await (await getBackend()).automixGuessRoles(strips.map((s) => s.name));
    const voices = strips.filter((_, i) => VOCAL_ROLES.includes(roles[i])).map((s) => s.index);
    await setManaged(voices, true);
  }

  return (
    <section className="panel" aria-labelledby="channels-title">
      <div className="panel-head">
        <h2 id="channels-title" className="text-heading">
          Channels auto-mix may ride
        </h2>
        <button className="sm-btn" onClick={() => void pickVoices()}>
          Select vocals and speech
        </button>
        <button
          className="sm-btn sm-btn--ghost"
          disabled={managed.length === 0}
          onClick={() => void setManaged(managed.map((c) => c.channel), false)}
        >
          Clear
        </button>
      </div>
      <p className="muted">
        Start with vocals and speech. Leave out room mics and anything you want to ride yourself. Channels you don't
        pick are never touched.
      </p>
      <table className="am-table">
        <thead>
          <tr>
            <th className="text-label">Auto</th>
            <th className="text-label">Channel</th>
            <th className="text-label">What it is</th>
            <th className="text-label">Now</th>
            <th className="text-label">Fader</th>
            <th>
              <span className="visually-hidden">Actions</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {strips.map((strip) => {
            const role = roleOf.get(strip.index);
            const st = statusOf.get(strip.index);
            return (
              <tr key={strip.index} data-managed={role !== undefined}>
                <td>
                  <input
                    type="checkbox"
                    checked={role !== undefined}
                    aria-label={`Let auto-mix ride ${strip.name}`}
                    onChange={(e) => void setManaged([strip.index], e.target.checked)}
                  />
                </td>
                <td>
                  <span className="text-readout muted">Ch {strip.index + 1}</span> {strip.name}
                </td>
                <td>
                  {role !== undefined && (
                    <select
                      className="sm-input"
                      value={role}
                      aria-label={`What ${strip.name} is`}
                      onChange={(e) => void setRole(strip.index, e.target.value as ChannelRole)}
                    >
                      {ROLES.map((r) => (
                        <option key={r} value={r}>
                          {ROLE_LABEL[r]}
                        </option>
                      ))}
                    </select>
                  )}
                </td>
                <td>{st && <ModeWord status={st} />}</td>
                <td className="text-readout">{st && <FaderChange status={st} />}</td>
                <td className="am-row-actions">{st && <ChannelActions status={st} undo={undo} resume={resumeChannel} />}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </section>
  );
}

export function ModeWord({ status }: { status: ChannelStatus }) {
  return (
    <span className="am-mode" data-mode={status.mode} title={MODE_DETAIL[status.mode]}>
      {MODE_LABEL[status.mode]}
    </span>
  );
}

/** "−10.0 → −8.5 dB" when auto-mix has moved it, else the position. */
export function FaderChange({ status }: { status: ChannelStatus }) {
  const { faderDb, baselineDb } = status;
  if (faderDb === null) return <span className="muted">Unknown</span>;
  if (baselineDb !== null && Math.abs(faderDb - baselineDb) >= 0.25 && status.mode !== "heldByOperator") {
    return (
      <span className="am-change">
        {formatDb(baselineDb)} → {formatDb(faderDb)} dB
      </span>
    );
  }
  return <span>{signed(faderDb)}</span>;
}

export function ChannelActions({
  status,
  undo,
  resume,
}: {
  status: ChannelStatus;
  undo(channel: number): Promise<void>;
  resume(channel: number): Promise<void>;
}) {
  const name = status.name ?? `Ch ${status.channel + 1}`;
  if (status.mode === "heldByOperator" || status.mode === "undone") {
    return (
      <button className="sm-btn sm-btn--sm" onClick={() => void resume(status.channel)}>
        <RotateCcw />
        Hand back
        <span className="visually-hidden"> {name} to auto-mix</span>
      </button>
    );
  }
  const moved =
    status.faderDb !== null && status.baselineDb !== null && Math.abs(status.faderDb - status.baselineDb) >= 0.25;
  if (!moved || status.mode === "off") return null;
  return (
    <button className="sm-btn sm-btn--sm" onClick={() => void undo(status.channel)}>
      <Undo2 />
      Undo
      <span className="visually-hidden"> auto-mix on {name}</span>
    </button>
  );
}

const NOTE_KINDS = new Set<Adjustment["kind"]>(["engaged", "disengaged", "frozen", "resumed"]);

/** Every move and takeover, newest first. Stored on this computer. */
export function ActivityLog({ limit = 40 }: { limit?: number }) {
  const log = useAutoMix((s) => s.log);
  const undo = useAutoMix((s) => s.undo);
  const statuses = useAutoMix((s) => s.status?.channels ?? []);
  const undoable = new Set(
    statuses
      .filter((c) => c.mode !== "undone" && c.mode !== "heldByOperator" && c.mode !== "off")
      .map((c) => c.channel),
  );
  // Only the newest move on a channel offers Undo (it puts the whole channel back).
  const seen = new Set<number>();

  return (
    <section className="panel" aria-labelledby="activity-title">
      <h2 id="activity-title" className="text-heading">
        What auto-mix did
      </h2>
      {log.length === 0 ? (
        <p className="empty">Nothing yet. Every move shows up here with the reason, and you can undo it.</p>
      ) : (
        <ol className="activity">
          {log.slice(0, limit).map((a, i) => {
            const offerUndo =
              a.kind === "auto" && a.channel !== null && !seen.has(a.channel) && undoable.has(a.channel);
            if (a.channel !== null) seen.add(a.channel);
            if (NOTE_KINDS.has(a.kind)) {
              return (
                <li key={`${a.atMs}-${i}`} className="activity-note text-caption muted">
                  <span className="text-readout">{time(a.atMs)}</span> {a.reason}
                </li>
              );
            }
            return (
              <li key={`${a.atMs}-${i}`} className={a.kind === "auto" ? "sm-assist activity-move" : "activity-move"}>
                <span className="sm-assist__head">
                  <span className="text-body-strong">
                    {a.channelName ?? (a.channel !== null ? `Ch ${a.channel + 1}` : "")}
                  </span>
                  <span className="sm-assist__time">{time(a.atMs)}</span>
                </span>
                {a.fromDb !== null && a.toDb !== null && (
                  <span className={a.kind === "auto" ? "sm-assist__change" : "text-readout"}>
                    {formatDb(a.fromDb)} → {formatDb(a.toDb)} dB
                  </span>
                )}
                <span className="sm-assist__why">{a.reason}</span>
                {offerUndo && (
                  <span className="sm-assist__actions">
                    <button className="sm-btn sm-btn--sm" onClick={() => void undo(a.channel!)}>
                      <Undo2 />
                      Undo
                    </button>
                  </span>
                )}
              </li>
            );
          })}
        </ol>
      )}
    </section>
  );
}

/** The guardrails in plain words, straight from the core's settings. */
export function RulesList() {
  const g = useAutoMix((s) => s.config?.guardrails);
  if (!g) return null;
  const n = (v: number) => formatDb(v).replace("+", "");
  return (
    <section className="panel" aria-labelledby="rules-title">
      <h2 id="rules-title" className="text-heading">
        Rules auto-mix always follows
      </h2>
      <ul className="rules">
        <li>
          Music moves at most {n(g.music.maxStepDb)} dB at a time and {n(g.music.maxRateDbPerSec)} dB a second. Speech
          moves at most {n(g.speech.maxStepDb)} dB at a time and {n(g.speech.maxRateDbPerSec)} dB a second.
        </li>
        <li>
          It stays within {n(g.music.maxCutDb)} dB below and {n(g.music.maxBoostDb)} dB above where you set a music
          channel ({n(g.speech.maxCutDb)} below and {n(g.speech.maxBoostDb)} above for speech).
        </li>
        <li>It never pushes a fader above 0 dB unless you already had it there, and never pulls one all the way down.</li>
        <li>It never mutes or unmutes, and never touches gain, routing, scenes or channels you didn't pick.</li>
        <li>It never raises a mic nobody is using, and leaves a clipping input for you to fix at the preamp.</li>
        <li>Move a fader it's riding and that channel is yours until you hand it back. Freeze stops everything at once.</li>
        <li>Every move is logged on this computer with the reason.</li>
      </ul>
    </section>
  );
}
