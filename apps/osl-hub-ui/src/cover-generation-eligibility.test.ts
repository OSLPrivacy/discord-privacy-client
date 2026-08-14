import { describe, expect, it } from "vitest";

import { coverGenerationPresentationEligibility, type CoverGenerationEligibilityInput } from "./cover-generation-eligibility";

function reportedState(overrides: Partial<CoverGenerationEligibilityInput> = {}): CoverGenerationEligibilityInput {
  return {
    carrier: "local-ai",
    entitlement: {
      access: "free",
      status: "UNCONFIGURED",
      currentPeriodEnd: null,
      lastValidatedAt: null,
    },
    ...overrides,
  };
}

describe("TU-84 on-device cover-generation Pro gate", () => {
  it("never renders the generation bar or popup for Free", () => {
    expect(coverGenerationPresentationEligibility(reportedState())).toMatchObject({
      renderProgress: false,
      renderPopup: false,
      reason: "pro-required",
    });
  });

  it("permits local generation for reported Pro entitlement", () => {
    expect(coverGenerationPresentationEligibility(reportedState({
      carrier: "local-ai",
      entitlement: {
        access: "offlineGrace",
        status: "GRACE",
        currentPeriodEnd: 1_800_000_000,
        lastValidatedAt: 1_700_000_000,
      },
    }))).toMatchObject({ renderProgress: true, renderPopup: true, reason: "eligible" });
  });

});
