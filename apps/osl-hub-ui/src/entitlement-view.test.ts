import { describe, expect, it } from "vitest";
import { entitlementView, type EntitlementView } from "./entitlement-view";
import type { HubLicenseState } from "./core";

const now = 1_735_689_600;

function state(status: HubLicenseState["status"], access: HubLicenseState["access"], currentPeriodEnd: number | null = null): HubLicenseState {
  return { status, access, currentPeriodEnd, lastValidatedAt: now };
}

describe("entitlement view-model", () => {
  it("maps every backend status to one complete renderable view", () => {
    const views: readonly [HubLicenseState, EntitlementView][] = [
      [state("UNCONFIGURED", "free"), { tier: "free", daysLeft: null, banner: "free", cta: "activate" }],
      [state("UNREDEEMED", "free"), { tier: "free", daysLeft: null, banner: "activationRequired", cta: "activate" }],
      [state("PENDING", "free"), { tier: "free", daysLeft: null, banner: "activationPending", cta: "wait" }],
      [state("ACTIVE", "pro", now + 86_401), { tier: "pro", daysLeft: 2, banner: "active", cta: "none" }],
      [state("CANCELLED", "pro", now + 86_400), { tier: "pro", daysLeft: 1, banner: "active", cta: "none" }],
      [state("GRACE", "offlineGrace", now + 86_400), { tier: "offlineGrace", daysLeft: 1, banner: "offlineGrace", cta: "none" }],
      [state("EXPIRED", "free", now - 1), { tier: "free", daysLeft: 0, banner: "lapsed", cta: "activate" }],
      [state("REVOKED", "free"), { tier: "free", daysLeft: null, banner: "accessUnavailable", cta: "activate" }],
      [state("UNKNOWN", "free"), { tier: "free", daysLeft: null, banner: "accessUnknown", cta: "retry" }],
    ];

    for (const [licenseState, expected] of views) {
      expect(entitlementView(licenseState, now)).toEqual(expected);
    }
  });

  it("uses the supplied clock and never reports negative remaining days", () => {
    expect(entitlementView(state("ACTIVE", "pro", now), now).daysLeft).toBe(0);
    expect(entitlementView(state("ACTIVE", "pro", now - 86_400), now).daysLeft).toBe(0);
  });
});
