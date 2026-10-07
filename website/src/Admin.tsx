import { ChevronRight, CloudOff, Search, ShieldCheck, ShieldQuestion } from "lucide-react";
import { useCallback, useEffect, useId, useMemo, useState, type FormEvent } from "react";
import {
  accountAction,
  loadAccounts,
  loadChurches,
  loadOverview,
  loadRecentChanges,
  mfaState,
  ownerStatus,
  setPlan,
  setTrial,
  signOutEverywhere,
  startEnrollment,
  verifyCode,
  type AccountAction,
  type AdminAccount,
  type AdminChurch,
  type AuditEntry,
  type BillingId,
  type Enrollment,
  type Overview,
  type PlanId,
} from "./auth/admin";
import { AuthForm, AuthShell, FormError } from "./auth/AuthForms";
import { signOut, useAccount } from "./auth/useAccount";
import { Dialog, Segmented } from "./components/Dialog";
import { Footer } from "./components/Footer";
import { Header } from "./components/Header";
import { PLANS } from "./pricing";

const ROOT = "../";
const DAY_MS = 86_400_000;

const dateFormat = new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric", year: "numeric" });
const timeFormat = new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });

const ROLE_LABEL = { admin: "Admin", engineer: "Engineer", volunteer: "Volunteer" } as const;
const PLAN_OPTIONS: { id: PlanId; name: string }[] = PLANS.map((p) => ({ id: p.id as PlanId, name: p.name }));
const BILLING_OPTIONS: { id: BillingId; name: string }[] = [
  { id: "monthly", name: "Monthly" },
  { id: "yearly", name: "Yearly" },
];

const planName = (id: string) => PLANS.find((p) => p.id === id)?.name ?? id;
const daysUntil = (d: Date) => Math.ceil((d.getTime() - Date.now()) / DAY_MS);
const lastSeen = (d: Date | null) => (d ? `Last signed in ${dateFormat.format(d)}` : "Never signed in");

