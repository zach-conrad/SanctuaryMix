import { Maximize2, RotateCcw } from "lucide-react";
import { useState } from "react";
import { formatGain, formatHz } from "../lib/eqCurve";
import { useAiEq, useEqActive, useEqChannel } from "../store/aieq";
import { useMixer } from "../store/mixer";
import { EqGraph } from "./EqGraph";

/** The Inspector's EQ section: the desk's curve, and what AI EQ is doing on this channel. */
export function InspectorEq({ channel }: { channel: number }) {
  const deskEq = useMixer((s) => s.strips[channel]?.eq ?? null);
  const name = useMixer((s) => s.strips[channel]?.name ?? `Ch ${channel + 1}`);
  const status = useEqChannel(channel);
  const active = useEqActive();
  const inService = useAiEq((s) => s.status?.inService ?? false);
  const openPanel = useAiEq((s) => s.openPanel);
  const undo = useAiEq((s) => s.undo);
  const handBack = useAiEq((s) => s.handBack);
  const setView = useMixer((s) => s.setView);
  const [dismissed, setDismissed] = useState<number | null>(null);

  const eq = status?.eq ?? deskEq;
  // Without the plan, or for channels AI EQ doesn't look after, only the desk's curve shows.
  const ai = active && status && status.mode !== "off" ? status : null;
  const yours = ai?.mode === "yours" || ai?.mode === "undone";
  const keeping = ai && ai.role === "speech" && ai.mode === "keeping";
  const showDiffers = ai && ai.differsFromProfile && inService && dismissed !== channel;

  return (
    <section className="inspector-section insp-eq" aria-label="EQ">
      <div className="insp-eq-head">
        <h2 className="text-heading">EQ</h2>
        {ai && !yours && <span className="sm-assist-badge">AI EQ</span>}
        {yours && <span className="state">Yours</span>}
        <button
          className="sm-btn sm-btn--ghost icon-btn"
          aria-label={`Open EQ for ${name}`}
          title="Open EQ"
          onClick={() => openPanel(channel)}
        >
          <Maximize2 />
        </button>
      </div>
      {eq ? (
        <EqGraph
          size="compact"
          eq={eq}
          spectrum={ai?.spectrum}
          ringHz={ai?.notch?.hz ?? null}
          label={`EQ curve for ${name}`}
        />
      ) : (
        <div className="eq-empty">The console hasn't sent this channel's EQ yet.</div>
      )}
      {ai && (ai.notch || keeping || yours) && (
        <div className="group insp-rows">
          {ai.notch && (
            <div className="row">
              <span className="row-text">Feedback</span>
              <span className="text-readout state-ai">
                {formatHz(ai.notch.hz)} {formatGain(ai.notch.gainDb)}
              </span>
              <button className="sm-btn sm-btn--sm" onClick={() => void undo(channel)}>
                Undo
                <span className="visually-hidden"> the feedback notch on {name}</span>
              </button>
            </div>
          )}
          {keeping && (
            <div className="row" title="Small, slow moves on the low mid keep this voice close to its profile.">
              <span className="row-text">Tone</span>
              <span className="text-caption state-ai">Keeping</span>
              <span className="text-readout muted">{formatGain(ai.toneOffsetDb ?? 0)}</span>
            </div>
          )}
          {yours && (
            <div className="row" title="You changed this EQ, so AI EQ leaves it alone. Feedback guard still works.">
              <span className="row-text">Yours</span>
              <button className="sm-btn sm-btn--sm" onClick={() => void handBack(channel)}>
                <RotateCcw />
                Hand back
              </button>
            </div>
          )}
        </div>
      )}
      {showDiffers && (
        <div className="sm-assist sm-assist--compact">
          <span className="sm-assist__head">
            <span className="sm-assist-badge">Assist</span>
          </span>
          <span className="sm-assist__title">{name} sounds very different from last week</span>
          <span className="sm-assist__why">Maybe the wrong mic was handed over. Check it by ear.</span>
          <span className="sm-assist__actions">
            <button className="sm-btn sm-btn--sm" onClick={() => setView("eqSoundcheck")}>
              Run soundcheck for this mic
            </button>
            <button className="sm-btn sm-btn--ghost sm-btn--sm" onClick={() => setDismissed(channel)}>
              Dismiss
            </button>
          </span>
        </div>
      )}
    </section>
  );
}
