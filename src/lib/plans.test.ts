import { describe, expect, it } from "vitest";
import { accessFor, entitlementsFor, LAPSED, lockReason } from "./plans";
import type { Session } from "./types";

const session = (access: Session["access"]): Session => ({
  user: { id: "u", displayName: "U", email: null },
  activeOrg: null,
  role: "admin",
  authenticated: true,
  access,
});

describe("plans", () => {
  it("match the core's table", () => {
    expect(entitlementsFor("essentials")).toMatchObject({ maxAiChannels: 32, teamLogins: 3, cloudSync: false });
    expect(entitlementsFor("pro")).toMatchObject({ maxAiChannels: 128, teamLogins: null, mixReports: true });
    expect(entitlementsFor("campus", 2).rooms).toBe(5);
  });

  it("lapse to manual mixing and playback", () => {
    expect(accessFor("pro", "paused").entitlements).toEqual(LAPSED);
    expect(accessFor("pro", "pastDue").entitlements.autoMix).toBe(true);
  });

  it("say why a feature is locked", () => {
    expect(lockReason(session(accessFor("essentials", "active")), "cloudSync")).toBe("Available on Pro");
    expect(lockReason(session(accessFor("essentials", "active")), "autoMix")).toBeNull();
    expect(lockReason(session(accessFor("pro", "canceled")), "autoMix")).toBe("Plan paused");
    expect(lockReason(null, "autoMix")).toBe("Sign in to use");
  });
});
