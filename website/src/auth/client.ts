// The website's Supabase client. Accounts only: email and password or Google,
// with churches and roles from the tables in supabase/migrations.
//
// The URL and publishable key are public by design (they ship to every
// browser); row-level security decides what each person can read. Override
// them with VITE_SUPABASE_URL / VITE_SUPABASE_PUBLISHABLE_KEY to point a build
// at another project. Never put a secret or service_role key here.

import { createClient } from "@supabase/supabase-js";

const DEFAULT_URL = "https://pfymsavkmnnxpbayyrsw.supabase.co";
const DEFAULT_KEY = "sb_publishable_QBbXzdnsQJOjNElfeJwhTQ_5MQYAwD8"; // gitleaks:allow (publishable, not a secret)

export const supabase = createClient(
  import.meta.env.VITE_SUPABASE_URL || DEFAULT_URL,
  import.meta.env.VITE_SUPABASE_PUBLISHABLE_KEY || DEFAULT_KEY,
  {
    auth: {
      flowType: "pkce",
      detectSessionInUrl: true,
      persistSession: true,
      autoRefreshToken: true,
    },
  },
);

export const MIN_PASSWORD = 10;

/** The account page's absolute URL, keeping the plan picked on the pricing section. */
export function accountUrl(root: string, extra: Record<string, string> = {}): string {
  const here = new URLSearchParams(window.location.search);
  const params = new URLSearchParams();
  for (const key of ["plan", "billing"]) {
    const value = here.get(key);
    if (value) params.set(key, value);
  }
  for (const [key, value] of Object.entries(extra)) params.set(key, value);
  const query = params.toString();
  return new URL(`${root}account/${query ? `?${query}` : ""}`, window.location.href).toString();
}

/** Plain words for the errors people actually hit. */
export function authMessage(error: { message?: string; code?: string } | null | undefined): string {
  const code = error?.code ?? "";
  const message = error?.message ?? "";
  if (code === "invalid_credentials" || /invalid login credentials/i.test(message))
    return "That email and password don't match. Try again or reset your password.";
  if (code === "email_not_confirmed" || /email not confirmed/i.test(message))
    return "Confirm your email first. Open the link we sent you, or send it again.";
  if (code === "user_already_exists" || /already registered/i.test(message))
    return "There's already an account with that email. Sign in instead.";
  if (code === "weak_password" || /password/i.test(message) && /(short|weak|characters)/i.test(message))
    return `Use a longer password, at least ${MIN_PASSWORD} characters with letters and numbers.`;
  if (code === "over_email_send_rate_limit" || code === "over_request_rate_limit" || /rate limit/i.test(message))
    return "Too many tries for now. Wait a few minutes and try again.";
  if (/pkce|code verifier|flow state/i.test(message))
    return "Open the link in the same browser you signed up in, or sign in with your password.";
  if (/fetch|network/i.test(message)) return "We couldn't reach the server. Check your connection and try again.";
  return message ? `${message.replace(/\.$/, "")}.` : "Something went wrong. Try again.";
}
