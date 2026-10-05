import { ChevronRight, Ear, Hand, Play, PowerOff, RotateCcw, Undo2 } from "lucide-react";
import { useEffect, useState } from "react";
import { getBackend } from "../lib/backend";
import { formatDb } from "../lib/levels";
import type { Adjustment, ChannelRole, ChannelStatus, HeardChannel, Nudges, RoomFeel } from "../lib/types";
import { MODE_DETAIL, MODE_LABEL, ROLE_LABEL, SOUND_LABEL, useAutoMix } from "../store/automix";
import { useMixer } from "../store/mixer";

const ROLES = Object.keys(ROLE_LABEL) as ChannelRole[];
const VOCAL_ROLES: ChannelRole[] = ["speech", "leadVocal", "backingVocal", "choir"];

const signed = (db: number) => `${formatDb(db)} dB`;

function time(ms: number): string {
  return new Date(ms).toLocaleTimeString([], {
    hour: "numeric",
    minute: "2-digit",
    second: "2-digit",
  });
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
    ? "Connect the console in Setup."
    : !audioOn
      ? "Start Dante audio in Setup."
      : count === 0
        ? "Pick channels below."
        : null;

  const state = !engaged
    ? "Off"
    : frozen
      ? "Frozen"
      : `Riding ${count} ${count === 1 ? "channel" : "channels"} · Esc to freeze`;

  return (
    <section className="panel automix-panel" aria-labelledby="automix-title">
      <div className="automix-text">
        <div className="panel-head">
          <h2 id="automix-title" className="text-heading">
            Auto-mix
          </h2>
          {engaged && <span className="sm-assist-badge">{frozen ? "Frozen" : "Auto"}</span>}
        </div>
        <span
          className="muted automix-state"
          role="status"
          title="Rides only the channels you pick, in small steps. Never mutes. Move a fader to take it back."
        >
          {!engaged && blocker ? blocker : state}
        </span>
        {error && <p className="error">{error}</p>}
      </div>
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
      </div>
    </section>
  );
}

/** The six room feels from the research, as a dropdown with the chosen one explained below it. */
export function RoomFeelPicker() {
  const presets = useAutoMix((s) => s.presets);
  const feel = useAutoMix((s) => s.config?.feel);
  const setFeel = useAutoMix((s) => s.setFeel);
  const isAdmin = useMixer((s) => s.session?.role === "admin");
  const chosen = presets.find((p) => p.feel === feel);

  return (
    <section className="section" aria-labelledby="feel-title">
      <div className="section-head">
        <h2 id="feel-title">Room feel</h2>
      </div>
      <div className="group">
        <label className="row">
          <span className="row-text">Feel</span>
          <select
            className="sm-input"
            value={feel ?? ""}
            disabled={presets.length === 0}
            onChange={(e) => void setFeel(e.target.value as RoomFeel)}
          >
            {presets.map((p) => (
              <option key={p.feel} value={p.feel} disabled={p.adminOnly && !isAdmin}>
                {p.name}
                {p.adminOnly ? " (admins only)" : ""}
              </option>
            ))}
          </select>
        </label>
        <NudgeSliders />
      </div>
      {chosen && (
        <p className="section-foot" title={`Room level ${chosen.roomLevel}`}>
          {chosen.description}
        </p>
      )}
      {chosen?.warning && <p className="section-foot feel-warning">{chosen.warning}</p>}
    </section>
  );
}

const NUDGES: { key: keyof Nudges; label: string; help: string }[] = [
  {
    key: "loudnessDb",
    label: "Loudness",
    help: "Moves the whole mix up or down.",
  },
  { key: "lowEndDb", label: "Low end", help: "More or less kick and bass." },
  {
    key: "vocalPresenceDb",
    label: "Vocal presence",
    help: "Lifts the voices over the band.",
  },
];

