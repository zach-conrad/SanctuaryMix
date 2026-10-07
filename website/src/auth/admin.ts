// Calls behind the owner Admin page and the church Team section. Every rule
// lives in the database (migrations/20261007180000_owner_admin.sql) and the
// owner-admin / team-invite Edge Functions; this file only asks. The browser
// never holds anything more powerful than the signed-in person's own session.

import { FunctionsHttpError } from "@supabase/supabase-js";
import type { Role } from "../cloud/types";
import { authMessage, supabase } from "./client";

export type PlanId = "essentials" | "pro" | "campus";
export type BillingId = "monthly" | "yearly";

export interface OwnerStatus {
  owner: boolean;
  /** This session passed two-factor (aal2). */
  mfa: boolean;
}

export interface Overview {
  churches: number;
  accounts: number;
  trials_ending_week: number;
  trials_ended: number;
  signups_week: number;
}

export interface AdminChurch {
  id: string;
  name: string;
  plan: PlanId;
  billing: BillingId;
  trialEndsAt: Date;
  createdAt: Date;
  members: number;
  adminEmail: string | null;
}

export interface AdminAccount {
  id: string;
  email: string;
  name: string | null;
  provider: string;
  confirmed: boolean;
  invited: boolean;
  createdAt: Date;
  lastSignInAt: Date | null;
  disabled: boolean;
  owner: boolean;
  orgId: string | null;
  orgName: string | null;
  role: Role | null;
}

export interface AuditEntry {
  createdAt: Date;
  actorEmail: string | null;
  action: string;
  target: string | null;
  detail: Record<string, unknown>;
}

export interface TeamMember {
  userId: string;
  email: string;
  name: string | null;
  role: Role;
  joinedAt: Date;
  lastSignInAt: Date | null;
  /** Invited but hasn't set a password yet. */
  pending: boolean;
}

const date = (v: string | null) => (v ? new Date(v) : null);

async function rpc<T>(fn: string, args?: Record<string, unknown>): Promise<T> {
  const { data, error } = await supabase.rpc(fn, args);
  if (error) throw new Error(authMessage(error));
  return data as T;
}

/** Edge Function call; errors carry the function's own plain message. */
async function invoke(fn: string, body: Record<string, unknown>): Promise<string> {
  const { data, error } = await supabase.functions.invoke<{ msg: string }>(fn, { body });
  if (error) {
    if (error instanceof FunctionsHttpError) {
      const payload = (await error.context.json().catch(() => null)) as { msg?: string } | null;
      if (payload?.msg) throw new Error(`${payload.msg.replace(/\.$/, "")}.`);
    }
    throw new Error(authMessage(error));
  }
  return `${(data?.msg ?? "Done").replace(/\.$/, "")}.`;
}

// ---- owner ----

export const ownerStatus = () => rpc<OwnerStatus>("owner_status");
export const loadOverview = () => rpc<Overview>("admin_overview");

interface ChurchRow {
  id: string;
  name: string;
  plan: PlanId;
  billing: BillingId;
  trial_ends_at: string;
  created_at: string;
  members: number;
  admin_email: string | null;
}

export async function loadChurches(): Promise<AdminChurch[]> {
  const rows = await rpc<ChurchRow[]>("admin_list_churches");
  return rows.map((r) => ({
    id: r.id,
    name: r.name,
    plan: r.plan,
    billing: r.billing,
    trialEndsAt: new Date(r.trial_ends_at),
    createdAt: new Date(r.created_at),
    members: Number(r.members),
    adminEmail: r.admin_email,
  }));
}

interface AccountRow {
  id: string;
  email: string;
  name: string | null;
  provider: string;
  confirmed: boolean;
  invited: boolean;
  created_at: string;
  last_sign_in_at: string | null;
  disabled: boolean;
  owner: boolean;
  org_id: string | null;
  org_name: string | null;
  role: Role | null;
}

