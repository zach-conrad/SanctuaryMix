// Who is signed in and the church they belong to, kept current as Supabase
// sessions change. Pages render from `AccountState` only.

import type { Session, User } from "@supabase/supabase-js";
import { useCallback, useEffect, useRef, useState } from "react";
import type { CloudSession, Role } from "../cloud/types";
import { authMessage, supabase } from "./client";

export interface Church {
  id: string;
  name: string;
  role: Role;
  plan: "essentials" | "pro" | "campus";
  billing: "monthly" | "yearly";
  trialEndsAt: Date;
}

export interface Account {
  user: User;
  name: string;
  email: string;
  church: Church;
}

export type AccountState =
  | { status: "loading" }
  /** `notice` is an error carried back from an email or Google link. */
  | { status: "signedOut"; notice: string | null }
  /** Opened a password-reset link: set a new password before anything else. */
  | { status: "recovery"; user: User }
  /** Signed in but not in a church yet (new Google sign-ups). */
  | { status: "needsChurch"; user: User; name: string }
  | { status: "ready"; account: Account }
  | { status: "error"; message: string };

export function displayName(user: User): string {
  const meta = user.user_metadata ?? {};
  const name = (meta.full_name || meta.name || "").toString().trim();
  return name || (user.email ?? "").split("@")[0];
}

export function cloudSession(account: Account): CloudSession {
  return {
    userId: account.user.id,
    name: account.name,
    orgId: account.church.id,
    orgName: account.church.name,
    role: account.church.role,
  };
}

interface UrlState {
  notice: string | null;
  /** Set a new password before anything else (reset link or invite). */
  reset: boolean;
  /** A session in the #hash, from an email the server sent (invite, owner-sent reset). */
  tokens: { access_token: string; refresh_token: string } | null;
}

/** Errors Supabase puts in the URL when an email or Google link fails. Read once, then cleaned up. */
function takeUrlState(): UrlState {
  const url = new URL(window.location.href);
  const hash = new URLSearchParams(url.hash.replace(/^#/, ""));
  const description = url.searchParams.get("error_description") ?? hash.get("error_description");
  const code = url.searchParams.get("error_code") ?? hash.get("error_code");
  const access = hash.get("access_token");
  const refreshToken = hash.get("refresh_token");
  const tokens = access && refreshToken ? { access_token: access, refresh_token: refreshToken } : null;
  const linkType = hash.get("type");
  const reset = url.searchParams.get("reset") === "1" || (tokens !== null && (linkType === "recovery" || linkType === "invite"));
  let notice: string | null = null;
  if (code === "otp_expired") notice = "That link has expired or was already used. Sign in, or ask for a new one.";
  else if (description) notice = authMessage({ message: description.replace(/\+/g, " ") });
  return { notice, reset, tokens };
}

function cleanUrl() {
  const url = new URL(window.location.href);
  let changed = false;
  for (const key of ["code", "error", "error_code", "error_description", "reset"]) {
    if (url.searchParams.has(key)) {
      url.searchParams.delete(key);
      changed = true;
    }
  }
  if (/error|access_token/.test(url.hash)) {
    url.hash = "";
    changed = true;
  }
  if (changed) window.history.replaceState(null, "", url.toString());
}

interface MembershipRow {
  role: Role;
  organizations: {
    id: string;
    name: string;
    plan: Church["plan"];
    billing: Church["billing"];
    trial_ends_at: string;
  } | null;
}

async function loadChurch(userId: string): Promise<Church | null> {
  const { data, error } = await supabase
    .from("memberships")
    .select("role, organizations(id, name, plan, billing, trial_ends_at)")
    .eq("user_id", userId)
    .order("created_at", { ascending: true })
    .limit(1)
    .returns<MembershipRow[]>();
  if (error) throw error;
  const row = data?.[0];
  if (!row?.organizations) return null;
  const org = row.organizations;
  return {
    id: org.id,
    name: org.name,
    role: row.role,
    plan: org.plan,
    billing: org.billing,
    trialEndsAt: new Date(org.trial_ends_at),
  };
}

/** Make the church named at sign-up, if the person gave one and has none yet. */
async function createChurchFromSignUp(user: User): Promise<Church | null> {
  const meta = user.user_metadata ?? {};
  const churchName = (meta.church_name ?? "").toString().trim();
  if (!churchName) return null;
  const { error } = await supabase.rpc("create_church", {
    church_name: churchName,
    plan: (meta.plan ?? "pro").toString(),
    billing: (meta.billing ?? "yearly").toString(),
  });
  // Another tab may have made it first; either way, read it back.
  if (error && !/already belong/i.test(error.message)) throw error;
  return loadChurch(user.id);
}

export function useAccount() {
  const [state, setState] = useState<AccountState>({ status: "loading" });
  // True between opening a password-reset link and saving the new password.
  const recovery = useRef(false);

  const resolve = useCallback(async (session: Session | null, notice: string | null = null) => {
    if (!session) {
      recovery.current = false;
      setState({ status: "signedOut", notice });
      return;
    }
    const user = session.user;
    if (recovery.current) {
      setState({ status: "recovery", user });
      return;
    }
    try {
      const church = (await loadChurch(user.id)) ?? (await createChurchFromSignUp(user));
      if (!church) {
        setState({ status: "needsChurch", user, name: displayName(user) });
        return;
      }
      setState({
        status: "ready",
        account: { user, name: displayName(user), email: user.email ?? "", church },
      });
    } catch (e) {
      setState({ status: "error", message: authMessage(e as { message?: string }) });
    }
  }, []);

  useEffect(() => {
    const { notice, reset, tokens } = takeUrlState();
    recovery.current = reset;
    let live = true;
    let lastUser: string | null = null;

    // getSession waits for the client to finish reading ?code= from the URL.
    // A session in the #hash (an invite or reset sent by the server) replaces it.
    const start = tokens
      ? supabase.auth.setSession(tokens).then(({ data, error }) => ({ data: { session: data.session }, error }))
      : supabase.auth.getSession();
    start.then(({ data, error }) => {
      if (!live) return;
      cleanUrl();
      lastUser = data.session?.user.id ?? null;
      if (tokens && !data.session) recovery.current = false;
      void resolve(data.session, error ? authMessage(error) : notice);
    });

    const { data: sub } = supabase.auth.onAuthStateChange((event, session) => {
      if (!live) return;
      if (event === "PASSWORD_RECOVERY") recovery.current = true;
      const userId = session?.user.id ?? null;
      // Token refreshes and profile edits keep the same person; only re-read on a real change.
      if (event === "INITIAL_SESSION") return;
      if (event !== "PASSWORD_RECOVERY" && event !== "SIGNED_OUT" && userId === lastUser) return;
      lastUser = userId;
      // Supabase asks not to await its own calls inside this callback.
      window.setTimeout(() => void resolve(session), 0);
    });

    return () => {
      live = false;
      sub.subscription.unsubscribe();
    };
  }, [resolve]);

  /** Re-read after a change the session events don't cover (church made, new password saved). */
  const refresh = useCallback(async () => {
    recovery.current = false;
    const { data } = await supabase.auth.getSession();
    await resolve(data.session);
  }, [resolve]);

  return { state, refresh };
}

export async function signOut(root: string) {
  await supabase.auth.signOut({ scope: "local" });
  window.location.href = root;
}
