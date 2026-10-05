// Sign-in for the browser demo (npm run dev), which has no core, no website
// round trip and no network. Signing in opens a demo church on a Pro trial, so
// every screen can be seen; there is no account behind it. The desktop app
// signs in through the website (crates/auth/src/supabase.rs).

import { accessFor, NO_ACCESS } from "./plans";
import type { Session } from "./types";

const KEY = "sanctuarymix.demoSignedIn";

const LOCAL: Session = {
  user: { id: "local", displayName: "Local operator", email: null },
  activeOrg: null,
  role: "admin",
  authenticated: false,
  access: NO_ACCESS,
};

const DEMO: Session = {
  user: { id: "demo-user", displayName: "Demo engineer", email: "demo@example.org" },
  activeOrg: { id: "demo-church", name: "Demo church" },
  role: "admin",
  authenticated: true,
  access: accessFor("pro", "trialing"),
};

function load(): boolean {
  try {
    return localStorage.getItem(KEY) === "1";
  } catch {
    return false;
  }
}

function save(on: boolean) {
  try {
    if (on) localStorage.setItem(KEY, "1");
    else localStorage.removeItem(KEY);
  } catch {
    // Sign-in still works for this run.
  }
}

export function createDemoAuth() {
  let signedIn = load();
  const session = () => (signedIn ? DEMO : LOCAL);
  return {
    session,
    signIn(): Session {
      signedIn = true;
      save(true);
      return session();
    },
    signOut(): Session {
      signedIn = false;
      save(false);
      return session();
    },
  };
}
