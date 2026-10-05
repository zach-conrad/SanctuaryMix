import { ArrowLeft, KeyRound, LogIn, MailCheck, UserPlus } from "lucide-react";
import { useId, useState, type FormEvent, type ReactNode } from "react";
import { TRIAL_DAYS, findPlan } from "../pricing";
import { MIN_PASSWORD, accountUrl, authMessage, supabase } from "./client";

type Mode = "signIn" | "signUp" | "reset";

const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

/** The plan picked on the pricing section (?plan=&billing=), if any. */
function pickedPlan() {
  const params = new URLSearchParams(window.location.search);
  const plan = findPlan(params.get("plan"));
  return plan ? { plan, billing: params.get("billing") === "monthly" ? "monthly" : "yearly" } : null;
}

/** The app links here with ?signup=1 or ?forgot=1; the pricing section with ?plan=. */
function initialMode(): Mode {
  const params = new URLSearchParams(window.location.search);
  if (params.get("forgot") === "1") return "reset";
  if (params.get("signup") === "1" || pickedPlan()) return "signUp";
  return "signIn";
}

function trialNote() {
  const picked = pickedPlan();
  return picked
    ? `Starts a ${TRIAL_DAYS}-day ${picked.plan.name} trial. No card needed.`
    : `Starts a ${TRIAL_DAYS}-day trial. No card needed.`;
}

interface FieldProps {
  label: string;
  type?: string;
  value: string;
  onChange: (v: string) => void;
  autoComplete: string;
  help?: string;
  /** Checked on blur and on submit; returns the error or null. */
  check?: (v: string) => string | null;
  showErrors: boolean;
}

function Field({ label, type = "text", value, onChange, autoComplete, help, check, showErrors }: FieldProps) {
  const id = useId();
  const [touched, setTouched] = useState(false);
  const error = (touched || showErrors) && check ? check(value) : null;
  return (
    <div className="sm-field auth__field" data-invalid={error ? "true" : undefined}>
      <label className="sm-field__label" htmlFor={id}>{label}</label>
      <input
        id={id}
        className="sm-input"
        type={type}
        value={value}
        autoComplete={autoComplete}
        onChange={(e) => onChange(e.target.value)}
        onBlur={() => setTouched(true)}
        aria-invalid={error ? true : undefined}
        aria-describedby={error || help ? `${id}-help` : undefined}
      />
      {error || help ? <p id={`${id}-help`} className="sm-field__help">{error ?? help}</p> : null}
    </div>
  );
}

const checkEmail = (v: string) => (EMAIL.test(v.trim()) ? null : "Enter an email like name@church.org");
const checkNewPassword = (v: string) =>
  v.length >= MIN_PASSWORD && /[a-z]/i.test(v) && /\d/.test(v)
    ? null
    : `Use at least ${MIN_PASSWORD} characters with letters and numbers`;
const checkPassword = (v: string) => (v ? null : "Enter your password");
const checkRequired = (what: string) => (v: string) => (v.trim() ? null : `Enter ${what}`);

function FormError({ message }: { message: string | null }) {
  return message ? <p className="auth__error" role="alert">{message}</p> : null;
}

function AuthShell({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="auth">
      <header className="page-header">
        <h1 className="page-title">{title}</h1>
      </header>
      {children}
    </div>
  );
}

