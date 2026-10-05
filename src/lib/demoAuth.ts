// Sign-in for the browser demo (npm run dev), which has no core and no
// network. Any email and password opens a demo church on a Pro trial, so every
// screen can be seen; there is no account behind it. The desktop app signs in
// with Supabase in the core (crates/auth/src/supabase.rs).

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

function demoSession(email: string): Session {
  const name = email.split("@")[0] || "Demo";
  return {
    user: { id: "demo-user", displayName: name.charAt(0).toUpperCase() + name.slice(1), email },
    activeOrg: { id: "demo-church", name: "Demo church" },
    role: "admin",
    authenticated: true,
    access: accessFor("pro", "trialing"),
  };
}

function load(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

function save(email: string | null) {
  try {
    if (email) localStorage.setItem(KEY, email);
    else localStorage.removeItem(KEY);
  } catch {
    // Sign-in still works for this run.
  }
}

export function createDemoAuth() {
  let email = load();
  const session = () => (email ? demoSession(email) : LOCAL);
  return {
    session,
    async signIn(address: string, password: string): Promise<Session> {
      if (!/^[^\s@]+@[^\s@]+$/.test(address.trim()) || !password) {
        throw "Enter an email and password. In the browser demo, any will do.";
      }
      email = address.trim().toLowerCase();
      save(email);
      return session();
    },
    signOut(): Session {
      email = null;
      save(null);
      return session();
    },
  };
}
