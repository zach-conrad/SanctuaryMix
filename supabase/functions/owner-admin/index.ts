// Owner Admin page actions that need the auth admin API: send a password
// reset, resend a confirmation or invite email, disable or re-enable sign-in.
// (Trial, plan and sign-out-everywhere are plain database functions; see
// migrations/20261007180000_owner_admin.sql.)
//
// Deployed with JWT verification on. The caller must be in
// private.platform_admins AND have passed two-factor this session; the
// database answers that from the caller's own token (owner_status) before the
// service role is touched. Every action is written to the audit log.

import { ACCOUNT_URL, UUID, corsHeaders, getCaller, reply, serviceClient } from "../_shared/caller.ts";

const ACTIONS = ["reset_password", "resend_email", "disable", "enable"] as const;
type Action = (typeof ACTIONS)[number];

// Supabase's way to say "until further notice": about a hundred years.
const FOREVER = "876000h";

Deno.serve(async (req) => {
  if (req.method === "OPTIONS") return new Response(null, { status: 204, headers: corsHeaders(req) });
  if (req.method !== "POST") return reply(req, 405, { msg: "Use POST" });

  const caller = getCaller(req);
  if (!caller) return reply(req, 401, { msg: "Sign in again" });

  const { data: status, error: statusError } = await caller.asCaller.rpc("owner_status");
  if (statusError) {
    console.error("owner_status failed", statusError.code, statusError.message);
    return reply(req, 500, { msg: "Couldn't check your access. Try again" });
  }
  if (!status?.owner) return reply(req, 404, { msg: "Not found" });
  if (!status?.mfa) return reply(req, 403, { msg: "Enter your two-factor code first" });

  let action: unknown, userId: unknown;
  try {
    ({ action, user_id: userId } = await req.json());
  } catch {
    return reply(req, 400, { msg: "Send an action and user_id" });
  }
  if (typeof action !== "string" || !ACTIONS.includes(action as Action)) return reply(req, 400, { msg: "Unknown action" });
  if (typeof userId !== "string" || !UUID.test(userId)) return reply(req, 400, { msg: "Unknown account" });

  const admin = serviceClient();
  const { data: found, error: findError } = await admin.auth.admin.getUserById(userId);
  const user = found?.user;
  if (findError || !user) return reply(req, 404, { msg: "That account no longer exists" });
  if (!user.email) return reply(req, 400, { msg: "This account has no email address" });

  let done: string;
  let error: { message: string } | null = null;
  switch (action as Action) {
    case "reset_password": {
      ({ error } = await admin.auth.resetPasswordForEmail(user.email, { redirectTo: ACCOUNT_URL }));
      done = `Password reset sent to ${user.email}`;
      break;
    }
    case "resend_email": {
      if (user.email_confirmed_at) return reply(req, 400, { msg: "Their email is already confirmed" });
      if (user.invited_at) {
        ({ error } = await admin.auth.admin.inviteUserByEmail(user.email, { redirectTo: ACCOUNT_URL }));
        done = `Invite sent again to ${user.email}`;
      } else {
        ({ error } = await admin.auth.resend({ type: "signup", email: user.email, options: { emailRedirectTo: ACCOUNT_URL } }));
        done = `Confirmation sent again to ${user.email}`;
      }
      break;
    }
    case "disable": {
      if (user.id === caller.id) return reply(req, 400, { msg: "You can't disable your own account" });
      ({ error } = await admin.auth.admin.updateUserById(user.id, { ban_duration: FOREVER }));
      done = `${user.email} can no longer sign in`;
      break;
    }
    case "enable": {
      ({ error } = await admin.auth.admin.updateUserById(user.id, { ban_duration: "none" }));
      done = `${user.email} can sign in again`;
      break;
    }
  }

  if (error) {
    console.error(action, "failed", error.message);
    const limited = /rate limit/i.test(error.message);
    return reply(req, limited ? 429 : 500, {
      msg: limited ? "Too many emails for now. Wait a few minutes and try again" : "That didn't go through. Try again",
    });
  }

  const { error: logError } = await admin.rpc("admin_record", {
    actor: caller.id,
    action: `account.${action}`,
    target_user: user.id,
    target_org: null,
    detail: {},
  });
  if (logError) console.error("audit failed", logError.message);

  return reply(req, 200, { msg: done });
});