/** Sign in, create an account, or ask for a password reset. */
export function AuthForm({ root, notice }: { root: string; notice: string | null }) {
  const [mode, setMode] = useState<Mode>(initialMode);
  const [name, setName] = useState("");
  const [church, setChurch] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(notice);
  const [showErrors, setShowErrors] = useState(false);
  const [sent, setSent] = useState<{ kind: "signup" | "reset"; email: string } | null>(null);

  const switchMode = (next: Mode) => {
    setMode(next);
    setError(null);
    setShowErrors(false);
  };

  const google = async () => {
    setBusy(true);
    setError(null);
    const { error } = await supabase.auth.signInWithOAuth({
      provider: "google",
      options: { redirectTo: accountUrl(root) },
    });
    // On success the browser is already leaving for Google.
    if (error) {
      setError(authMessage(error));
      setBusy(false);
    }
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setShowErrors(true);
    const invalid =
      checkEmail(email) ||
      (mode === "signIn" && checkPassword(password)) ||
      (mode === "signUp" && (checkRequired("your name")(name) || checkRequired("your church's name")(church) || checkNewPassword(password)));
    if (invalid) return;

    setBusy(true);
    setError(null);
    const address = email.trim();
    try {
      if (mode === "signIn") {
        const { error } = await supabase.auth.signInWithPassword({ email: address, password });
        if (error) setError(authMessage(error));
      } else if (mode === "signUp") {
        const picked = pickedPlan();
        const { data, error } = await supabase.auth.signUp({
          email: address,
          password,
          options: {
            emailRedirectTo: accountUrl(root),
            data: {
              full_name: name.trim(),
              church_name: church.trim(),
              plan: picked?.plan.id ?? "pro",
              billing: picked?.billing ?? "yearly",
            },
          },
        });
        if (error) setError(authMessage(error));
        // With email confirmation on there's no session until the link is opened.
        else if (!data.session) setSent({ kind: "signup", email: address });
      } else {
        const { error } = await supabase.auth.resetPasswordForEmail(address, {
          redirectTo: accountUrl(root, { reset: "1" }),
        });
        if (error) setError(authMessage(error));
        else setSent({ kind: "reset", email: address });
      }
    } finally {
      setBusy(false);
    }
  };

  if (sent) return <CheckEmail root={root} sent={sent} onBack={() => { setSent(null); switchMode("signIn"); }} />;

  const title = mode === "signIn" ? "Sign in" : mode === "signUp" ? "Create your account" : "Reset your password";

  return (
    <AuthShell title={title}>
      {mode !== "reset" ? (
        <>
          <div className="sm-seg auth__seg" role="group" aria-label="Account">
            <button type="button" aria-pressed={mode === "signIn"} onClick={() => switchMode("signIn")}>
              Sign in
            </button>
            <button type="button" aria-pressed={mode === "signUp"} onClick={() => switchMode("signUp")}>
              Create account
            </button>
          </div>
          <button type="button" className="sm-btn sm-btn--lg auth__wide" onClick={google} disabled={busy}>
            Continue with Google
          </button>
          <p className="auth__or text-caption" aria-hidden="true">or with email</p>
        </>
      ) : (
        <p className="section-foot auth__lede">We'll email you a link to set a new password.</p>
      )}

      <form className="auth__form" onSubmit={submit} noValidate>
        {mode === "signUp" ? (
          <>
            <Field label="Your name" value={name} onChange={setName} autoComplete="name" check={checkRequired("your name")} showErrors={showErrors} />
            <Field label="Church" value={church} onChange={setChurch} autoComplete="organization" check={checkRequired("your church's name")} showErrors={showErrors} />
          </>
        ) : null}
        <Field label="Email" type="email" value={email} onChange={setEmail} autoComplete="email" check={checkEmail} showErrors={showErrors} />
        {mode !== "reset" ? (
          <Field
            label="Password"
            type="password"
            value={password}
            onChange={setPassword}
            autoComplete={mode === "signUp" ? "new-password" : "current-password"}
            help={mode === "signUp" ? `At least ${MIN_PASSWORD} characters with letters and numbers` : undefined}
            check={mode === "signUp" ? checkNewPassword : checkPassword}
            showErrors={showErrors}
          />
        ) : null}

        <FormError message={error} />

        <button type="submit" className="sm-btn sm-btn--primary sm-btn--lg auth__wide" disabled={busy}>
          {mode === "signIn" ? <LogIn aria-hidden="true" /> : mode === "signUp" ? <UserPlus aria-hidden="true" /> : <KeyRound aria-hidden="true" />}
          {mode === "signIn" ? "Sign in" : mode === "signUp" ? "Create account" : "Send reset link"}
        </button>
      </form>

      {mode === "signIn" ? (
        <button type="button" className="sm-btn sm-btn--ghost auth__wide" onClick={() => switchMode("reset")}>
          Forgot password
        </button>
      ) : null}
      {mode === "signUp" ? <p className="section-foot auth__foot">{trialNote()}</p> : null}
      {mode === "reset" ? (
        <button type="button" className="sm-btn sm-btn--ghost auth__wide" onClick={() => switchMode("signIn")}>
          <ArrowLeft aria-hidden="true" />
          Back to sign in
        </button>
      ) : null}
    </AuthShell>
  );
}

