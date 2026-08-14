import { describe, expect, it } from "vitest";
import { COOLDOWN_LIMIT_DISCLOSURE, unlockAttemptWarning } from "./unlock-attempts";

describe("unlockAttemptWarning", () => {
  it("shows the exact remaining count for ordinary failures one through nine", () => {
    for (let used = 1; used <= 9; used += 1) {
      const remaining = 10 - used;
      const noun = remaining === 1 ? "attempt" : "attempts";
      expect(unlockAttemptWarning(used)).toBe(`${remaining} ${noun} remaining before a 15-minute cooldown.`);
    }
  });

  it("leaves the tenth attempt to the cooldown response", () => {
    expect(unlockAttemptWarning(10)).toBeNull();
  });
});

describe("COOLDOWN_LIMIT_DISCLOSURE", () => {
  it("contains the exact disclosure message about the 15-minute cooldown bypass", () => {
    expect(COOLDOWN_LIMIT_DISCLOSURE).toBe(
      "The 15-minute unlock cooldown can be bypassed by restoring or modifying OSL data files; it is not protection against someone with access to those files."
    );
  });
});
