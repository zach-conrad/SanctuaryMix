import { Check, ChevronLeft, Ear } from "lucide-react";
import { useState } from "react";
import { Dialog } from "../components/Dialog";
import { EQ_GROUPS, roleGroup, soundcheckAsk, soundcheckWord } from "../lib/aieq";
import { formatGain, formatHz } from "../lib/eqCurve";
import { can, lockReason } from "../lib/plans";
import type { EqChannelStatus, SoundcheckChannel } from "../lib/types";
import { eqBlocker, useAiEq } from "../store/aieq";
import { ROLE_LABEL } from "../store/automix";
import { useMixer } from "../store/mixer";

/** A stable empty list, so store selectors don't return a new array every time. */
const NONE: never[] = [];

const HEARD: SoundcheckChannel["state"][] = ["done", "soundsGood", "applied", "skipped"];

/** EQ soundcheck: listens to each picked channel in role order and proposes EQ you apply with one tap. */
export function EqSoundcheckView() {
  const setView = useMixer((s) => s.setView);
  const setTab = useAiEq((s) => s.setAssistTab);
  const keepMyEq = useAiEq((s) => s.keepMyEq);
  const engineer = useMixer((s) => can(s.session, "changeAutoMixSetup"));
  const error = useAiEq((s) => s.error);

  return (
    <div className="page page--narrow">
      <div className="page-head">
        <div className="page-head-text">
          <button
            className="sm-btn sm-btn--ghost sm-btn--sm back-link"
            onClick={() => {
              setTab("eq");
              setView("assist");
            }}
          >
            <ChevronLeft />
            Assist
          </button>
          <h1 className="text-title">EQ soundcheck</h1>
        </div>
        <div className="page-head-actions">
          <button
            className="sm-btn sm-btn--ghost"
            disabled={!engineer}
            title={
              engineer
                ? "Use the console's EQ as it is. AI EQ only keeps band 4 for feedback."
                : "Engineers and admins can keep the console's EQ."
            }
            onClick={() => void keepMyEq()}
          >
            Keep my EQ
          </button>
        </div>
      </div>
      <TopPanel />
      {error && <p className="error">{error}</p>}
      <Groups />
      <FeedbackCheck />
    </div>
  );
}