function CheckEmail({ root, sent, onBack }: { root: string; sent: { kind: "signup" | "reset"; email: string }; onBack: () => void }) {
  const [state, setState] = useState<"idle" | "busy" | "again" | string>("idle");

  const resend = async () => {
    setState("busy");
    const { error } =
      sent.kind === "signup"
        ? await supabase.auth.resend({ type: "signup", email: sent.email, options: { emailRedirectTo: accountUrl(root) } })
        : await supabase.auth.resetPasswordForEmail(sent.email, { redirectTo: accountUrl(root, { reset: "1" }) });
    setState(error ? authMessage(error) : "again");
  };

  return (
    <div className="auth">
      <div className="empty-state">
        <MailCheck aria-hidden="true" />
        <h1 className="text-heading">Check your email</h1>
        <p className="text-caption mix-muted">
          We sent a link to {sent.email}. Open it in this browser to {sent.kind === "signup" ? "finish signing up" : "set a new password"}.
        </p>
      </div>
      {state !== "idle" && state !== "busy" ? (
        state === "again" ? <p className="section-foot auth__foot" role="status">Sent again.</p> : <FormError message={state} />
      ) : null}
      <button type="button" className="sm-btn sm-btn--lg auth__wide" onClick={resend} disabled={state === "busy"}>
        Send again
      </button>
      <button type="button" className="sm-btn sm-btn--ghost auth__wide" onClick={onBack}>
        <ArrowLeft aria-hidden="true" />
        Back to sign in
      </button>
    </div>
  );
}

/** First sign-in with Google: name the church, which makes the person its Admin. */
export function ChurchForm({ name, onDone }: { name: string; onDone: () => void }) {
  const [church, setChurch] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showErrors, setShowErrors] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setShowErrors(true);
    if (!church.trim()) return;
    setBusy(true);
    setError(null);
    const picked = pickedPlan();
    const { error } = await supabase.rpc("create_church", {
      church_name: church.trim(),
      plan: picked?.plan.id ?? "pro",
      billing: picked?.billing ?? "yearly",
    });
    setBusy(false);
    if (error && !/already belong/i.test(error.message)) setError(authMessage(error));
    else onDone();
  };

  return (
    <AuthShell title={`Welcome, ${name}`}>
      <form className="auth__form" onSubmit={submit} noValidate>
        <Field label="Church" value={church} onChange={setChurch} autoComplete="organization" check={checkRequired("your church's name")} showErrors={showErrors} help="You'll be its admin and can invite your team later." />
        <FormError message={error} />
        <button type="submit" className="sm-btn sm-btn--primary sm-btn--lg auth__wide" disabled={busy}>
          Continue
        </button>
      </form>
      <p className="section-foot auth__foot">{trialNote()}</p>
    </AuthShell>
  );
}

/** Opened from a reset email: choose the new password. */
export function NewPasswordForm({ onDone }: { onDone: () => void }) {
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showErrors, setShowErrors] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setShowErrors(true);
    if (checkNewPassword(password)) return;
    setBusy(true);
    setError(null);
    const { error } = await supabase.auth.updateUser({ password });
    setBusy(false);
    if (error) setError(authMessage(error));
    else onDone();
  };

  return (
    <AuthShell title="Set a new password">
      <form className="auth__form" onSubmit={submit} noValidate>
        <Field
          label="New password"
          type="password"
          value={password}
          onChange={setPassword}
          autoComplete="new-password"
          help={`At least ${MIN_PASSWORD} characters with letters and numbers`}
          check={checkNewPassword}
          showErrors={showErrors}
        />
        <FormError message={error} />
        <button type="submit" className="sm-btn sm-btn--primary sm-btn--lg auth__wide" disabled={busy}>
          Save password
        </button>
      </form>
    </AuthShell>
  );
}
