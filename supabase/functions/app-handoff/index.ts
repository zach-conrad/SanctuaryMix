// Finishes the desktop app's sign-in through the website (see
// migrations/20261005200000_app_handoff.sql). The app posts the one-time code
// from its sanctuarymix:// link and its PKCE secret; this returns a new
// session for the app. Deployed with JWT verification off: the app holds only
// the publishable key, and the code plus secret are the proof.

import { createClient } from "npm:@supabase/supabase-js@2";

const headers = { "content-type": "application/json" };

function reply(status: number, body: unknown) {
  return new Response(JSON.stringify(body), { status, headers });
}

function refuse(msg: string) {
  return reply(400, { error_code: "handoff_invalid", msg });
}

async function challengeOf(verifier: string): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier)));
  return btoa(String.fromCharCode(...digest)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

Deno.serve(async (req) => {
  if (req.method !== "POST") return reply(405, { error_code: "method_not_allowed", msg: "Use POST" });

  let code: unknown, verifier: unknown;
  try {
    ({ code, code_verifier: verifier } = await req.json());
  } catch {
    return refuse("Send a code and code_verifier");
  }
  if (typeof code !== "string" || !/^[0-9a-f]{64}$/.test(code)) return refuse("That sign-in link isn't valid");
  if (typeof verifier !== "string" || !/^[A-Za-z0-9_-]{43,128}$/.test(verifier)) return refuse("That sign-in link isn't valid");

  const admin = createClient(Deno.env.get("SUPABASE_URL")!, Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!, {
    auth: { persistSession: false, autoRefreshToken: false },
  });

  const { data: rows, error } = await admin.rpc("redeem_app_handoff", { handoff_code: code });
  if (error) {
    console.error("redeem failed", error.message);
    return reply(500, { error_code: "server_error", msg: "Sign-in didn't finish. Try again" });
  }
  const row = rows?.[0] as { user_id: string; email: string | null; challenge: string } | undefined;
  if (!row) return refuse("That sign-in link has expired or was already used. Sign in again from SanctuaryMix");
  if ((await challengeOf(verifier)) !== row.challenge) return refuse("That sign-in link wasn't started by this app");
  if (!row.email) return refuse("This account has no email address");

  // A fresh session for the app, separate from the browser's.
  const { data: link, error: linkError } = await admin.auth.admin.generateLink({ type: "magiclink", email: row.email });
  if (linkError || !link?.properties?.hashed_token) {
    console.error("generateLink failed", linkError?.message);
    return reply(500, { error_code: "server_error", msg: "Sign-in didn't finish. Try again" });
  }
  const { data: verified, error: verifyError } = await admin.auth.verifyOtp({
    type: "magiclink",
    token_hash: link.properties.hashed_token,
  });
  const session = verified?.session;
  if (verifyError || !session || session.user.id !== row.user_id) {
    console.error("verifyOtp failed", verifyError?.message);
    return reply(500, { error_code: "server_error", msg: "Sign-in didn't finish. Try again" });
  }

  return reply(200, {
    access_token: session.access_token,
    refresh_token: session.refresh_token,
    expires_in: session.expires_in,
    user: session.user,
  });
});
