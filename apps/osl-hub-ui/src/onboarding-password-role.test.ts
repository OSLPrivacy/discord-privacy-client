import { describe, expect, it } from "vitest";

import { BURN_PASSWORD_CONFIRMATION, canSetOnboardingPasswordRole } from "./onboarding-password-role";

describe("onboarding burn password confirmation", () => {
  it("keeps Set disabled until the burn confirmation is typed exactly", () => {
    const valid = {
      current: "main-123",
      alternate: "erase-456",
      confirm: "erase-456",
    };

    expect(canSetOnboardingPasswordRole("burn", { ...valid, burnConfirmation: "" })).toBe(false);
    expect(canSetOnboardingPasswordRole("burn", { ...valid, burnConfirmation: BURN_PASSWORD_CONFIRMATION.toLowerCase() })).toBe(false);
    expect(canSetOnboardingPasswordRole("burn", { ...valid, burnConfirmation: BURN_PASSWORD_CONFIRMATION })).toBe(true);
    expect(canSetOnboardingPasswordRole("stealth", { ...valid, burnConfirmation: "" })).toBe(true);
  });
});
