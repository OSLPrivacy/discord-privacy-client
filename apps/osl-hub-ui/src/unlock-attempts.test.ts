import { describe, expect, it } from "vitest";
import { unlockAttemptWarning } from "./unlock-attempts";

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