/** Tells one person what to do now, in big type. */
function TopPanel() {
  const status = useAiEq((s) => s.status);
  const lock = useMixer((s) => lockReason(s.session, "aiEq"));
  const consoleOn = useMixer((s) => s.consoleStatus === "on");
  const { soundcheckStart, soundcheckStop, applyAll, skip } = useAiEq.getState();

  const channels = status?.channels ?? [];
  const sc = status?.soundcheck ?? null;
  const rows = sc?.channels ?? [];
  const byChannel = new Map(channels.map((c) => [c.channel, c]));
  const listening = rows.filter((r) => r.state === "listening");
  const first = listening[0] ? byChannel.get(listening[0].channel) : undefined;
  const heard = rows.filter((r) => HEARD.includes(r.state)).length;
  const ready = rows.filter((r) => r.state === "done").reduce((n, r) => n + r.changes, 0);
  const total = rows.length || channels.length;
  const inService = status?.inService ?? false;
  const blocker =
    eqBlocker(lock) ??
    (!consoleOn
      ? "Connect the console in Setup."
      : inService
        ? "Soundcheck waits until the service ends. Stop recording and auto-mix first."
        : channels.length === 0
          ? "Pick channels in Assist › Levels first."
          : null);

  const applyButton = (
    <button
      className="sm-btn sm-btn--primary sm-btn--lg"
      disabled={ready === 0 || inService}
      onClick={() => void applyAll()}
    >
      <Check />
      {ready === 0 ? "Apply changes" : `Apply ${ready} ${ready === 1 ? "change" : "changes"}`}
    </button>
  );

  let title: string;
  let line: string;
  let badge: string | null = null;
  let actions;
  if (sc?.running && first) {
    title = `${first.name ?? `Ch ${first.channel + 1}`}, ${soundcheckAsk(first.role)}`;
    line = "About 20 seconds each. Nothing changes until you apply.";
    badge = "Listening";
    actions = (
      <>
        <button className="sm-btn sm-btn--lg" onClick={() => void skip(first.channel)}>
          Skip
        </button>
        {applyButton}
      </>
    );
  } else if (sc && rows.length > 0 && !rows.some((r) => r.state === "upNext" || r.state === "listening")) {
    title = ready > 0 ? "All channels heard" : "Soundcheck done";
    line = ready > 0 ? "Review each change, or apply them all." : "Nothing waiting. Run it again any time before the service.";
    actions = (
      <>
        <button className="sm-btn sm-btn--lg" disabled={!!blocker} onClick={() => void soundcheckStart()}>
          Start again
        </button>
        {applyButton}
      </>
    );
  } else if (sc?.running) {
    title = "Getting ready";
    line = "About 20 seconds each. Nothing changes until you apply.";
    actions = applyButton;
  } else {
    const firstUp = [...channels].sort((a, b) => roleGroup(a.role) - roleGroup(b.role) || a.channel - b.channel)[0];
    title = firstUp ? `Start with ${firstUp.name ?? `Ch ${firstUp.channel + 1}`}` : "EQ soundcheck";
    line = blocker ?? "Each person speaks or sings for about 20 seconds. Nothing changes until you apply.";
    actions = (
      <>
        {sc && rows.length > 0 && ready > 0 && applyButton}
        <button
          className={`sm-btn sm-btn--lg${sc && ready > 0 ? "" : " sm-btn--primary"}`}
          disabled={!!blocker}
          title={blocker ?? undefined}
          onClick={() => void soundcheckStart()}
        >
          <Ear />
          {sc && rows.length > 0 ? "Keep listening" : "Start listening"}
        </button>
      </>
    );
  }

  return (
    <section className="panel tune-panel" aria-label="Now" aria-live="polite">
      <div className="automix-text">
        <div className="panel-head">
          <h2 className="text-heading">{title}</h2>
          {badge && <span className="sm-assist-badge">{badge}</span>}
        </div>
        <span className="muted">{line}</span>
        {(sc?.running || heard > 0) && total > 0 && (
          <>
            <div
              className="tune-progress"
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={total}
              aria-valuenow={heard}
              aria-label="Channels heard"
              style={{ "--p": `${(heard / total) * 100}%`, marginTop: "var(--space-2)" } as React.CSSProperties}
            >
              <span />
            </div>
            <span className="text-caption muted">
              {heard} of {total} channels heard
            </span>
          </>
        )}
        {sc?.running && (
          <button className="sm-btn sm-btn--ghost sm-btn--sm back-link" onClick={() => void soundcheckStop()}>
            Stop listening
          </button>
        )}
      </div>
      <div className="automix-actions">{actions}</div>
    </section>
  );
}

function Groups() {
  const channels = useAiEq((s) => s.status?.channels ?? NONE);
  const rows = useAiEq((s) => s.status?.soundcheck?.channels ?? NONE);
  const rowOf = new Map(rows.map((r) => [r.channel, r]));
  return (
    <>
      {EQ_GROUPS.map((g) => {
        const inGroup = channels.filter((c) => g.roles.includes(c.role));
        if (inGroup.length === 0) return null;
        return (
          <section key={g.label} className="section" aria-label={g.label}>
            <div className="section-head">
              <h2>{g.label}</h2>
            </div>
            <div className="group">
              {inGroup.map((c) => (
                <SoundcheckRow key={c.channel} status={c} row={rowOf.get(c.channel)} />
              ))}
            </div>
          </section>
        );
      })}
    </>
  );
}

function SoundcheckRow({ status, row }: { status: EqChannelStatus; row: SoundcheckChannel | undefined }) {
  const setView = useMixer((s) => s.setView);
  const openPanel = useAiEq((s) => s.openPanel);
  const name = status.name ?? `Ch ${status.channel + 1}`;
  const word = row
    ? soundcheckWord(row)
    : status.mode === "notChecked"
      ? { word: "Not checked yet", tone: "warn" as const, detail: "Start listening to check it." }
      : { word: "Set last time", tone: "plain" as const, detail: "Holding last soundcheck's EQ." };
  const tone = word.tone === "ai" ? "is-ai" : word.tone === "warn" ? "is-warn" : word.tone === "muted" ? "is-muted" : "";
  const review = row && (row.state === "done" || row.state === "applied");
  return (
    <div className="row eq-ch-row">
      <span className="ch-num">Ch {status.channel + 1}</span>
      <span className="row-text">
        <span>{name}</span>
        <span className="text-caption">{ROLE_LABEL[status.role]}</span>
      </span>
      {row?.state === "listening" && <span className="listen-dot" aria-hidden />}
      <span className={`state ${tone}`} title={word.detail}>
        {word.word}
      </span>
      <span className="row-btn">
        {review && (
          <button
            className="sm-btn"
            onClick={() => {
              openPanel(status.channel);
              setView("mixer");
            }}
          >
            Review
            <span className="visually-hidden"> {name}</span>
          </button>
        )}
      </span>
    </div>
  );
}

