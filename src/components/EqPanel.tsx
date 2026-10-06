import { Check, RotateCcw, Undo2 } from "lucide-react";
import { useRef, useState } from "react";
import { timeOfDay } from "../lib/recordings";
import { allows, lockReason } from "../lib/plans";
import type { ChannelEq, CompareSide } from "../lib/types";
import { ROLE_LABEL } from "../store/automix";
import { eqBlocker, useAiEq, useEqActive, useEqChannel } from "../store/aieq";
import { useMixer } from "../store/mixer";
import { BandTable } from "./BandTable";
import { EqGraph, type HandleId } from "./EqGraph";

/** How often a drag sends its value to the desk. */
const SEND_MS = 120;

/**
 * The full EQ for the selected channel. It takes the Inspector's place at twice its width,
 * so the bay keeps scrolling beside it and faders stay live. Esc does not close it (Esc is Freeze).
 */
export function EqPanel() {
  const channel = useMixer((s) => s.selected);
  // A fresh panel per channel, so a half-finished drag never lands on another channel.
  return <EqPanelFor key={channel} channel={channel} />;
}

function EqPanelFor({ channel }: { channel: number }) {
  const strip = useMixer((s) => s.strips[channel]);
  const consoleOn = useMixer((s) => s.consoleStatus === "on");
  const lock = useMixer((s) => (allows(s.session, "aiEq") ? null : lockReason(s.session, "aiEq")));
  const status = useEqChannel(channel);
  const active = useEqActive();
  const inService = useAiEq((s) => s.status?.inService ?? false);
  const compareSide = useAiEq((s) => (s.status?.compare?.channel === channel ? s.status.compare.side : null));
  const appliedAt = useAiEq((s) => s.applied[channel] ?? null);
  const error = useAiEq((s) => s.error);
  const { closePanel, apply, skip, undo, setEq, compare, handBack } = useAiEq.getState();

  const [draft, setDraft] = useState<ChannelEq | null>(null);
  const [selected, setSelected] = useState<HandleId | null>(null);
  const lastSent = useRef(0);

  const name = strip?.name ?? `Ch ${channel + 1}`;
  const desk = status?.eq ?? strip?.eq ?? null;
  const eq = draft ?? desk;
  const ai = active && status && status.mode !== "off" ? status : null;
  const proposal = ai?.proposal ?? null;
  const yours = ai?.mode === "yours" || ai?.mode === "undone";
  const editable = consoleOn && !lock && !!eq;
  const showApplied = !!ai && appliedAt !== null && !proposal;
  const showCompare = showApplied && !inService;
  const side: CompareSide = compareSide ?? "after";

  /** A hand edit: show it at once, send it at most every SEND_MS while dragging, and always at the end. */
  function onEdit(next: ChannelEq, done: boolean) {
    setDraft(next);
    const now = performance.now();
    if (!done && now - lastSent.current < SEND_MS) return;
    lastSent.current = now;
    const sent = setEq(channel, next);
    if (done) void sent.then(() => setDraft(null));
  }

  const caption = !editable
    ? lock
      ? eqBlocker(lock)
      : !consoleOn
        ? "Connect the console to change the EQ."
        : null
    : proposal
      ? "Apply or skip the suggestion to edit by hand"
      : "Drag a point or tap a value";

  return (
    <aside className="eq-panel" aria-label={`EQ for ${name}`}>
      <div className="eq-panel-head">
        <div className="who">
          <span className="text-label muted">
            Ch {channel + 1}
            {status ? ` · ${ROLE_LABEL[status.role]}` : ""}
          </span>
          <h2 className="text-heading" title={`${name} EQ`}>
            {name} EQ
          </h2>
        </div>
        {showCompare && (
          <div className="sm-seg" role="group" aria-label="Compare by ear">
            <button
              aria-pressed={side === "before"}
              title="Puts the old EQ on the desk so you can hear the difference"
              onClick={() => void compare(channel, "before")}
            >
              Before
            </button>
            <button aria-pressed={side === "after"} onClick={() => void compare(channel, "after")}>
              After
            </button>
          </div>
        )}
        <button
          className="sm-btn"
          onClick={() => {
            if (compareSide === "before") void compare(channel, null);
            closePanel();
          }}
        >
          Done
        </button>
      </div>

      <div className="eq-graph-wrap">
        <EqGraph
          size="full"
          eq={eq}
          proposed={proposal?.eq ?? null}
          spectrum={ai?.spectrum}
          ringHz={ai?.notch?.hz ?? null}
          editable={editable}
          selected={selected}
          onSelect={setSelected}
          onChange={onEdit}
          label={`EQ curve for ${name}. The band table below has every value.`}
        />
        <div className="eq-legend" aria-hidden>
          <span>
            <i />
            {showApplied ? "On the console" : "Now"}
          </span>
          {proposal && (
            <span>
              <i className="ai" />
              Suggested
            </span>
          )}
          {ai?.spectrum && (
            <span>
              <i className="spec" />
              What the mic hears
            </span>
          )}
          {ai?.notch && <span className="state-ai">Feedback {"·"} band 4</span>}
        </div>
      </div>

      {proposal && (
        <div className="sm-assist sm-assist--compact sm-assist--wide">
          <span className="sm-assist__head">
            <span className="sm-assist-badge">Assist</span>
            <span className="sm-assist__time">
              Soundcheck · {proposal.changes.length} {proposal.changes.length === 1 ? "change" : "changes"}
            </span>
          </span>
          <span className="sm-assist__title">{proposal.title}</span>
          <span className="sm-assist__why">{proposal.reason}</span>
          <span className="sm-assist__actions">
            <button
              className="sm-btn sm-btn--primary"
              disabled={inService || !consoleOn}
              title={inService ? "Soundcheck changes wait until the service ends." : undefined}
              onClick={() => void apply(channel)}
            >
              <Check />
              Apply
            </button>
            <button className="sm-btn sm-btn--ghost" onClick={() => void skip(channel)}>
              Skip
            </button>
          </span>
          {inService && <span className="text-caption muted">Changes wait until the service ends.</span>}
        </div>
      )}

      {showApplied && appliedAt !== null && (
        <div className="sm-assist sm-assist--compact sm-assist--wide">
          <span className="sm-assist__head">
            <span className="sm-assist-badge">Assist</span>
            <span className="sm-assist__time">Applied {timeOfDay(appliedAt).toLowerCase()}</span>
          </span>
          <span className="sm-assist__title">
            {showCompare ? "Applied. Flip Before and After to hear it." : "Applied."}
          </span>
          <span className="sm-assist__actions">
            <button className="sm-btn" onClick={() => void undo(channel)}>
              <Undo2 />
              Undo
            </button>
          </span>
        </div>
      )}

      {yours && (
        <div className="group">
          <div className="row eq-panel-note">
            <span className="row-text">
              <span>Yours</span>
              <span className="text-caption">You changed this EQ, so AI EQ leaves it alone. Feedback guard still works.</span>
            </span>
            <button className="sm-btn" onClick={() => void handBack(channel)}>
              <RotateCcw />
              Hand back
            </button>
          </div>
        </div>
      )}

      {error && <p className="error">{error}</p>}

      <section className="section" aria-labelledby="bands-title">
        <div className="section-head">
          <h2 id="bands-title">Bands</h2>
          {caption && <span className="text-caption">{caption}</span>}
        </div>
        {eq ? (
          <BandTable
            eq={eq}
            proposed={proposal?.eq ?? null}
            editable={editable}
            selected={selected}
            onSelect={setSelected}
            onChange={(next) => onEdit(next, true)}
          />
        ) : (
          <div className="group">
            <div className="row empty">The console hasn't sent this channel's EQ yet.</div>
          </div>
        )}
      </section>
    </aside>
  );
}