/** `YYYY-MM-DD` in local time, for a date input. */
function dayValue(d: Date) {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

function trialLabel(c: AdminChurch) {
  const days = daysUntil(c.trialEndsAt);
  return days > 0 ? `Trial · ${days} ${days === 1 ? "day" : "days"}` : "Trial ended";
}

/** Owner-only page: every church and account, plus the few changes an owner makes. */
export function Admin() {
  const { state, refresh } = useAccount();
  const user = state.status === "ready" ? { name: state.account.name, church: state.account.church.name } : null;

  return (
    <>
      <Header root={ROOT} account user={user} onSignOut={() => void signOut(ROOT)} />
      <main className="section section--tight">
        <div className="site-container">
          {state.status === "loading" ? <p className="mix-muted service__status">Checking your account</p> : null}
          {state.status === "signedOut" ? <AuthForm root={ROOT} notice={state.notice} /> : null}
          {state.status === "recovery" || state.status === "needsChurch" ? (
            <div className="empty-state">
              <ShieldQuestion aria-hidden="true" />
              <h1 className="text-heading">Finish setting up your account</h1>
              <a className="sm-btn" href={`${ROOT}account/`}>
                Go to your account
              </a>
            </div>
          ) : null}
          {state.status === "error" ? <LoadError message={state.message} /> : null}
          {state.status === "ready" ? <OwnerGate onSignedIn={() => void refresh()} /> : null}
        </div>
      </main>
      <Footer />
    </>
  );
}

function LoadError({ message }: { message: string }) {
  return (
    <div className="empty-state">
      <CloudOff aria-hidden="true" />
      <h1 className="text-heading">This page didn't load</h1>
      <p className="text-caption mix-muted">{message} Reload the page to try again.</p>
    </div>
  );
}

/** Owners only, and only after two-factor. Everyone else sees "not found". */
function OwnerGate({ onSignedIn }: { onSignedIn: () => void }) {
  const [status, setStatus] = useState<{ owner: boolean; mfa: boolean } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const check = useCallback(() => {
    ownerStatus().then(setStatus, (e: Error) => setError(e.message));
  }, []);
  useEffect(check, [check]);

  if (error) return <LoadError message={error} />;
  if (!status) return <p className="mix-muted service__status">Checking your access</p>;
  if (!status.owner) {
    return (
      <div className="empty-state">
        <ShieldQuestion aria-hidden="true" />
        <h1 className="text-heading">Page not found</h1>
        <a className="sm-btn" href={`${ROOT}account/`}>
          Go to your account
        </a>
      </div>
    );
  }
  if (!status.mfa) {
    return (
      <TwoFactor
        onDone={() => {
          check();
          onSignedIn();
        }}
      />
    );
  }
  return <Dashboard />;
}

/** Ask for the authenticator code, or set one up the first time. */
function TwoFactor({ onDone }: { onDone: () => void }) {
  const [factorId, setFactorId] = useState<string | null | undefined>(undefined);
  const [enrollment, setEnrollment] = useState<Enrollment | null>(null);
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const codeId = useId();

  useEffect(() => {
    mfaState().then(
      (s) => setFactorId(s.factorId),
      (e: Error) => setError(e.message),
    );
  }, []);

  const begin = async () => {
    setBusy(true);
    setError(null);
    try {
      setEnrollment(await startEnrollment());
    } catch (e) {
      setError((e as Error).message);
    }
    setBusy(false);
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const id = enrollment?.factorId ?? factorId;
    if (!id) return;
    if (!/^\d{6}$/.test(code.trim())) {
      setError("Enter the 6-digit code from your authenticator app.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await verifyCode(id, code.trim());
      onDone();
    } catch (err) {
      setError((err as Error).message);
      setBusy(false);
    }
  };

  if (factorId === undefined && !error) return <p className="mix-muted service__status">Checking two-factor</p>;

  const codeForm = (
    <form className="auth__form" onSubmit={submit} noValidate>
      <div className="sm-field auth__field" data-invalid={error ? "true" : undefined}>
        <label className="sm-field__label" htmlFor={codeId}>
          6-digit code
        </label>
        <input
          id={codeId}
          className="sm-input text-readout"
          inputMode="numeric"
          autoComplete="one-time-code"
          maxLength={6}
          value={code}
          onChange={(e) => setCode(e.target.value.replace(/\D/g, ""))}
          autoFocus
        />
      </div>
      <FormError message={error} />
      <button type="submit" className="sm-btn sm-btn--primary sm-btn--lg auth__wide" disabled={busy}>
        Continue
      </button>
    </form>
  );

  if (factorId) {
    return (
      <AuthShell title="Two-factor check" lede="Enter the code from your authenticator app to open Admin.">
        {codeForm}
      </AuthShell>
    );
  }

  if (!enrollment) {
    return (
      <AuthShell title="Turn on two-factor" lede="Admin can change any church, so it needs a code from an authenticator app as well as your password.">
        <FormError message={error} />
        <button type="button" className="sm-btn sm-btn--primary sm-btn--lg auth__wide" onClick={() => void begin()} disabled={busy}>
          <ShieldCheck aria-hidden="true" />
          Set up two-factor
        </button>
      </AuthShell>
    );
  }

  return (
    <AuthShell title="Scan this code" lede="Use 1Password, Google Authenticator or any authenticator app, then enter the code it shows.">
      <div className="mfa__qr">
        <img src={enrollment.qr} alt="QR code for your authenticator app" width={200} height={200} />
      </div>
      <p className="section-foot auth__lede">
        Can't scan? Enter this key: <span className="text-readout mfa__secret">{enrollment.secret}</span>
      </p>
      {codeForm}
    </AuthShell>
  );
}

interface Data {
  overview: Overview;
  churches: AdminChurch[];
  accounts: AdminAccount[];
  changes: AuditEntry[];
}

function Dashboard() {
  const [data, setData] = useState<Data | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [church, setChurch] = useState<string | null>(null);
  const [account, setAccount] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      const [overview, churches, accounts, changes] = await Promise.all([
        loadOverview(),
        loadChurches(),
        loadAccounts(),
        loadRecentChanges(),
      ]);
      setData({ overview, churches, accounts, changes });
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  }, []);
  useEffect(() => {
    void reload();
  }, [reload]);

  const accounts = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!data || !q) return data?.accounts ?? [];
    return data.accounts.filter((a) =>
      [a.email, a.name ?? "", a.orgName ?? ""].some((v) => v.toLowerCase().includes(q)),
    );
  }, [data, query]);

  if (error && !data) return <LoadError message={error} />;
  if (!data) return <p className="mix-muted service__status">Loading churches and accounts</p>;

  const openChurch = data.churches.find((c) => c.id === church) ?? null;
  const openAccount = data.accounts.find((a) => a.id === account) ?? null;
  const { overview } = data;

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">Admin</h1>
      </header>

      <div className="page-split">
        <div className="page-main">
          <section className="grouped" aria-labelledby="churches-heading">
            <div className="section-head">
              <h2 id="churches-heading">Churches</h2>
              <span className="text-caption mix-muted">{data.churches.length}</span>
            </div>
            <ul className="group">
              {data.churches.map((c) => {
                const ended = daysUntil(c.trialEndsAt) <= 0;
                return (
                  <li key={c.id}>
                    <button type="button" className="row row--link row--button" onClick={() => setChurch(c.id)}>
                      <span className="row-text">
                        <span>{c.name}</span>
                        <span className="text-caption">
                          {planName(c.plan)}, {c.billing} · {c.members} {c.members === 1 ? "member" : "members"}
                          {c.adminEmail ? ` · ${c.adminEmail}` : ""}
                        </span>
                      </span>
                      <span className={`sm-pill${ended ? " sm-pill--warn" : ""}`}>
                        <span className="sm-pill__dot" aria-hidden="true" />
                        {trialLabel(c)}
                      </span>
                      <ChevronRight aria-hidden="true" className="row-chevron" />
                    </button>
                  </li>
                );
              })}
            </ul>
            {data.churches.length === 0 ? <p className="section-foot">No churches yet.</p> : null}
          </section>

          <section className="grouped" aria-labelledby="accounts-heading">
            <div className="section-head">
              <h2 id="accounts-heading">Accounts</h2>
              <span className="text-caption mix-muted">{data.accounts.length}</span>
            </div>
            <label className="admin__search">
              <Search aria-hidden="true" />
              <input
                className="sm-input"
                type="search"
                placeholder="Search by name, email or church"
                aria-label="Search accounts"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </label>
            <ul className="group">
              {accounts.map((a) => (
                <li key={a.id}>
                  <button type="button" className="row row--link row--button" onClick={() => setAccount(a.id)}>
                    <span className="row-text">
                      <span className="account__email">{a.name ? `${a.name} · ${a.email}` : a.email}</span>
                      <span className="text-caption">
                        {a.orgName ? `${a.orgName} · ${ROLE_LABEL[a.role ?? "volunteer"]}` : "No church"} · {lastSeen(a.lastSignInAt)}
                      </span>
                    </span>
                    <AccountPills account={a} />
                    <ChevronRight aria-hidden="true" className="row-chevron" />
                  </button>
                </li>
              ))}
            </ul>
            {accounts.length === 0 ? <p className="section-foot">No accounts match.</p> : null}
          </section>
        </div>

        <aside className="page-side">
          <section className="grouped" aria-labelledby="overview-heading">
            <div className="section-head">
              <h2 id="overview-heading">Overview</h2>
            </div>
            <dl className="group">
              <Stat label="Churches" value={overview.churches} />
              <Stat label="Accounts" value={overview.accounts} />
              <Stat label="New accounts this week" value={overview.signups_week} />
              <Stat label="Trials ending this week" value={overview.trials_ending_week} />
              <Stat label="Trials ended" value={overview.trials_ended} />
            </dl>
            <p className="section-foot">Billing moves to Stripe's dashboard once checkout is live.</p>
          </section>

          <section className="grouped" aria-labelledby="changes-heading">
            <div className="section-head">
              <h2 id="changes-heading">Recent changes</h2>
            </div>
            {data.changes.length ? (
              <ul className="group">
                {data.changes.map((c, i) => (
                  <li key={i} className="row">
                    <span className="row-text">
                      <span>{describeChange(c)}</span>
                      <span className="text-caption">
                        {timeFormat.format(c.createdAt)}
                        {c.actorEmail ? ` · ${c.actorEmail}` : ""}
                      </span>
                    </span>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="section-foot">Changes made here and on Team pages show up here.</p>
            )}
          </section>
        </aside>
      </div>

      <Dialog open={openChurch !== null} onClose={() => setChurch(null)} labelledBy="church-dialog-title">
        {openChurch ? (
          <ChurchPanel
            church={openChurch}
            members={data.accounts.filter((a) => a.orgId === openChurch.id)}
            onChanged={reload}
          />
        ) : null}
      </Dialog>
      <Dialog open={openAccount !== null} onClose={() => setAccount(null)} labelledBy="account-dialog-title">
        {openAccount ? <AccountPanel account={openAccount} onChanged={reload} /> : null}
      </Dialog>
    </>
  );
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div className="row">
      <dt className="row-text">{label}</dt>
      <dd className="row-value text-readout">{value}</dd>
    </div>
  );
}

