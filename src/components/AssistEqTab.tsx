import { ChevronRight, Ear, RotateCcw } from "lucide-react";
import { eqStateWord } from "../lib/aieq";
import { can, lockReason } from "../lib/plans";
import type { EqChannelStatus } from "../lib/types";
import { eqBlocker, useAiEq } from "../store/aieq";
import { ROLE_LABEL } from "../store/automix";
import { useMixer } from "../store/mixer";
import { EqGraph } from "./EqGraph";

/** A stable empty list, so store selectors don't return a new array every time. */
const NONE: never[] = [];

const MINUS = "−";

/** Assist › EQ: the on/off switch, the channels AI EQ looks after, and its limits. */
export function AssistEqTab() {
  return (
    <>
      <AiEqControls />
      <div className="page-split">
        <div className="page-main">
          <EqChannels />
        </div>
        <aside className="page-side">
          <ProfileCheck />
          <DuringService />
          <Limits />
        </aside>
      </div>
    </>
  );
}

function AiEqControls() {
  const status = useAiEq((s) => s.status);
  const config = useAiEq((s) => s.config);
  const error = useAiEq((s) => s.error);
  const setEnabled = useAiEq((s) => s.setEnabled);
  const setView = useMixer((s) => s.setView);
  const consoleOn = useMixer((s) => s.consoleStatus === "on");
  const audioOn = useMixer((s) => s.audioStatus === "on");
  const lock = useMixer((s) => lockReason(s.session, "aiEq"));

  const enabled = (status?.enabled ?? false) && !lock;
  const frozen = status?.frozen ?? false;
  const channels = status?.channels ?? [];
  const keeping = channels.filter((c) => c.mode === "keeping").length;
  const blocker =
    eqBlocker(lock) ??
    (!consoleOn
      ? "Connect the console in Setup."
      : !audioOn
        ? "Start Dante audio in Setup."
        : status && !status.eqSupported
          ? "This console's EQ can't be reached yet."
          : channels.length === 0
            ? "Pick channels in Levels."
            : null);
  const state = !enabled
    ? "Off"
    : frozen
      ? "Frozen. Resume from the top bar."
      : `Guarding ${channels.length} ${channels.length === 1 ? "channel" : "channels"}${
          keeping ? ` · keeping tone on ${keeping}` : ""
        }`;

  return (
    <section className="panel eq-mode-panel" aria-labelledby="aieq-title">
      <div className="automix-text">
        <div className="panel-head">
          <h2 id="aieq-title" className="text-heading">
            AI EQ
          </h2>
          {enabled ? (
            <span className="sm-assist-badge">{frozen ? "Frozen" : "On"}</span>
          ) : (
            <span className="eq-off-badge">Off</span>
          )}
        </div>
        <span
          className="muted automix-state"
          role="status"
          title="Cuts feedback on picked mics and keeps speech sounding the same. Nothing changes during a service except those two."
        >
          {blocker ?? state}
        </span>
        {error && <p className="error">{error}</p>}
      </div>
      <div className="automix-actions">
        <div className="sm-seg" role="group" aria-label="AI EQ" title={lock ? (eqBlocker(lock) ?? undefined) : undefined}>
          <button aria-pressed={!enabled} disabled={!!lock} onClick={() => void setEnabled(false)}>
            Off
          </button>
          <button aria-pressed={enabled} disabled={!!lock || !config} onClick={() => void setEnabled(true)}>
            On
          </button>
        </div>
        <button
          className="sm-btn sm-btn--primary sm-btn--lg"
          disabled={!!lock}
          title={lock ? (eqBlocker(lock) ?? undefined) : "Listen to each picked channel and suggest EQ"}
          onClick={() => setView("eqSoundcheck")}
        >
          <Ear />
          EQ soundcheck
        </button>
      </div>
    </section>
  );
}

function EqChannels() {
  const channels = useAiEq((s) => s.status?.channels ?? NONE);
  const enabled = useAiEq((s) => s.status?.enabled ?? false);
  const locked = useMixer((s) => lockReason(s.session, "aiEq") !== null);
  return (
    <section className="section" aria-labelledby="eq-channels-title">
      <div className="section-head">
        <h2 id="eq-channels-title">Channels</h2>
        <span className="text-caption muted">{channels.length} picked</span>
      </div>
      <div className="group">
        {channels.length === 0 ? (
          <div className="row empty">No channels picked yet. Pick them in Levels.</div>
        ) : (
          channels.map((c) => <EqChannelRow key={c.channel} status={c} enabled={enabled && !locked} />)
        )}
      </div>
      <p className="section-foot">Same channels as Levels. Pick them there.</p>
    </section>
  );
}

