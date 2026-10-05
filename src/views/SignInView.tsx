import { useState, type FormEvent } from "react";
import { getBackend } from "../lib/backend";
import { useResolvedTheme } from "../components/TopBar";
import { useMixer } from "../store/mixer";

/**
 * Shown at launch until someone signs in or chooses to mix without an account.
 * Never a lock: the console always works without signing in. Accounts are
 * made on the website; the app signs in with email and password, or with
 * Google in the browser.
 */
export function SignInView() {
  const theme = useMixer((s) => s.theme);
  const signIn = useMixer((s) => s.signIn);
  const signInWithGoogle = useMixer((s) => s.signInWithGoogle);
  const googleError = useMixer((s) => s.signInError);
  const workLocally = useMixer((s) => s.workLocally);
  const resolved = useResolvedTheme(theme);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [waitingForGoogle, setWaitingForGoogle] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const shown = error ?? (waitingForGoogle ? null : googleError);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await signIn(email, password);
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  const google = async () => {
    setError(null);
    try {
      await signInWithGoogle();
      setWaitingForGoogle(true);
    } catch (e) {
      setError(String(e));
    }
  };

  const openWebsite = async (page: "signUp" | "resetPassword") => {
    try {
      await (await getBackend()).openAccountPage(page);
    } catch (e) {
      setError(String(e));
    }
  };

  if (waitingForGoogle && !googleError) {
    return (
      <div className="signin" data-tauri-drag-region>
        <div className="signin-card">
          <img className="signin-wordmark" src={`/brand/wordmark-${resolved}.svg`} alt="SanctuaryMix" />
          <h1 className="text-title">Finish in your browser</h1>
          <p className="signin-lede">Sign in with Google there. SanctuaryMix opens again when you're done.</p>
          <button className="sm-btn sm-btn--lg" type="button" onClick={() => void google()}>
            Open the browser again
          </button>
          <button className="sm-btn sm-btn--ghost sm-btn--lg" type="button" onClick={() => setWaitingForGoogle(false)}>
            Cancel
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="signin" data-tauri-drag-region>
      <form className="signin-card" onSubmit={(e) => void submit(e)} noValidate>
        <img className="signin-wordmark" src={`/brand/wordmark-${resolved}.svg`} alt="SanctuaryMix" />
        <h1 className="text-title">Sign in</h1>
        <button className="sm-btn sm-btn--lg" type="button" onClick={() => void google()}>
          Continue with Google
        </button>
        <div className="signin-or text-caption" aria-hidden>
          or with email
        </div>
        <div className="sm-field" data-invalid={shown ? "true" : undefined}>
          <label className="sm-field__label" htmlFor="signin-email">
            Email
          </label>
          <input
            id="signin-email"
            className="sm-input"
            type="email"
            autoComplete="username"
            autoFocus
            value={email}
            onChange={(e) => setEmail(e.target.value)}
          />
        </div>
        <div className="sm-field" data-invalid={shown ? "true" : undefined}>
          <label className="sm-field__label" htmlFor="signin-password">
            Password
          </label>
          <input
            id="signin-password"
            className="sm-input"
            type="password"
            autoComplete="current-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            aria-describedby={shown ? "signin-error" : undefined}
          />
          {shown && (
            <span className="sm-field__help" id="signin-error" role="alert">
              {shown}
            </span>
          )}
        </div>
        <button className="sm-btn sm-btn--primary sm-btn--lg" type="submit" disabled={busy || !email || !password}>
          {busy ? "Signing in" : "Sign in"}
        </button>
        <div className="signin-links">
          <button className="sm-btn sm-btn--ghost" type="button" onClick={() => void openWebsite("resetPassword")}>
            Forgot password
          </button>
          <button className="sm-btn sm-btn--ghost" type="button" onClick={() => void openWebsite("signUp")}>
            Create account
          </button>
        </div>
        <button className="sm-btn sm-btn--ghost sm-btn--lg" type="button" onClick={workLocally}>
          Mix without signing in
        </button>
        <p className="signin-foot text-caption">The console works without an account.</p>
      </form>
    </div>
  );
}
