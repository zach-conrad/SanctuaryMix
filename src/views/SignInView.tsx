import { useState, type FormEvent } from "react";
import { useResolvedTheme } from "../components/TopBar";
import { useMixer } from "../store/mixer";

/**
 * Shown at launch until someone signs in or chooses to mix without an account.
 * Never a lock: the console always works without signing in.
 */
export function SignInView() {
  const theme = useMixer((s) => s.theme);
  const signIn = useMixer((s) => s.signIn);
  const workLocally = useMixer((s) => s.workLocally);
  const resolved = useResolvedTheme(theme);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

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

  return (
    <div className="signin" data-tauri-drag-region>
      <form className="signin-card" onSubmit={(e) => void submit(e)} noValidate>
        <img className="signin-wordmark" src={`/brand/wordmark-${resolved}.svg`} alt="SanctuaryMix" />
        <h1 className="text-title">Sign in</h1>
        <div className="sm-field" data-invalid={error ? "true" : undefined}>
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
        <div className="sm-field" data-invalid={error ? "true" : undefined}>
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
            aria-describedby={error ? "signin-error" : undefined}
          />
          {error && (
            <span className="sm-field__help" id="signin-error" role="alert">
              {error}
            </span>
          )}
        </div>
        <button className="sm-btn sm-btn--primary sm-btn--lg" type="submit" disabled={busy || !email || !password}>
          {busy ? "Signing in" : "Sign in"}
        </button>
        <div className="signin-or text-caption" aria-hidden>
          or
        </div>
        <button className="sm-btn sm-btn--lg" type="button" disabled title="Coming soon">
          Continue with Google
        </button>
        <button className="sm-btn sm-btn--ghost sm-btn--lg" type="button" onClick={workLocally}>
          Mix without signing in
        </button>
        <p className="signin-foot text-caption">The console works without an account.</p>
      </form>
    </div>
  );
}
