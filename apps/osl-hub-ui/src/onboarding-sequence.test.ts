import { describe, expect, it } from "vitest";
import {
  continueFromProOnboarding,
  nextOnboardingRoute,
  ONBOARDING_SEQUENCE,
  previousOnboardingRoute,
  proOnboardingStepContract,
} from "./onboarding-sequence";

// TASK 6802: the spine no longer branches. The argument is kept because the
// navigation helpers still take it, and an empty record is the only value it
// can now hold.
const ALL_BRANCHES = {};

describe("T15-C1 onboarding sequence", () => {
  // Protects the exact first-run order after owner rulings D4/D5. Each deleted
  // page is asserted absent by its own name so putting one back fails on that
  // name rather than as a length mismatch.
  it("pins every setup step in its intended order", () => {
    expect(ONBOARDING_SEQUENCE).toEqual([
      "welcome",
      "recovery",
      "recovery-check",
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
      "setup-apps",
    ]);
    for (const deleted of ["tutorial", "detected", "install", "apps", "silent-visible"]) {
      expect(ONBOARDING_SEQUENCE).not.toContain(deleted);
    }
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

  // Protects: browser consent runs straight into the one app page, with
  // nothing wedged between them in either direction.
  it("runs browser consent straight into the one app page", () => {
    expect(nextOnboardingRoute("browser", ALL_BRANCHES)).toBe("setup-apps");
    expect(previousOnboardingRoute("setup-apps", ALL_BRANCHES)).toBe("browser");
    expect(nextOnboardingRoute("setup-apps", ALL_BRANCHES)).toBeNull();
  });

  // TASK 6802: a deleted page has no place in the spine's navigation in either
  // direction, and nothing in the spine leads to one.
  it("gives every deleted page no place in the spine's navigation", () => {
    for (const deleted of ["tutorial", "detected", "install", "apps", "silent-visible"]) {
      expect(nextOnboardingRoute(deleted, ALL_BRANCHES)).toBeNull();
      expect(previousOnboardingRoute(deleted, ALL_BRANCHES)).toBeNull();
      for (const route of ONBOARDING_SEQUENCE) {
        expect(nextOnboardingRoute(route, ALL_BRANCHES)).not.toBe(deleted);
        expect(previousOnboardingRoute(route, ALL_BRANCHES)).not.toBe(deleted);
      }
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
