import { describe, expect, it } from "vitest";
import {
  continueFromProOnboarding,
  nextOnboardingRoute,
  ONBOARDING_SEQUENCE,
  previousOnboardingRoute,
  proOnboardingStepContract,
} from "./onboarding-sequence";

const ALL_BRANCHES = { detected: true, install: true };

describe("T15-C1 onboarding sequence", () => {
  it("pins every setup step in its intended order", () => {
    expect(ONBOARDING_SEQUENCE).toEqual([
      "welcome",
      "recovery",
      "pro",
      "privacy",
      "defaults",
      "sending",
      "cover",
      "passwords",
      "burnpass",
      "mullvad",
      "browser",
      "tutorial",
      "detected",
      "install",
      "apps",
    ]);
  });

  it("round-trips every non-initial route through Back and Next", () => {
    for (const route of ONBOARDING_SEQUENCE.slice(1)) {
      const previous = previousOnboardingRoute(route, ALL_BRANCHES);
      expect(previous, `${route} must have a previous route`).not.toBeNull();
      expect(nextOnboardingRoute(previous!, ALL_BRANCHES)).toBe(route);
    }
  });

  it("makes every setup step reachable from welcome", () => {
    const reached = ["welcome"];
    let current = "welcome";
    while (true) {
      const next = nextOnboardingRoute(current, ALL_BRANCHES);
      if (!next) break;
      reached.push(next);
      current = next;
    }

    expect(reached).toEqual(ONBOARDING_SEQUENCE);
  });

  it("skips optional app branches symmetrically", () => {
    const noOptionalAppSteps = { detected: false, install: false };

    expect(nextOnboardingRoute("tutorial", noOptionalAppSteps)).toBe("apps");
    expect(previousOnboardingRoute("apps", noOptionalAppSteps)).toBe("tutorial");
  });
});

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
