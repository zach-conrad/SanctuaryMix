import { Undo2, X } from "lucide-react";
import { formatGain, formatHz } from "../lib/eqCurve";
import { useAiEq, useEqActive } from "../store/aieq";

/** Top of the mixer bay when AI EQ caught ringing: blue because the AI acted, the word Feedback in amber. */
export function FeedbackBanner() {
  const event = useAiEq((s) => s.status?.feedback ?? null);
  const active = useEqActive();
  const undo = useAiEq((s) => s.undo);
  const dismiss = useAiEq((s) => s.dismissFeedback);
  if (!event || !active) return null;

  const fader = formatGain(Math.abs(event.faderCutDb)).replace("+", "");
  const name = event.channelName;
  return (
    <div className="banner banner--assist" role="alert">
      <span className="sm-assist-badge">Assist</span>
      <span className="word">Feedback</span>
      {event.notchDb !== null ? (
        <span className="what">
          {name} rang at <span className="mono">{formatHz(event.hz)}</span>. Cut it{" "}
          <span className="mono">{formatGain(event.notchDb)}</span> and pulled the fader down{" "}
          <span className="mono">{fader}</span>.
          {event.countToday > 1 && <span className="count"> · {event.countToday} today</span>}
        </span>
      ) : (
        <span className="what">
          Pulled {name} down <span className="mono">{fader}</span>. Band 4 is already used.
          {event.countToday > 1 && <span className="count"> · {event.countToday} today</span>}
        </span>
      )}
      <button className="sm-btn sm-btn--sm" onClick={() => void undo(event.channel)}>
        <Undo2 />
        Undo
        <span className="visually-hidden"> the feedback cut on {name}</span>
      </button>
      <button
        className="sm-btn sm-btn--ghost sm-btn--sm icon-btn-sm"
        aria-label="Close"
        title="Close"
        onClick={() => void dismiss()}
      >
        <X />
      </button>
    </div>
  );
}
