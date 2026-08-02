import { describe, expect, it } from "vitest";
import { entitlementCopy, type EntitlementCopy } from "./entitlement-copy";
import type { EntitlementView } from "./entitlement-view";

function view(overrides: Partial<EntitlementView>): EntitlementView {
  return {
    tier: "free",
    daysLeft: null,
    banner: "free",
    cta: "activate",
    ...overrides,
  };
}

describe("entitlement expiry copy", () => {
  it("explains an approaching Pro expiry without a renewal obligation", () => {
    expect(entitlementCopy(view({ tier: "pro", daysLeft: 2, banner: "active", cta: "none" }))).toEqual({
      title: "Pro access ends in 2 days",
      detail: "When Pro access ends, Free OSL keeps working.",
    });
  });

  it("names the Free features that keep working after Pro access lapses", () => {
    expect(entitlementCopy(view({ daysLeft: 0, banner: "lapsed" }))).toEqual({
      title: "Pro access has ended",
      detail: "Free OSL keeps working: encrypted messages, sending, receiving, and the word-bank carrier.",
    });
  });

  it("keeps every rendered entitlement string free of subscription language and data-loss claims", () => {
    const copies: readonly EntitlementCopy[] = [
      entitlementCopy(view({ tier: "pro", daysLeft: 1, banner: "active", cta: "none" })),
      entitlementCopy(view({ tier: "pro", daysLeft: 0, banner: "active", cta: "none" })),
      entitlementCopy(view({ tier: "free", daysLeft: 0, banner: "lapsed" })),
      entitlementCopy(view({ tier: "offlineGrace", daysLeft: 3, banner: "offlineGrace", cta: "none" })),
      entitlementCopy(view({ banner: "free" })),
      entitlementCopy(view({ banner: "activationRequired" })),
      entitlementCopy(view({ banner: "activationPending", cta: "wait" })),
      entitlementCopy(view({ banner: "accessUnavailable" })),
      entitlementCopy(view({ banner: "accessUnknown", cta: "retry" })),
    ];

    for (const copy of copies) {
      expect(`${copy.title} ${copy.detail}`).not.toMatch(/\b(?:cancel|renew|subscription|billing|destroy(?:ed|s|ing)?|delete(?:d|s|ing)?|lose|loss)\b/iu);
    }
  });
});
