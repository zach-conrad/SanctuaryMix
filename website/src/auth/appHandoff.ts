// Signing in to the desktop app through the website.
//
// The app opens /account/?app_challenge=<S256 of a secret it keeps>. Once the
// person is signed in here (which can take an email link or a trip to Google,
// so the challenge waits in localStorage), the website asks Supabase for a
// one-time code bound to that challenge and opens sanctuarymix://auth/callback
// with it. Only the app holding the secret can trade the code for a session
// (supabase/functions/app-handoff), and it expires in five minutes.

import { supabase } from "./client";

const KEY = "sanctuarymix.appChallenge";
const KEEP_MS = 15 * 60_000;
const CHALLENGE = /^[A-Za-z0-9_-]{43}$/;
export const APP_CALLBACK = "sanctuarymix://auth/callback";

// Used when localStorage is blocked, for this page load only.
let memory: string | null = null;
// One code per handoff, however many times the page asks.
let minting: Promise<string> | null = null;

function read(): string | null {
  try {
    const saved = JSON.parse(window.localStorage.getItem(KEY) ?? "null") as { challenge?: string; until?: number } | null;
    if (saved?.challenge && CHALLENGE.test(saved.challenge) && (saved.until ?? 0) > Date.now()) return saved.challenge;
  } catch {
    // Unreadable or blocked storage: no handoff waiting.
  }
  return null;
}

function forget() {
  try {
    window.localStorage.removeItem(KEY);
  } catch {
    // Nothing to clear.
  }
}

/** Keep the app's challenge from the URL, then take it out of the address bar. Call before rendering. */
export function captureAppChallenge() {
  const url = new URL(window.location.href);
  const challenge = url.searchParams.get("app_challenge");
  if (challenge === null) return;
  url.searchParams.delete("app_challenge");
  window.history.replaceState(null, "", url.toString());
  if (!CHALLENGE.test(challenge)) return;
  try {
    window.localStorage.setItem(KEY, JSON.stringify({ challenge, until: Date.now() + KEEP_MS }));
  } catch {
    // Storage blocked: the handoff still works if sign-in finishes on this page load.
    memory = challenge;
  }
}

/** True while the app is waiting for this browser to finish signing in. */
export function appWaiting(): boolean {
  return (memory ?? read()) !== null;
}

/** Stop handing off (the person chose to stay on the website). */
export function cancelAppHandoff() {
  memory = null;
  forget();
}

/** Mint the one-time code for the waiting app and return the link that opens it. */
export function appSignInLink(): Promise<string> {
  minting ??= (async () => {
    const challenge = memory ?? read();
    if (!challenge) throw new Error("The app isn't waiting for a sign-in. Choose Sign in in the app again.");
    const { data, error } = await supabase.rpc("create_app_handoff", { challenge });
    if (error) {
      minting = null;
      throw error;
    }
    memory = null;
    forget();
    return `${APP_CALLBACK}?code=${encodeURIComponent(String(data))}`;
  })();
  return minting;
}