function NudgeSliders() {
  const nudges = useAutoMix((s) => s.config?.nudges);
  const setNudge = useAutoMix((s) => s.setNudge);
  if (!nudges) return null;
  return (
    <>
      {NUDGES.map(({ key, label, help }) => (
        <label key={key} className="row nudge" title={help}>
          <span className="nudge-label">{label}</span>
          <input
            type="range"
            min={-3}
            max={3}
            step={0.5}
            value={nudges[key]}
            aria-valuetext={signed(nudges[key])}
            onChange={(e) => void setNudge(key, Number(e.target.value))}
          />
          <span className="text-readout nudge-value">{signed(nudges[key])}</span>
        </label>
      ))}
    </>
  );
}

/** Which channels auto-mix may ride, and what each one is. */
export function ChannelPicker() {
  const strips = useMixer((s) => s.strips);
  const managed = useAutoMix((s) => s.config?.channels ?? []);
  const listen = useAutoMix((s) => s.config?.listen ?? false);
  const statuses = useAutoMix((s) => s.status?.channels ?? []);
  const scanResults = useAutoMix((s) => s.scan.results);
  const { setManaged, setRole, setListen, undo, resumeChannel, applySuggestion } = useAutoMix();

  const roleOf = new Map(managed.map((c) => [c.channel, c.role]));
  const statusOf = new Map(statuses.map((c) => [c.channel, c]));
  const heardOf = new Map(scanResults.map((h) => [h.channel, h]));
  // The live columns stay hidden until auto-mix or a listen has something to put in them.
  const live = statuses.length > 0 || scanResults.length > 0;

  async function pickVoices() {
    const roles = await (await getBackend()).automixGuessRoles(strips.map((s) => s.name));
    const voices = strips.filter((_, i) => VOCAL_ROLES.includes(roles[i])).map((s) => s.index);
    await setManaged(voices, true);
  }

  return (
    <section className="section" aria-labelledby="channels-title">
      <div className="section-head">
        <h2 id="channels-title">Channels</h2>
        <span className="text-caption muted">
          {managed.length} of {strips.length} picked
        </span>
      </div>
      <div className="group channels-group">
        <div className="toolbar">
          <button className="sm-btn" onClick={() => void pickVoices()}>
            Select vocals and speech
          </button>
          <ListenForRolesButton />
          <button
            className="sm-btn sm-btn--ghost"
            disabled={managed.length === 0}
            onClick={() =>
              void setManaged(
                managed.map((c) => c.channel),
                false,
              )
            }
          >
            Clear
          </button>
        </div>
        <label
          className="am-listen"
          title="Tells a real voice from band bleed, so bleed is never turned up. Runs on this computer only."
        >
          <input type="checkbox" checked={listen} onChange={(e) => void setListen(e.target.checked)} />
          <span className="text-body-strong">Listen to each mic</span>
        </label>
        <div className="am-table-scroll">
          <table className="am-table">
            <thead>
              <tr>
                <th className="text-label">Auto</th>
                <th className="text-label">Channel</th>
                <th className="text-label">What it is</th>
                {live && (
                  <>
                    <th className="text-label">Hears</th>
                    <th className="text-label">Now</th>
                    <th className="text-label">Fader</th>
                    <th>
                      <span className="visually-hidden">Actions</span>
                    </th>
                  </>
                )}
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
                    {live && (
                      <>
                        <td>
                          <HeardCell
                            status={st}
                            scanned={heardOf.get(strip.index)}
                            name={strip.name}
                            role={role}
                            apply={applySuggestion}
                          />
                        </td>
                        <td>{st && <ModeWord status={st} />}</td>
                        <td className="text-readout">{st && <FaderChange status={st} />}</td>
                        <td className="am-row-actions">
                          {st && <ChannelActions status={st} undo={undo} resume={resumeChannel} compact />}
                        </td>
                      </>
                    )}
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}

/** Listens to every input for a while and suggests roles from what it hears. */
function ListenForRolesButton() {
  const until = useAutoMix((s) => s.scan.until);
  const listenForRoles = useAutoMix((s) => s.listenForRoles);
  const audioOn = useMixer((s) => s.audioStatus === "on");
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (until === null) return;
    const timer = setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(timer);
  }, [until]);

  if (until !== null) {
    const left = Math.max(0, Math.ceil((until - now) / 1000));
    return (
      <button className="sm-btn am-listening" disabled aria-live="polite">
        <Ear />
        Listening, {left} s
      </button>
    );
  }
  return (
    <button
      className="sm-btn"
      disabled={!audioOn}
      title={audioOn ? "Play or sing through every mic while it listens." : "Start Dante audio in Setup first."}
      onClick={() => void listenForRoles()}
    >
      <Ear />
      Listen and suggest roles
    </button>
  );
}