export async function loadAccounts(): Promise<AdminAccount[]> {
  const rows = await rpc<AccountRow[]>("admin_list_accounts");
  return rows.map((r) => ({
    id: r.id,
    email: r.email,
    name: r.name,
    provider: r.provider,
    confirmed: r.confirmed,
    invited: r.invited,
    createdAt: new Date(r.created_at),
    lastSignInAt: date(r.last_sign_in_at),
    disabled: r.disabled,
    owner: r.owner,
    orgId: r.org_id,
    orgName: r.org_name,
    role: r.role,
  }));
}

interface AuditRow {
  created_at: string;
  actor_email: string | null;
  action: string;
  target: string | null;
  detail: Record<string, unknown> | null;
}

export async function loadRecentChanges(): Promise<AuditEntry[]> {
  const rows = await rpc<AuditRow[]>("admin_recent_changes", { max_rows: 20 });
  return rows.map((r) => ({
    createdAt: new Date(r.created_at),
    actorEmail: r.actor_email,
    action: r.action,
    target: r.target,
    detail: r.detail ?? {},
  }));
}

export const setTrial = (org: string, endsAt: Date) => rpc<void>("admin_set_trial", { org, ends_at: endsAt.toISOString() });
export const setPlan = (org: string, plan: PlanId, billing: BillingId) =>
  rpc<void>("admin_set_plan", { org, new_plan: plan, new_billing: billing });
export const signOutEverywhere = (userId: string) => rpc<number>("admin_sign_out_everywhere", { target: userId });

export type AccountAction = "reset_password" | "resend_email" | "disable" | "enable";
export const accountAction = (action: AccountAction, userId: string) => invoke("owner-admin", { action, user_id: userId });

// ---- church Admins ----

interface MemberRow {
  user_id: string;
  email: string;
  name: string | null;
  role: Role;
  joined_at: string;
  last_sign_in_at: string | null;
  pending: boolean;
}

export async function loadTeam(org: string): Promise<TeamMember[]> {
  const rows = await rpc<MemberRow[]>("team_members", { org });
  return rows.map((r) => ({
    userId: r.user_id,
    email: r.email,
    name: r.name,
    role: r.role,
    joinedAt: new Date(r.joined_at),
    lastSignInAt: date(r.last_sign_in_at),
    pending: r.pending,
  }));
}

export const setMemberRole = (org: string, member: string, role: Role) => rpc<void>("team_set_role", { org, member, new_role: role });
export const removeMember = (org: string, member: string) => rpc<void>("team_remove_member", { org, member });
export const inviteMember = (org: string, email: string, role: Role) => invoke("team-invite", { org_id: org, email, role });

// ---- two-factor ----

export interface MfaState {
  /** A verified authenticator app exists; ask for its code. */
  factorId: string | null;
}

export async function mfaState(): Promise<MfaState> {
  const { data, error } = await supabase.auth.mfa.listFactors();
  if (error) throw new Error(authMessage(error));
  const verified = data.totp.find((f) => f.status === "verified");
  return { factorId: verified?.id ?? null };
}

export interface Enrollment {
  factorId: string;
  /** SVG data URL of the QR code. */
  qr: string;
  secret: string;
}

/** Start adding an authenticator app, clearing any half-finished attempt first. */
export async function startEnrollment(): Promise<Enrollment> {
  const { data: list } = await supabase.auth.mfa.listFactors();
  for (const f of list?.all ?? []) {
    if (f.factor_type === "totp" && f.status !== "verified") await supabase.auth.mfa.unenroll({ factorId: f.id });
  }
  const { data, error } = await supabase.auth.mfa.enroll({ factorType: "totp", friendlyName: "SanctuaryMix" });
  if (error) throw new Error(authMessage(error));
  return { factorId: data.id, qr: data.totp.qr_code, secret: data.totp.secret };
}

/** Check a 6-digit code; on success this session becomes two-factor (aal2). */
export async function verifyCode(factorId: string, code: string): Promise<void> {
  const { error } = await supabase.auth.mfa.challengeAndVerify({ factorId, code });
  if (error) {
    if (/invalid|expired/i.test(error.message)) throw new Error("That code didn't match. Check the app and try the newest code.");
    throw new Error(authMessage(error));
  }
}