function AccountPills({ account: a }: { account: AdminAccount }) {
  return (
    <span className="admin__pills">
      {a.owner ? (
        <span className="sm-pill">
          <span className="sm-pill__dot" aria-hidden="true" />
          Owner
        </span>
      ) : null}
      {a.disabled ? (
        <span className="sm-pill">
          <span className="sm-pill__dot" aria-hidden="true" />
          Disabled
        </span>
      ) : !a.confirmed ? (
        <span className="sm-pill sm-pill--warn">
          <span className="sm-pill__dot" aria-hidden="true" />
          {a.invited ? "Invited" : "Unconfirmed"}
        </span>
      ) : null}
    </span>
  );
}

const ACTION_LABEL: Record<string, string> = {
  "church.trial": "Trial changed",
  "church.plan": "Plan changed",
  "account.sign_out_everywhere": "Signed out everywhere",
  "account.reset_password": "Password reset sent",
  "account.resend_email": "Email sent again",
  "account.disable": "Sign-in disabled",
  "account.enable": "Sign-in enabled",
  "team.add": "Added to team",
  "team.role": "Role changed",
  "team.remove": "Removed from team",
};

function describeChange(c: AuditEntry) {
  const what = ACTION_LABEL[c.action] ?? c.action;
  let after = "";
  if (c.action === "church.trial" && typeof c.detail.after === "string") after = ` to ${dateFormat.format(new Date(c.detail.after))}`;
  else if (typeof c.detail.after === "string") after = ` to ${c.detail.after}`;
  return `${what}${c.target ? `: ${c.target}` : ""}${after}`;
}

