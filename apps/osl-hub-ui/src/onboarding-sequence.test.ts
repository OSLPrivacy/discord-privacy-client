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
  // Protects the exact first-run order. "tutorial" left this list on
  // 2026-08-06 by the owner's instruction -- the route and its five steps are
  // still built and still replayable from Settings -> About, but nobody is
  // walked through them before they have used the app once. The absence is
  // asserted separately so re-adding it fails on its own name, not just as a
  // length mismatch.
  it("pins every setup step in its intended order", () => {
    expect(ONBOARDING_SEQUENCE).toEqual([
      "welcome",
      "recovery",
      "identity-choice",
      "pro",
      "forward-secrecy",
      "privacy",
      "tor",
      "defaults",
      "sending",
      "cover",
      "visibility",
      "passwords",
      "burnpass",
      "mullvad",
      "browser",
      "detected",
      "install",
      "apps",
    ]);
    expect(ONBOARDING_SEQUENCE).not.toContain("tutorial");
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

  // Protects: turning the optional app branches off skips the SAME routes in
  // both directions, so Back always retraces the exact path Next took. The
  // anchor used to be "tutorial"; with the tour off the spine the step either
  // side of the optional branches is `browser` -> `apps`.
  it("skips optional app branches symmetrically", () => {
    const noOptionalAppSteps = { detected: false, install: false };

    expect(nextOnboardingRoute("browser", noOptionalAppSteps)).toBe("apps");
    expect(previousOnboardingRoute("apps", noOptionalAppSteps)).toBe("browser");
  });

  // Protects the 2026-08-06 removal: the tour is not a setup step, so the spine
  // cannot walk into it or out of it in either direction. If "tutorial" is put
  // back into ONBOARDING_SEQUENCE these stop being null and this fails.
  it("gives the replay-only tour no place in the spine's navigation", () => {
    expect(nextOnboardingRoute("tutorial", ALL_BRANCHES)).toBeNull();
    expect(previousOnboardingRoute("tutorial", ALL_BRANCHES)).toBeNull();
    // ...and nothing in the spine leads to it.
    for (const route of ONBOARDING_SEQUENCE) {
      expect(nextOnboardingRoute(route, ALL_BRANCHES)).not.toBe("tutorial");
      expect(previousOnboardingRoute(route, ALL_BRANCHES)).not.toBe("tutorial");
    }
  });
});

describe("Pro onboarding seam", () => {
  it("continues to the message-protection choice with a usable free account when activation is skipped or fails", () => {
    expect(proOnboardingStepContract).toMatchObject({ skippable: true, worksOffline: true });
    expect(continueFromProOnboarding("skipped")).toEqual({ route: "forward-secrecy", access: "free" });
    expect(continueFromProOnboarding("failed")).toEqual({ route: "forward-secrecy", access: "free" });
  });

  it("continues to the message-protection choice after a successful activation without defining redemption semantics", () => {
    expect(continueFromProOnboarding("activated")).toEqual({ route: "forward-secrecy", access: "pro" });
  });
});
