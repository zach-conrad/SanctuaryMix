import { ChevronRight, UserPlus } from "lucide-react";
import { useCallback, useEffect, useState, type FormEvent } from "react";
import { inviteMember, loadTeam, removeMember, setMemberRole, type TeamMember } from "./auth/admin";
import { Field, FormError, checkEmail } from "./auth/AuthForms";
import type { Church } from "./auth/useAccount";
import type { Role } from "./cloud/types";
import { Dialog, Segmented } from "./components/Dialog";

const dateFormat = new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric", year: "numeric" });

const ROLE_LABEL = { admin: "Admin", engineer: "Engineer", volunteer: "Volunteer" } as const;
const ROLE_OPTIONS: { id: Role; name: string }[] = [
  { id: "admin", name: "Admin" },
  { id: "engineer", name: "Engineer" },
  { id: "volunteer", name: "Volunteer" },
];
const ROLE_HELP: Record<Role, string> = {
  admin: "Manages the team and the plan, and mixes.",
  engineer: "Mixes and changes the setup.",
  volunteer: "Mixes and runs a ready auto-mix, without changing the setup.",
};

/** Essentials' team login limit; matches private.member_limit in the database. */
const ESSENTIALS_LOGINS = 3;

/** Church Admins only: who's on the team, invites, roles. */
export function Team({ church, me }: { church: Church; me: string }) {
  const [team, setTeam] = useState<TeamMember[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [inviting, setInviting] = useState(false);

  const reload = useCallback(async () => {
    try {
      setTeam(await loadTeam(church.id));
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  }, [church.id]);
  useEffect(() => {
    void reload();
  }, [reload]);

  const full = church.plan === "essentials" && (team?.length ?? 0) >= ESSENTIALS_LOGINS;
  const member = team?.find((m) => m.userId === open) ?? null;

  return (
    <section className="grouped" aria-labelledby="team-heading">
      <div className="section-head">
        <h2 id="team-heading">Team</h2>
        {team ? <span className="text-caption mix-muted">{team.length}</span> : null}
      </div>
      {error ? <p className="section-foot">Your team didn't load. Reload the page to try again.</p> : null}
      {team ? (
        <ul className="group">
          {team.map((m) => (
            <li key={m.userId}>
              <button type="button" className="row row--link row--button" onClick={() => setOpen(m.userId)}>
                <span className="row-text">
                  <span className="account__email">
                    {m.name ?? m.email}
                    {m.userId === me ? " (you)" : ""}
                  </span>
                  <span className="text-caption account__email">
                    {m.pending
                      ? m.name
                        ? `${m.email} · Invited, hasn't joined yet`
                        : "Invited, hasn't joined yet"
                      : m.name
                        ? m.email
                        : m.lastSignInAt
                          ? `Last signed in ${dateFormat.format(m.lastSignInAt)}`
                          : "Never signed in"}
                  </span>
                </span>
                <span className="row-value">{ROLE_LABEL[m.role]}</span>
                <ChevronRight aria-hidden="true" className="row-chevron" />
              </button>
            </li>
          ))}
          <li className="row row--actions">
            <span className="row-text text-caption">{full ? `Essentials includes ${ESSENTIALS_LOGINS} team logins` : null}</span>
            <button type="button" className="sm-btn" onClick={() => setInviting(true)} disabled={full}>
              <UserPlus aria-hidden="true" />
              Invite
            </button>
          </li>
        </ul>
      ) : null}
      <p className="section-foot">Everyone signs in with their own email. Only Admins see this list.</p>

      <Dialog open={member !== null} onClose={() => setOpen(null)} labelledBy="member-dialog-title">
        {member ? (
          <MemberPanel
            church={church}
            member={member}
            isMe={member.userId === me}
            onChanged={reload}
            onRemoved={() => setOpen(null)}
          />
        ) : null}
      </Dialog>
      <Dialog open={inviting} onClose={() => setInviting(false)} labelledBy="invite-dialog-title">
        <InvitePanel church={church} onInvited={reload} />
      </Dialog>
    </section>
  );
}

function MemberPanel({
  church,
  member,
  isMe,
  onChanged,
  onRemoved,
}: {
  church: Church;
  member: TeamMember;
  isMe: boolean;
  onChanged: () => Promise<void>;
  onRemoved: () => void;
}) {
  const [role, setRole] = useState<Role>(member.role);
  const [busy, setBusy] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);

  const saveRole = async () => {
    setBusy(true);
    setMessage(null);
    try {
      await setMemberRole(church.id, member.userId, role);
      await onChanged();
      // Stepping down from Admin hides this section; reload to show the page as they now see it.
      if (isMe && role !== "admin") window.location.reload();
      setMessage({ text: "Role saved.", error: false });
    } catch (e) {
      setMessage({ text: (e as Error).message, error: true });
    }
    setBusy(false);
  };

  const remove = async () => {
    setBusy(true);
    setMessage(null);
    try {
      await removeMember(church.id, member.userId);
      if (isMe) {
        window.location.reload();
        return;
      }
      await onChanged();
      onRemoved();
    } catch (e) {
      setMessage({ text: (e as Error).message, error: true });
      setBusy(false);
      setConfirmRemove(false);
    }
  };

  return (
    <div className="share">
      <div className="share__head">
        <h2 id="member-dialog-title" className="text-heading account__email">
          {member.name ?? member.email}
        </h2>
        <p className="text-caption mix-muted account__email">
          {member.email} · {member.pending ? "Invited, hasn't joined yet" : `Joined ${dateFormat.format(member.joinedAt)}`}
        </p>
      </div>

      <section className="grouped" aria-labelledby="role-heading">
        <div className="section-head">
          <h3 id="role-heading">Role</h3>
        </div>
        <div className="group">
          <div className="row">
            <Segmented label="Role" options={ROLE_OPTIONS} value={role} onChange={setRole} />
          </div>
          <div className="row row--actions">
            <span className="row-text text-caption">{ROLE_HELP[role]}</span>
            <button type="button" className="sm-btn" disabled={busy || role === member.role} onClick={() => void saveRole()}>
              Save role
            </button>
          </div>
        </div>
      </section>

      <div className="group">
        <div className="row">
          <span className="row-text">
            <span>{isMe ? "Leave this church" : "Remove from team"}</span>
            <span className="text-caption">Their recorded services stay with the church</span>
          </span>
          {confirmRemove ? (
            <span className="admin__confirm">
              <button type="button" className="sm-btn sm-btn--ghost" disabled={busy} onClick={() => setConfirmRemove(false)}>
                Cancel
              </button>
              <button type="button" className="sm-btn sm-btn--danger" disabled={busy} onClick={() => void remove()}>
                {isMe ? "Leave" : "Remove"}
              </button>
            </span>
          ) : (
            <button type="button" className="sm-btn sm-btn--danger" disabled={busy} onClick={() => setConfirmRemove(true)}>
              {isMe ? "Leave" : "Remove"}
            </button>
          )}
        </div>
      </div>

      {message ? (
        <p className={message.error ? "auth__error" : "section-foot"} role={message.error ? "alert" : "status"}>
          {message.text}
        </p>
      ) : null}
    </div>
  );
}

function InvitePanel({ church, onInvited }: { church: Church; onInvited: () => Promise<void> }) {
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<Role>("volunteer");
  const [busy, setBusy] = useState(false);
  const [showErrors, setShowErrors] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sent, setSent] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setShowErrors(true);
    if (checkEmail(email)) return;
    setBusy(true);
    setError(null);
    try {
      setSent(await inviteMember(church.id, email.trim(), role));
      setEmail("");
      setShowErrors(false);
      await onInvited();
    } catch (err) {
      setError((err as Error).message);
    }
    setBusy(false);
  };

  return (
    <form className="share" onSubmit={submit} noValidate>
      <div className="share__head">
        <h2 id="invite-dialog-title" className="text-heading">
          Invite to {church.name}
        </h2>
        <p className="text-caption mix-muted">They get an email to choose a password, then sign in to the app.</p>
      </div>
      <Field
        label="Email"
        type="email"
        value={email}
        onChange={(v) => {
          setEmail(v);
          setSent(null);
        }}
        autoComplete="off"
        check={checkEmail}
        showErrors={showErrors}
      />
      <div className="sm-field auth__field">
        <span className="sm-field__label">Role</span>
        <Segmented label="Role" options={ROLE_OPTIONS} value={role} onChange={setRole} />
        <p className="sm-field__help">{ROLE_HELP[role]}</p>
      </div>
      <FormError message={error} />
      {sent ? (
        <p className="section-foot" role="status">
          {sent}
        </p>
      ) : null}
      <button type="submit" className="sm-btn sm-btn--primary sm-btn--lg auth__wide" disabled={busy}>
        Send invite
      </button>
    </form>
  );
}
