import { describe, expect, it } from "vitest";
import {
  firstRunOnboardingStepContract,
  firstRunOnboardingStepOrder,
  parseFirstRunOnboardingStep,
  type FirstRunOnboardingStep,
} from "./state";

describe("first-run onboarding six-step contract", () => {
  it("defines the approved six steps in order", () => {
    expect(firstRunOnboardingStepOrder).toEqual<FirstRunOnboardingStep[]>([
      "welcome",
      "choose-protection",
      "choose-apps",
      "choose-send",
      "review-defaults",
      "secure-recovery",
    ]);
  });

  it("keeps one contract entry for every approved step", () => {
    expect(firstRunOnboardingStepContract.map((definition) => definition.step)).toEqual(firstRunOnboardingStepOrder);
    expect(new Set(firstRunOnboardingStepContract.map((definition) => definition.step)).size).toBe(6);
  });

  it("captures the binding safety promises for first run", () => {
    const byStep = new Map(firstRunOnboardingStepContract.map((definition) => [definition.step, definition]));

    expect(byStep.get("choose-protection")?.requiredGuarantees).toContain("balanced-recommended");
    expect(byStep.get("choose-apps")?.requiredGuarantees).toContain("native-client-sign-in-only");
    expect(byStep.get("choose-send")?.requiredGuarantees).toEqual(expect.arrayContaining([
      "manual-recommended",
      "ordinary-modes-manual-clipboard-double-enter",
      "no-silent-send",
      "no-auto-retry",
    ]));
    expect(byStep.get("review-defaults")?.requiredGuarantees).toContain("destructive-automation-off");
    expect(byStep.get("secure-recovery")?.requiredGuarantees).toContain("establish-recovery-first");
  });

  it("refuses unknown persisted steps by returning the safe first step", () => {
    expect(parseFirstRunOnboardingStep("choose-send")).toBe("choose-send");
    expect(parseFirstRunOnboardingStep("single-enter")).toBe("welcome");
    expect(parseFirstRunOnboardingStep(null)).toBe("welcome");
  });
});
