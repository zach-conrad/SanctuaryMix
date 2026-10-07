// A church Admin adds someone to their team from the Account page.
// Deployed with JWT verification on. The database checks, with the Admin's
// own token, that they are their church's Admin and the plan has room
// (team_invite_check). Then, with the service role: an existing account with
// no church joins straight away; anyone else gets Supabase's invite email and
// joins as they set their password on the website.

import { ACCOUNT_URL, UUID, corsHeaders, getCaller, reply, serviceClient } from "../_shared/caller.ts";

const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const ROLES = ["admin", "engineer", "volunteer"];

Deno.serve(async (req) => {
  if (req.method === "OPTIONS") return new Response(null, { status: 204, headers: corsHeaders(req) });
  if (req.method !== "POST") return reply(req, 405, { msg: "Use POST" });

  const caller = getCaller(req);
  if (!caller) return reply(req, 401, { msg: "Sign in again" });

  let orgId: unknown, email: unknown, role: unknown;
  try {
    ({ org_id: orgId, email, role } = await req.json());
  } catch {
    return reply(req, 400, { msg: "Send an email and role" });
  }
  if (typeof orgId !== "string" || !UUID.test(orgId)) return reply(req, 400, { msg: "Unknown church" });
  if (typeof email !== "string" || !EMAIL.test(email.trim()) || email.length > 254)
    return reply(req, 400, { msg: "Enter an email like name@church.org" });
  if (typeof role !== "string" || !ROLES.includes(role)) return reply(req, 400, { msg: "Pick Admin, Engineer or Volunteer" });
  const address = email.trim().toLowerCase();

  const { error: checkError } = await caller.asCaller.rpc("team_invite_check", { org: orgId });
  if (checkError) {
    console.error("team_invite_check failed", checkError.code, checkError.message);
    return reply(req, 403, { msg: checkError.message });
  }

  const admin = serviceClient();
  const add = () => admin.rpc("team_add_member", { org: orgId, member_email: address, new_role: role, actor: caller.id });

  const { data: result, error } = await add();
  if (error) return reply(req, 400, { msg: error.message });

  if (result === "new" || result === "added_unconfirmed") {
    const { error: inviteError } = await admin.auth.admin.inviteUserByEmail(address, { redirectTo: ACCOUNT_URL });
    if (inviteError) {
      console.error("invite failed", inviteError.message);
      const limited = /rate limit/i.test(inviteError.message);
      return reply(req, limited ? 429 : 500, {
        msg: limited ? "Too many emails for now. Wait a few minutes and try again" : "The invite didn't send. Try again",
      });
    }
    if (result === "new") {
      const { error: joinError } = await add();
      if (joinError) return reply(req, 400, { msg: joinError.message });
    }
    return reply(req, 200, { msg: `Invite sent to ${address}` });
  }

  return reply(req, 200, { msg: `${address} joined your team` });
});
