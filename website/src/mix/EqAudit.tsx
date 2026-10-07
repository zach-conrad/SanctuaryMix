import { formatClock } from "../../../src/lib/recordings";
import type { EqActor, EqAction, EqAudit as Audit, EqAuditEntry, EqIdea, Role } from "../cloud/types";

interface Props {
  audit: Audit;
  /** Church members see names and ideas for next week; share links see roles only. */
  audience: "church" | "shared";
  positionMs: number;
  onSeek(ms: number): void;
}

const ACTION: Record<EqAction, string> = {
  soundcheck: "Soundcheck EQ",
  feedback: "Feedback cut",
  tone: "Tone kept",
  undo: "Undone",
  person: "Changed by hand",
  handBack: "Handed back to AI EQ",
};

/** Share links show roles, never names. */
const ROLE: Record<Role, string> = { admin: "an Admin", engineer: "an Engineer", volunteer: "a volunteer" };
const ROLE_TITLE: Record<Role, string> = { admin: "Admin", engineer: "Engineer", volunteer: "Volunteer" };

const IDEA_STATE: Record<EqIdea["state"], string> = { waiting: "Waiting in the app", kept: "Kept", dismissed: "Dismissed" };

const clock = new Intl.DateTimeFormat("en-US", { hour: "numeric", minute: "2-digit" });

function who(actor: EqActor, audience: Props["audience"]): string {
  if (actor.where === "desk") return "On the desk";
  if (audience === "church" && actor.name) return actor.name;
  return actor.role ? ROLE_TITLE[actor.role] : "A person";
}

function By({ entry, audience }: { entry: EqAuditEntry; audience: Props["audience"] }) {
  if (entry.by.kind === "ai") {
    return (
      <span className="eq-row__by">
        <span className="sm-assist-badge">AI EQ</span>
        {entry.appliedBy ? (
          <span className="text-caption mix-muted">
            Applied by{" "}
            {audience === "church" && entry.appliedBy.name
              ? entry.appliedBy.name
              : entry.appliedBy.role
                ? ROLE[entry.appliedBy.role]
                : "a person"}
          </span>
        ) : null}
      </span>
    );
  }
  return (
    <span className="eq-row__by">
      <span>{who(entry.by, audience)}</span>
      {entry.by.where === "app" ? <span className="text-caption mix-muted">In the app</span> : null}
    </span>
  );
}

interface RowProps {
  e: EqAuditEntry;
  audience: Props["audience"];
  current: boolean;
  onSeek(ms: number): void;
}

function Row({ e, audience, current, onSeek }: RowProps) {
  return (
    <li className="eq-row" aria-current={current}>
      <span className="eq-row__time">
        {e.tMs === null ? (
          <span className="text-readout mix-muted">{clock.format(e.at)}</span>
        ) : (
          <button className="mix-changes__time text-readout" onClick={() => onSeek(e.tMs!)} title="Play from here">
            {formatClock(e.tMs)}
          </button>
        )}
      </span>
      <span className="eq-row__channel">
        <span className="text-channel-name">{e.channel.name}</span>
        <span className="text-readout mix-subtle">{e.channel.label}</span>
      </span>
      <span className="eq-row__what">
        <span className="text-body-strong">{ACTION[e.action]}</span>
        <span className={`eq-row__changes text-readout${e.by.kind === "ai" ? " eq-row__changes--ai" : ""}`}>
          {e.changes.map((c, i) => (
            <span key={c}>
              {i > 0 ? <span className="eq-row__sep" aria-hidden="true">·</span> : null}
              {c}
            </span>
          ))}
        </span>
        {e.reason ? <span className="text-caption mix-muted">{e.reason}</span> : null}
        {e.action === "person" ? (
          <span className="text-caption mix-muted">AI EQ left this channel alone after that.</span>
        ) : null}
      </span>
      <By entry={e} audience={audience} />
    </li>
  );
}

/** Every EQ change around one service: soundcheck, live, and (for the church) ideas for next week. */
export function EqAudit({ audit, audience, positionMs, onSeek }: Props) {
  const before = audit.entries.filter((e) => e.tMs === null);
  const during = audit.entries.filter((e) => e.tMs !== null);
  const channels = new Set(audit.entries.map((e) => e.channel.label)).size;
  let current = -1;
  during.forEach((e, i) => {
    if (e.tMs! <= positionMs) current = i;
  });

  return (
    <section className="panel eq-audit" aria-labelledby="eq-heading">
      <div className="panel__head">
        <h2 id="eq-heading" className="text-heading">EQ changes</h2>
        <span className="text-caption mix-muted">
          {audit.entries.length} changes · {channels} channels
        </span>
      </div>

      {audit.entries.length === 0 ? <p className="mix-muted">EQ didn't change in this service.</p> : null}

      {before.length ? (
        <div className="grouped">
          <div className="section-head section-head--sub">
            <h3>At soundcheck</h3>
          </div>
          <ul className="group eq-group">
            {before.map((e) => (
              <Row key={e.seq} e={e} audience={audience} current={false} onSeek={onSeek} />
            ))}
          </ul>
        </div>
      ) : null}

      {during.length ? (
        <div className="grouped">
          <div className="section-head section-head--sub">
            <h3>During the service</h3>
          </div>
          <ul className="group eq-group">
            {during.map((e, i) => (
              <Row key={e.seq} e={e} audience={audience} current={i === current} onSeek={onSeek} />
            ))}
          </ul>
        </div>
      ) : null}

      {audience === "church" && audit.ideas.length ? (
        <div className="grouped">
          <div className="section-head section-head--sub">
            <h3>Ideas for next week</h3>
          </div>
          <ul className="group eq-group">
            {audit.ideas.map((idea) => (
              <li key={idea.channel.label + idea.title} className="eq-row eq-row--idea">
                <span className="eq-row__time" />
                <span className="eq-row__channel">
                  <span className="text-channel-name">{idea.channel.name}</span>
                  <span className="text-readout mix-subtle">{idea.channel.label}</span>
                </span>
                <span className="eq-row__what">
                  <span className="text-body-strong">{idea.title}</span>
                  <span className="eq-row__changes eq-row__changes--ai text-readout">{idea.change}</span>
                  <span className="text-caption mix-muted">{idea.reason}</span>
                </span>
                <span className="eq-row__by">
                  <span className="sm-assist-badge">AI EQ</span>
                  <span className="text-caption mix-muted">{IDEA_STATE[idea.state]}</span>
                </span>
              </li>
            ))}
          </ul>
          <p className="section-foot">Keep or dismiss these in the app. Share links never include them.</p>
        </div>
      ) : null}
    </section>
  );
}