/** Plan and trial for one church, plus who's on it. */
function ChurchPanel({ church, members, onChanged }: { church: AdminChurch; members: AdminAccount[]; onChanged: () => Promise<void> }) {
  const [plan, setPlanChoice] = useState<PlanId>(church.plan);
  const [billing, setBilling] = useState<BillingId>(church.billing);
  const [trialDay, setTrialDay] = useState(dayValue(church.trialEndsAt));
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);
  const trialId = useId();

  const planChanged = plan !== church.plan || billing !== church.billing;
  const trialChanged = trialDay !== dayValue(church.trialEndsAt);

  const run = async (work: () => Promise<unknown>, done: string) => {
    setBusy(true);
    setMessage(null);
    try {
      await work();
      await onChanged();
      setMessage({ text: done, error: false });
    } catch (e) {
      setMessage({ text: (e as Error).message, error: true });
    }
    setBusy(false);
  };

  const addDays = (n: number) => {
    const from = Math.max(church.trialEndsAt.getTime(), Date.now());
    setTrialDay(dayValue(new Date(from + n * DAY_MS)));
  };

  // End of the chosen day, local time, so "Oct 31" means the whole of Oct 31.
  const trialEnd = () => {
    const [y, m, d] = trialDay.split("-").map(Number);
    return new Date(y, m - 1, d, 23, 59, 59);
  };

  return (
    <div className="share">
      <div className="share__head">
        <h2 id="church-dialog-title" className="text-heading">
          {church.name}
        </h2>
        <p className="text-caption mix-muted">Created {dateFormat.format(church.createdAt)}</p>
      </div>

      <section className="grouped" aria-labelledby="plan-heading">
        <div className="section-head">
          <h3 id="plan-heading">Plan</h3>
        </div>
        <div className="group">
          <div className="row">
            <span className="row-text">Plan</span>
            <Segmented label="Plan" options={PLAN_OPTIONS} value={plan} onChange={setPlanChoice} />
          </div>
          <div className="row">
            <span className="row-text">Billing</span>
            <Segmented label="Billing" options={BILLING_OPTIONS} value={billing} onChange={setBilling} />
          </div>
          <div className="row row--actions">
            <button
              type="button"
              className="sm-btn"
              disabled={busy || !planChanged}
              onClick={() => void run(() => setPlan(church.id, plan, billing), "Plan saved.")}
            >
              Save plan
            </button>
          </div>
        </div>
      </section>

      <section className="grouped" aria-labelledby="trial-heading">
        <div className="section-head">
          <h3 id="trial-heading">Trial</h3>
          <span className="text-caption mix-muted">{trialLabel(church)}</span>
        </div>
        <div className="group">
          <div className="row">
            <label className="row-text" htmlFor={trialId}>
              Ends
            </label>
            <input
              id={trialId}
              className="sm-input admin__date"
              type="date"
              value={trialDay}
              onChange={(e) => setTrialDay(e.target.value)}
            />
          </div>
          <div className="row row--actions">
            <button type="button" className="sm-btn sm-btn--ghost" onClick={() => addDays(14)} disabled={busy}>
              Add 14 days
            </button>
            <button type="button" className="sm-btn sm-btn--ghost" onClick={() => addDays(30)} disabled={busy}>
              Add 30 days
            </button>
            <button
              type="button"
              className="sm-btn sm-btn--primary"
              disabled={busy || !trialChanged || !trialDay}
              onClick={() => void run(() => setTrial(church.id, trialEnd()), "Trial saved.")}
            >
              Save trial
            </button>
          </div>
        </div>
      </section>

      <section className="grouped" aria-labelledby="members-heading">
        <div className="section-head">
          <h3 id="members-heading">Members</h3>
          <span className="text-caption mix-muted">{members.length}</span>
        </div>
        <ul className="group">
          {members.map((m) => (
            <li key={m.id} className="row">
              <span className="row-text">
                <span className="account__email">{m.name ?? m.email}</span>
                <span className="text-caption account__email">{m.name ? m.email : lastSeen(m.lastSignInAt)}</span>
              </span>
              <span className="row-value">{m.role ? ROLE_LABEL[m.role] : ""}</span>
            </li>
          ))}
        </ul>
      </section>

      {message ? (
        <p className={message.error ? "auth__error" : "section-foot"} role={message.error ? "alert" : "status"}>
          {message.text}
        </p>
      ) : null}
    </div>
  );
}