/** What the listening models hear on a channel, and a role to apply when it doesn't fit. */
function HeardCell({
  status,
  scanned,
  name,
  role,
  apply,
}: {
  status: ChannelStatus | undefined;
  scanned: HeardChannel | undefined;
  name: string;
  role: ChannelRole | undefined;
  apply(channel: number, role: ChannelRole): Promise<void>;
}) {
  const live = status?.heard ?? null;
  const suggestion = scanned?.suggestedRole ?? null;
  if (live === null && !scanned) return null;
  return (
    <span className="am-heard">
      {live !== null ? (
        <span className="am-heard-word" title="What the listening models hear on this mic right now.">
          {SOUND_LABEL[live]}
        </span>
      ) : (
        scanned && (
          <span
            className="am-heard-word"
            title={`Mostly this (${Math.round(scanned.share * 100)}% of what it heard) during the last listen.`}
          >
            {SOUND_LABEL[scanned.sound]}
          </span>
        )
      )}
      {suggestion && scanned && (
        <button
          className="sm-btn sm-btn--sm am-suggest"
          title={`${name} sounds like ${SOUND_LABEL[scanned.sound].toLowerCase()}${
            role ? `, not ${ROLE_LABEL[role].toLowerCase()}` : ""
          }.`}
          onClick={() => void apply(scanned.channel, suggestion)}
        >
          Use {ROLE_LABEL[suggestion]}
          <span className="visually-hidden"> for {name}</span>
        </button>
      )}
    </span>
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
  compact = false,
}: {
  status: ChannelStatus;
  undo(channel: number): Promise<void>;
  resume(channel: number): Promise<void>;
  /** Icon-only Undo, for the channel table where width is tight. */
  compact?: boolean;
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
  if (compact) {
    return (
      <button
        className="sm-btn sm-btn--sm icon-btn-sm"
        onClick={() => void undo(status.channel)}
        aria-label={`Undo auto-mix on ${name}`}
        title="Undo"
      >
        <Undo2 />
      </button>
    );
  }
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
    <section className="section" aria-labelledby="activity-title">
      <div className="section-head">
        <h2 id="activity-title">What auto-mix did</h2>
      </div>
      {log.length === 0 ? (
        <div className="group">
          <div className="row empty">No moves yet.</div>
        </div>
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
    <details className="panel disclosure">
      <summary>
        <h2 className="text-heading">Auto-mix rules</h2>
        <ChevronRight size={20} strokeWidth={1.75} aria-hidden />
      </summary>
      <ul className="rules disclosure-body">
        <li>
          Music moves at most {n(g.music.maxStepDb)} dB at a time and {n(g.music.maxRateDbPerSec)} dB a second. Speech
          moves at most {n(g.speech.maxStepDb)} dB at a time and {n(g.speech.maxRateDbPerSec)} dB a second.
        </li>
        <li>
          It stays within {n(g.music.maxCutDb)} dB below and {n(g.music.maxBoostDb)} dB above where you set a music
          channel ({n(g.speech.maxCutDb)} below and {n(g.speech.maxBoostDb)} above for speech).
        </li>
        <li>
          It never pushes a fader above 0 dB unless you already had it there, and never pulls one all the way down.
        </li>
        <li>It never mutes or unmutes, and never touches gain, routing, scenes or channels you didn't pick.</li>
        <li>It never raises a mic nobody is using, and leaves a clipping input for you to fix at the preamp.</li>
        <li>
          While it listens, a voice mic that only hears the band is left alone: it isn't turned up, and it doesn't make
          the band step back.
        </li>
        <li>
          Move a fader it's riding and that channel is yours until you hand it back. Freeze stops everything at once.
        </li>
        <li>Every move is logged on this computer with the reason.</li>
      </ul>
    </details>
  );
}
