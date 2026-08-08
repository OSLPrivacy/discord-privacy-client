import { describe, expect, it } from "vitest";

import {
  backOnboardingPasswordRole,
  continueOnboardingPasswordRole,
  skipOnboardingPasswordRole,
  togglePasswordVisibility,
  type OnboardingPasswordRoleValues,
} from "./onboarding-password-role";
import { onboardingPasswordRoleContent } from "./password-roles";

const matching: OnboardingPasswordRoleValues = {
  current: "normal-0307-password",
  alternate: "stealth-0307-password",
  confirm: "stealth-0307-password",
  burnConfirmation: "",
};

describe("TASK 0307 connected stealth-password page", () => {
  it("connects password, Show, Confirm, Continue, Skip, and Back", async () => {
    const markup = onboardingPasswordRoleContent({
      role: "stealth",
      configured: false,
      passwordEyeIcon: () => "<svg></svg>",
      statusTag: (label) => `<span>${label}</span>`,
    });
    const readyMarkup = onboardingPasswordRoleContent({
      role: "stealth",
      configured: true,
      passwordEyeIcon: () => "<svg></svg>",
      statusTag: (label) => `<span>${label}</span>`,
    });

    expect(markup).toContain('name="alternate"');
    expect(markup).toContain('name="confirm"');
    expect(markup.match(/data-password-toggle=/gu)).toHaveLength(3);
    expect(markup).toContain('data-onboarding-role-submit');
    expect(markup).toContain('data-skip-onboarding-password-role="burnpass"');
    expect(readyMarkup).toContain('data-password-role-next="burnpass"');
    expect(readyMarkup).toContain("Continue");

    const enteredPassword = matching.alternate;
    const shown = togglePasswordVisibility("password");
    const hidden = togglePasswordVisibility(shown.type);
    expect([shown.type, hidden.type]).toEqual(["text", "password"]);
    expect([shown.label, hidden.label]).toEqual(["Hide password", "Show password"]);
    expect(enteredPassword).toBe(matching.alternate);

    let storedStealthPassword: string | null = null;
    const writes: string[] = [];
    const save = async (role: "stealth" | "burn", current: string, alternate: string) => {
      expect(role).toBe("stealth");
      expect(current).toBe(matching.current);
      storedStealthPassword = alternate;
      writes.push(alternate);
      return { stealthPasswordSet: true };
    };

    const mismatch = await continueOnboardingPasswordRole(
      "stealth",
      { ...matching, confirm: "stealth-0307-near-match" },
      save,
    );
    expect(mismatch).toMatchObject({ accepted: false, saved: false, route: "passwords", reason: "invalid-passwords" });
    expect(writes).toHaveLength(0);
    expect(storedStealthPassword).toBeNull();

    const skipped = skipOnboardingPasswordRole("stealth");
    expect(skipped).toMatchObject({ accepted: true, saved: false, route: "burnpass", status: null });
    expect(writes).toHaveLength(0);
    expect(storedStealthPassword).toBeNull();

    const back = backOnboardingPasswordRole("stealth");
    expect(back).toMatchObject({ accepted: true, saved: false, route: "visibility", status: null });

    const continued = await continueOnboardingPasswordRole("stealth", matching, save);
    expect(continued).toMatchObject({ accepted: true, saved: true, route: "burnpass", status: { stealthPasswordSet: true } });
    expect(writes).toEqual([matching.alternate]);
    expect(storedStealthPassword).toBe(matching.alternate);

    console.log(
      `TASK0307 controls=password,show,confirm,Continue,Skip,Back show_types=${shown.type},${hidden.type} `
      + `mismatch_save_count=${mismatch.saved ? 1 : 0} mismatch_route=${mismatch.route} `
      + `skip_save_count=${skipped.saved ? 1 : 0} skip_stealth_password=${skipped.status === null ? "none" : "set"} `
      + `continue_save_count=${writes.length} continue_route=${continued.route} back_route=${back.route}`,
    );
  });
});
