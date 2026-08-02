import { describe, expect, it } from "vitest";

import { coverGenerationPresentationEligibility, type CoverGenerationEligibilityInput } from "./cover-generation-eligibility";

function reportedState(overrides: Partial<CoverGenerationEligibilityInput> = {}): CoverGenerationEligibilityInput {
  return {
    carrier: "cloud",
    entitlement: {
      access: "free",
      status: "UNCONFIGURED",
      currentPeriodEnd: null,
      lastValidatedAt: null,
    },
    cloudConsent: "unavailable",
    ...overrides,
  };
}

describe("TU-84 cover-generation Pro and cloud-consent gate", () => {
  it("never renders the generation bar or popup for Free", () => {
    expect(coverGenerationPresentationEligibility(reportedState())).toMatchObject({
      renderProgress: false,
      renderPopup: false,
      reason: "pro-required",
    });
  });

  it("requires a separately reported cloud-consent grant, even for Pro", () => {
    const proWithoutCloudConsent = reportedState({
      entitlement: {
        access: "pro",
        status: "ACTIVE",
        currentPeriodEnd: 1_800_000_000,
        lastValidatedAt: 1_700_000_000,
      },
      cloudConsent: "declined",
    });

    expect(coverGenerationPresentationEligibility(proWithoutCloudConsent)).toMatchObject({
      renderProgress: false,
      renderPopup: false,
      reason: "cloud-consent-required",
    });
  });

  it("permits local generation for reported Pro entitlement without cloud consent", () => {
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

  it("permits cloud generation only after the reported consent grant", () => {
    expect(coverGenerationPresentationEligibility(reportedState({
      entitlement: {
        access: "pro",
        status: "ACTIVE",
        currentPeriodEnd: 1_800_000_000,
        lastValidatedAt: 1_700_000_000,
      },
      cloudConsent: "granted",
    }))).toMatchObject({ renderProgress: true, renderPopup: true, reason: "eligible" });
  });
});