/** One account: details, then email and sign-in actions. */
function AccountPanel({ account: a, onChanged }: { account: AdminAccount; onChanged: () => Promise<void> }) {
  const [busy, setBusy] = useState(false);
  const [confirmDisable, setConfirmDisable] = useState(false);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);

  const run = async (work: () => Promise<string>) => {
    setBusy(true);
    setMessage(null);
    try {
      const text = await work();
      await onChanged();
      setMessage({ text, error: false });
    } catch (e) {
      setMessage({ text: (e as Error).message, error: true });
    }
    setBusy(false);
    setConfirmDisable(false);
  };
  const act = (action: AccountAction) => run(() => accountAction(action, a.id));

  return (
    <div className="share">
      <div className="share__head">
        <h2 id="account-dialog-title" className="text-heading account__email">
          {a.name ?? a.email}
        </h2>
        <p className="text-caption mix-muted account__email">
          {a.email} · {a.provider === "google" ? "Google" : "Email and password"}
        </p>
      </div>

      <dl className="group">
        <div className="row">
          <dt className="row-text">Church</dt>
          <dd className="row-value">{a.orgName ? `${a.orgName}, ${ROLE_LABEL[a.role ?? "volunteer"]}` : "None"}</dd>
        </div>
        <div className="row">
          <dt className="row-text">Signed up</dt>
          <dd className="row-value">{dateFormat.format(a.createdAt)}</dd>
        </div>
        <div className="row">
          <dt className="row-text">Last sign-in</dt>
          <dd className="row-value">{a.lastSignInAt ? timeFormat.format(a.lastSignInAt) : "Never"}</dd>
        </div>
        <div className="row">
          <dt className="row-text">Email</dt>
          <dd className="row-value">{a.confirmed ? "Confirmed" : a.invited ? "Invited, not joined" : "Not confirmed"}</dd>
        </div>
        <div className="row">
          <dt className="row-text">Sign-in</dt>
          <dd className="row-value">{a.disabled ? "Disabled" : "Allowed"}</dd>
        </div>
      </dl>

      <section className="grouped" aria-labelledby="actions-heading">
        <div className="section-head">
          <h3 id="actions-heading">Actions</h3>
        </div>
        <div className="group">
          <div className="row">
            <span className="row-text">
              <span>Password reset</span>
              <span className="text-caption">Emails a link to choose a new password</span>
            </span>
            <button type="button" className="sm-btn" disabled={busy} onClick={() => void act("reset_password")}>
              Send
            </button>
          </div>
          {!a.confirmed ? (
            <div className="row">
              <span className="row-text">
                <span>{a.invited ? "Invite" : "Confirmation email"}</span>
                <span className="text-caption">They haven't opened the last one</span>
              </span>
              <button type="button" className="sm-btn" disabled={busy} onClick={() => void act("resend_email")}>
                Resend
              </button>
            </div>
          ) : null}
          <div className="row">
            <span className="row-text">
              <span>Sign out everywhere</span>
              <span className="text-caption">Website and app, within the hour. Mixing keeps working.</span>
            </span>
            <button
              type="button"
              className="sm-btn"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  const n = await signOutEverywhere(a.id);
                  return n ? `Signed out of ${n} ${n === 1 ? "session" : "sessions"}.` : "No sessions were open.";
                })
              }
            >
              Sign out
            </button>
          </div>
          {a.owner ? null : (
            <div className="row">
              <span className="row-text">
                <span>{a.disabled ? "Sign-in is off" : "Turn off sign-in"}</span>
                <span className="text-caption">
                  {a.disabled ? "They can't sign in until you turn it back on" : "Their data stays; a live service isn't stopped"}
                </span>
              </span>
              {a.disabled ? (
                <button type="button" className="sm-btn" disabled={busy} onClick={() => void act("enable")}>
                  Turn on
                </button>
              ) : confirmDisable ? (
                <span className="admin__confirm">
                  <button type="button" className="sm-btn sm-btn--ghost" disabled={busy} onClick={() => setConfirmDisable(false)}>
                    Cancel
                  </button>
                  <button type="button" className="sm-btn sm-btn--danger" disabled={busy} onClick={() => void act("disable")}>
                    Turn off
                  </button>
                </span>
              ) : (
                <button type="button" className="sm-btn sm-btn--danger" disabled={busy} onClick={() => setConfirmDisable(true)}>
                  Turn off
                </button>
              )}
            </div>
          )}
        </div>
      </section>

      {message ? (
        <p className={message.error ? "auth__error" : "section-foot"} role={message.error ? "alert" : "status"}>
          {message.text}
        </p>
      ) : null}
    </div>
  );
}