function EqChannelRow({ status, enabled }: { status: EqChannelStatus; enabled: boolean }) {
  const deskEq = useMixer((s) => s.strips[status.channel]?.eq ?? null);
  const setView = useMixer((s) => s.setView);
  const openPanel = useAiEq((s) => s.openPanel);
  const handBack = useAiEq((s) => s.handBack);
  const word = eqStateWord(status, enabled);
  const name = status.name ?? `Ch ${status.channel + 1}`;
  const eq = status.eq ?? deskEq;
  const open = () => {
    openPanel(status.channel);
    setView("mixer");
  };
  const yours = enabled && (status.mode === "yours" || status.mode === "undone");
  const tone = word.tone === "ai" ? "is-ai" : word.tone === "warn" ? "is-warn" : word.tone === "manual" ? "is-manual" : word.tone === "muted" ? "is-muted" : "";

  return (
    <div className="row eq-ch-row is-open" onClick={open}>
      <span className="ch-num">Ch {status.channel + 1}</span>
      <span className="row-text">
        <span>{name}</span>
        <span className="text-caption">{ROLE_LABEL[status.role]}</span>
      </span>
      <span className={`state ${tone}`} title={word.detail}>
        {word.word}
      </span>
      {yours && (
        <button
          className="sm-btn sm-btn--sm"
          onClick={(e) => {
            e.stopPropagation();
            void handBack(status.channel);
          }}
        >
          <RotateCcw />
          Hand back
          <span className="visually-hidden"> {name}</span>
        </button>
      )}
      <span className="eq-thumb">
        {eq ? <EqGraph size="thumb" eq={eq} label={`EQ curve for ${name}`} /> : null}
      </span>
      <button
        className="sm-btn sm-btn--ghost icon-btn chev-btn"
        aria-label={`Open EQ for ${name}`}
        title="Open EQ"
        onClick={(e) => {
          e.stopPropagation();
          open();
        }}
      >
        <ChevronRight />
      </button>
    </div>
  );
}

/** Sunday start: the desk's EQ differs from the saved profiles. */
function ProfileCheck() {
  const differs = useAiEq((s) => s.status?.profileDiffers ?? NONE);
  const active = useAiEq((s) => s.status?.enabled ?? false);
  const restore = useAiEq((s) => s.restoreProfile);
  const dismiss = useAiEq((s) => s.dismissProfile);
  if (!active || differs.length === 0) return null;
  return (
    <section className="section" aria-labelledby="eq-waiting-title">
      <div className="section-head">
        <h2 id="eq-waiting-title">Waiting</h2>
      </div>
      <div className="sm-assist sm-assist--wide">
        <span className="sm-assist__head">
          <span className="sm-assist-badge">Assist</span>
          <span className="sm-assist__time">
            {differs.length} {differs.length === 1 ? "channel" : "channels"}
          </span>
        </span>
        <span className="sm-assist__title">EQ differs from last Sunday</span>
        <span className="sm-assist__why">Someone may have loaded an older scene. Check by ear before restoring.</span>
        <span className="sm-assist__actions">
          <button className="sm-btn" onClick={() => void restore()}>
            Restore Sunday EQ
          </button>
          <button className="sm-btn sm-btn--ghost" onClick={() => void dismiss()}>
            Keep
          </button>
        </span>
      </div>
    </section>
  );
}

function DuringService() {
  const enabled = useAiEq((s) => s.status?.enabled ?? false);
  const toneKeeping = useAiEq((s) => s.config?.toneKeeping ?? false);
  const setToneKeeping = useAiEq((s) => s.setToneKeeping);
  const canEdit = useMixer((s) => can(s.session, "changeAutoMixSetup"));
  const locked = useMixer((s) => lockReason(s.session, "aiEq") !== null);
  const disabled = !canEdit || locked;
  return (
    <section className="section" aria-labelledby="eq-service-title">
      <div className="section-head">
        <h2 id="eq-service-title">During the service</h2>
      </div>
      <div className="group">
        <div
          className="row"
          title="Cuts ringing on any picked mic with band 4 and pulls its fader down. Runs while AI EQ is on, until you freeze."
        >
          <span className="row-text">Feedback guard</span>
          <span className={`text-caption ${enabled ? "state-ai" : "muted"}`}>{enabled ? "Always on" : "Off"}</span>
        </div>
        <label
          className="row check-row"
          title={
            locked
              ? "AI EQ is in Pro. See Settings."
              : canEdit
                ? "Small, slow moves on speech mics only, so the pastor sounds the same every week."
                : "Engineers and admins can change this."
          }
        >
          <span className="row-text">Keep speech tone</span>
          <input
            type="checkbox"
            className="check-box"
            checked={toneKeeping}
            disabled={disabled}
            onChange={(e) => void setToneKeeping(e.target.checked)}
          />
        </label>
      </div>
      <p className="section-foot">Music and singers never change during a service.</p>
    </section>
  );
}

/** The core's hard limits, shown so people can trust what it may do. */
function Limits() {
  return (
    <section className="section" aria-labelledby="eq-limits-title">
      <div className="section-head">
        <h2 id="eq-limits-title">Limits</h2>
      </div>
      <div className="group">
        <div className="row" title="Above the channel's soundcheck EQ, on any band.">
          <span className="row-text">Largest boost</span>
          <span className="limit-value">+3.0 dB</span>
        </div>
        <div className="row" title="Below the channel's soundcheck EQ, for tone.">
          <span className="row-text">Largest cut</span>
          <span className="limit-value">{MINUS}6.0 dB</span>
        </div>
        <div className="row" title="The deepest cut band 4 makes on a ringing frequency.">
          <span className="row-text">Feedback notch</span>
          <span className="limit-value">{MINUS}9.0 dB</span>
        </div>
      </div>
    </section>
  );
}
