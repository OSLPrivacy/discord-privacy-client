import { describe, expect, it } from "vitest";
import { unlockAttemptWarning } from "./unlock-attempts";

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
