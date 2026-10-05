// What each plan unlocks, for the UI. The core (crates/auth/src/plan.rs) owns
// the table and checks it before starting anything a plan gates; the app gets
// the church's limits on the session. This copy only feeds the browser demo,
// so keep the two in step (plans.test.ts checks the numbers).

import type { Access, Entitlements, Feature, Plan, Session, SubscriptionStatus } from "./types";

export const PLAN_NAME: Record<Plan, string> = {
  essentials: "Essentials",
  pro: "Pro",
  campus: "Campus",
};

export const STATUS_LABEL: Record<SubscriptionStatus, string> = {
  trialing: "Trial",
  active: "Active",
  pastDue: "Payment failed",
  paused: "Paused",
  canceled: "Canceled",
  unpaid: "Unpaid",
  incompleteExpired: "Not started",
};

const ALL_INPUTS = 128;
const CAMPUS_ROOMS = 3;

export function entitlementsFor(plan: Plan, extraRooms = 0): Entitlements {
  switch (plan) {
    case "essentials":
      return {
        maxAiChannels: 32,
        autoMix: true,
        recordServices: true,
        cloudSync: false,
        mixReports: false,
        aiEq: false,
        teamLogins: 3,
        rooms: 1,
      };
    case "pro":
      return {
        maxAiChannels: ALL_INPUTS,
        autoMix: true,
        recordServices: true,
        cloudSync: true,
        mixReports: true,
        aiEq: true,
        teamLogins: null,
        rooms: 1,
      };
    case "campus":
      return { ...entitlementsFor("pro"), rooms: CAMPUS_ROOMS + extraRooms };
  }
}

/** No plan, or it lapsed: manual mixing and playback only. */
export const LAPSED: Entitlements = {
  maxAiChannels: 0,
  autoMix: false,
  recordServices: false,
  cloudSync: false,
  mixReports: false,
  aiEq: false,
  teamLogins: null,
  rooms: 1,
};

export const NO_ACCESS: Access = { plan: null, status: null, entitlements: LAPSED };

const grantsPlan = (status: SubscriptionStatus) =>
  status === "trialing" || status === "active" || status === "pastDue";

export function accessFor(plan: Plan, status: SubscriptionStatus, extraRooms = 0): Access {
  return { plan, status, entitlements: grantsPlan(status) ? entitlementsFor(plan, extraRooms) : LAPSED };
}

/** The features shown in Settings, cheapest plan first. */
export const FEATURES: { feature: Feature; label: string; minPlan: Plan }[] = [
  { feature: "autoMix", label: "Auto-mix", minPlan: "essentials" },
  { feature: "recordServices", label: "Record services", minPlan: "essentials" },
  { feature: "cloudSync", label: "Cloud sync", minPlan: "pro" },
  { feature: "mixReports", label: "Mix reports", minPlan: "pro" },
  { feature: "aiEq", label: "AI EQ", minPlan: "pro" },
];

export function allows(session: Session | null, feature: Feature): boolean {
  return session?.access.entitlements[feature] ?? false;
}

/** A few words on why a feature is locked, or null if it isn't. */
export function lockReason(session: Session | null, feature: Feature): string | null {
  if (allows(session, feature)) return null;
  const access = session?.access ?? NO_ACCESS;
  if (!access.plan) return "Sign in to use";
  if (access.status && !grantsPlan(access.status)) return "Plan paused";
  const min = FEATURES.find((f) => f.feature === feature)!.minPlan;
  return `Available on ${PLAN_NAME[min]}`;
}
