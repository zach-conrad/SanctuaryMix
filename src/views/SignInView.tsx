import { useState } from "react";
import { useResolvedTheme } from "../components/TopBar";
import { useMixer } from "../store/mixer";

/**
 * Shown at launch until someone signs in or chooses to mix without an account.
 * Never a lock: the console always works without signing in. Signing in (and
 * creating an account, or resetting a password) happens on the website in the
 * browser, which hands the sign-in back to the app.
 */
export function SignInView() {
  const theme = useMixer((s) => s.theme);
  const signIn = useMixer((s) => s.signIn);
  const returnedError = useMixer((s) => s.signInError);
  const workLocally = useMixer((s) => s.workLocally);
  const resolved = useResolvedTheme(theme);
  const [waiting, setWaiting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const shown = error ?? returnedError;

  const start = async () => {
    setError(null);
    try {
      await signIn();
      setWaiting(true);
    } catch (e) {
      setError(String(e));
    }
  };

  if (waiting && !shown) {
    return (
      <div className="signin" data-tauri-drag-region>
        <div className="signin-card">
          <img className="signin-wordmark" src={`/brand/wordmark-${resolved}.svg`} alt="SanctuaryMix" />
          <h1 className="text-title">Finish in your browser</h1>
          <p className="signin-lede">Sign in on the SanctuaryMix website. The app opens again when you're done.</p>
          <button className="sm-btn sm-btn--lg" type="button" onClick={() => void start()}>
            Open the website again
          </button>
          <button className="sm-btn sm-btn--ghost sm-btn--lg" type="button" onClick={() => setWaiting(false)}>
            Cancel
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="signin" data-tauri-drag-region>
      <div className="signin-card">
        <img className="signin-wordmark" src={`/brand/wordmark-${resolved}.svg`} alt="SanctuaryMix" />
        <h1 className="text-title">Sign in</h1>
        <p className="signin-lede">Use your SanctuaryMix account on the website. New here? You can create one there.</p>
        <button className="sm-btn sm-btn--primary sm-btn--lg" type="button" onClick={() => void start()}>
          Sign in with your browser
        </button>
        {shown && (
          <p className="signin-error text-caption" role="alert">
            {shown}
          </p>
        )}
        <button className="sm-btn sm-btn--ghost sm-btn--lg" type="button" onClick={workLocally}>
          Mix without signing in
        </button>
        <p className="signin-foot text-caption">The console works without an account.</p>
      </div>
    </div>
  );
}