/** Ring-out: finds where each speech and vocal mic starts to ring. Always asks first. */
function FeedbackCheck() {
  const ringOut = useAiEq((s) => s.status?.ringOut ?? null);
  const channels = useAiEq((s) => s.status?.channels ?? NONE);
  const inService = useAiEq((s) => s.status?.inService ?? false);
  const engineer = useMixer((s) => can(s.session, "changeAutoMixSetup"));
  const locked = useMixer((s) => lockReason(s.session, "aiEq") !== null);
  const { ringOutStart, ringOutStop } = useAiEq.getState();
  const [asking, setAsking] = useState(false);
  const nameOf = (ch: number) => channels.find((c) => c.channel === ch)?.name ?? `Ch ${ch + 1}`;
  const reason = locked
    ? "AI EQ is in Pro. See Settings."
    : !engineer
      ? "Engineers and admins can run the feedback check."
      : inService
        ? "The feedback check waits until the service ends."
        : null;

  return (
    <section className="section" aria-labelledby="ring-title">
      <div className="section-head">
        <h2 id="ring-title">Feedback check</h2>
      </div>
      <div className="group">
        {ringOut?.running && ringOut.channel !== null ? (
          <div className="row" role="status">
            <span className="listen-dot" aria-hidden />
            <span className="row-text">
              <span>Raising {nameOf(ringOut.channel)} slowly</span>
              <span className="text-caption">It drops 6 dB the moment it rings.</span>
            </span>
            <button className="sm-btn" onClick={() => void ringOutStop()}>
              Stop
            </button>
          </div>
        ) : (
          <div className="row">
            <span className="row-text">
              <span>Find ringing spots</span>
              <span className="text-caption">Speakers on, nobody in in-ears. Engineers only.</span>
            </span>
            <button className="sm-btn" disabled={!!reason} title={reason ?? undefined} onClick={() => setAsking(true)}>
              {ringOut && ringOut.results.length > 0 ? "Run again" : "Start"}
            </button>
          </div>
        )}
        {ringOut?.results.map((r) => (
          <div key={r.channel} className="row eq-ch-row">
            <span className="ch-num">Ch {r.channel + 1}</span>
            <span className="row-text">
              <span>{nameOf(r.channel)}</span>
              <span className="text-caption">
                {r.ceilingDb !== null ? `Fader ceiling ${formatGain(r.ceilingDb)}` : "Reached its top without ringing"}
              </span>
            </span>
            <span className={`state ${r.hz !== null ? "is-warn" : ""}`}>
              {r.hz !== null ? `Rings at ${formatHz(r.hz)}` : "No ringing"}
            </span>
          </div>
        ))}
      </div>
      {ringOut && ringOut.results.length > 0 && !ringOut.running && (
        <p className="section-foot">Band 4 watches these frequencies during the service.</p>
      )}
      {asking && (
        <Dialog
          title="Start the feedback check?"
          onClose={() => setAsking(false)}
          actions={
            <>
              <button className="sm-btn sm-btn--ghost" onClick={() => setAsking(false)} data-autofocus>
                Cancel
              </button>
              <button
                className="sm-btn sm-btn--primary"
                onClick={() => {
                  setAsking(false);
                  void ringOutStart();
                }}
              >
                Start feedback check
              </button>
            </>
          }
        >
          <p>
            SanctuaryMix raises each speech and vocal mic slowly until it starts to ring, then pulls it down 6 dB at
            once.
          </p>
          <p className="muted">Turn the speakers on, keep the band quiet, and make sure nobody is wearing in-ears.</p>
        </Dialog>
      )}
    </section>
  );
}
