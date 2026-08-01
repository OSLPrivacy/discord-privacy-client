import { describe, expect, it } from "vitest";
import { continueFromProOnboarding, proOnboardingStepContract } from "./onboarding-sequence";

describe("Pro onboarding seam", () => {
  it("continues to privacy with a usable free account when activation is skipped or fails", () => {
    expect(proOnboardingStepContract).toMatchObject({ skippable: true, worksOffline: true });
    expect(continueFromProOnboarding("skipped")).toEqual({ route: "privacy", access: "free" });
    expect(continueFromProOnboarding("failed")).toEqual({ route: "privacy", access: "free" });
  });

  it("continues to privacy after a successful activation without defining redemption semantics", () => {
    expect(continueFromProOnboarding("activated")).toEqual({ route: "privacy", access: "pro" });
  });
});
