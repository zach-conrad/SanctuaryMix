// The browser demo's copy of the sample account in crates/auth/src/sample.rs:
// test@example.com / 1234 signs in as an admin on Campus, so everything is on.
// Not secrets; the core's provider is the one the app uses.

import { accessFor, NO_ACCESS } from "./plans";
import type { Session } from "./types";

const SAMPLE_EMAIL = "test@example.com";
const SAMPLE_PASSWORD = "1234";
const KEY = "sanctuarymix.demoSignedIn";

const LOCAL: Session = {
  user: { id: "local", displayName: "Local operator", email: null },
  activeOrg: null,
  role: "admin",
  authenticated: false,
  access: NO_ACCESS,
};

const SAMPLE: Session = {
  user: { id: "sample-user", displayName: "Test User", email: SAMPLE_EMAIL },
  activeOrg: { id: "sample-church", name: "Sample Church" },
  role: "admin",
  authenticated: true,
  access: accessFor("campus", "active"),
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
  const session = () => (signedIn ? SAMPLE : LOCAL);
  return {
    session,
    async signIn(email: string, password: string): Promise<Session> {
      if (email.trim().toLowerCase() !== SAMPLE_EMAIL || password !== SAMPLE_PASSWORD) {
        throw "That email and password don't match. Check them and try again.";
      }
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
