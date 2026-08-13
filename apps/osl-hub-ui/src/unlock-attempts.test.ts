import { describe, expect, it } from "vitest";
import { COOLDOWN_LIMIT_DISCLOSURE, unlockAttemptWarning } from "./unlock-attempts";

describe("unlockAttemptWarning", () => {
  it("escalates the warning for attempts seven through nine", () => {
    expect(unlockAttemptWarning(7)).toBe("3 attempts left before this device is erased.");
    expect(unlockAttemptWarning(8)).toBe("2 attempts left before this device is erased.");
    expect(unlockAttemptWarning(9)).toBe("1 attempt left before this device is erased.");
  });

  it("does not treat the destructive tenth attempt as a warning", () => {
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
