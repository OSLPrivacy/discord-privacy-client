import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

import {
  BURN_PASSWORD_CONFIRMATION,
  backOnboardingPasswordRole,
  continueOnboardingPasswordRole,
  skipOnboardingPasswordRole,
  togglePasswordVisibility,
  type OnboardingPasswordRoleValues,
} from "./onboarding-password-role";
import { onboardingPasswordRoleContent } from "./password-roles";

const matching: OnboardingPasswordRoleValues = {
  current: "normal-0309-password",
  alternate: "burn-0309-password",
  confirm: "burn-0309-password",
  burnConfirmation: BURN_PASSWORD_CONFIRMATION,
};

describe("TASK 0309 connected burn-password page", () => {
  it("connects password, Show, Confirm, Continue, Skip, and Back and gates Pro on save or Skip", async () => {
    const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const markup = onboardingPasswordRoleContent({
      role: "burn",
      configured: false,
      passwordEyeIcon: () => "<svg></svg>",
      statusTag: (label) => `<span>${label}</span>`,
    });
    const readyMarkup = onboardingPasswordRoleContent({
      role: "burn",
      configured: true,
      passwordEyeIcon: () => "<svg></svg>",
      statusTag: (label) => `<span>${label}</span>`,
    });

    expect(markup).toContain('name="alternate"');
    expect(markup).toContain('name="confirm"');
    expect(markup.match(/data-password-toggle=/gu)).toHaveLength(3);
    expect(markup).toContain('data-onboarding-role-submit');
    expect(markup).toContain("Continue");
    expect(markup).toContain('data-skip-onboarding-password-role="pro"');
    expect(readyMarkup).toContain('data-password-role-next="pro"');
    expect(readyMarkup).toContain("Continue");
    expect(mainSource).toContain("continueOnboardingPasswordRole(role, submitted, setHubAlternatePassword)");
    expect(mainSource).toMatch(/await continueOnboardingPasswordRole[\s\S]*?outcome\.route !== next[\s\S]*?onboardingRoute = next/u);
    expect(mainSource).toMatch(/skipOnboardingPasswordRole\(role\)[\s\S]*?outcome\.route !== next[\s\S]*?onboardingRoute = next/u);
    expect(mainSource).toContain('backOnboardingPasswordRole("burn").route');
    expect(mainSource).toContain("togglePasswordVisibility(input.type)");

    const enteredPassword = matching.alternate;
    const shown = togglePasswordVisibility("password");
    const hidden = togglePasswordVisibility(shown.type);
    expect([shown.type, hidden.type]).toEqual(["text", "password"]);
    expect([shown.label, hidden.label]).toEqual(["Hide password", "Show password"]);
    expect(enteredPassword).toBe(matching.alternate);

    let storedBurnPassword: string | null = null;
    let releaseSave: (() => void) | null = null;
    const saveStarted = new Promise<void>((resolve) => { releaseSave = resolve; });
    let writes = 0;
    const save = async (role: "stealth" | "burn", current: string, alternate: string) => {
      expect(role).toBe("burn");
      expect(current).toBe(matching.current);
      writes += 1;
      await saveStarted;
      storedBurnPassword = alternate;
      return { burnPasswordSet: true };
    };

    const mismatch = await continueOnboardingPasswordRole(
      "burn",
      { ...matching, confirm: "burn-0309-near-match" },
      save,
    );
    expect(mismatch).toMatchObject({ accepted: false, saved: false, route: "burnpass", reason: "invalid-passwords" });
    expect(writes).toBe(0);

    const unconfirmed = await continueOnboardingPasswordRole(
      "burn",
      { ...matching, burnConfirmation: "" },
      save,
    );
    expect(unconfirmed).toMatchObject({ accepted: false, saved: false, route: "burnpass", reason: "invalid-passwords" });
    expect(writes).toBe(0);

    const skipped = skipOnboardingPasswordRole("burn");
    expect(skipped).toMatchObject({ accepted: true, saved: false, route: "pro", status: null });
    expect(writes).toBe(0);
    expect(storedBurnPassword).toBeNull();

    const back = backOnboardingPasswordRole("burn");
    expect(back).toMatchObject({ accepted: true, saved: false, route: "passwords", status: null });

    let continueSettled = false;
    const continuedPromise = continueOnboardingPasswordRole("burn", matching, save)
      .then((outcome) => { continueSettled = true; return outcome; });
    await Promise.resolve();
    expect(writes).toBe(1);
    expect(continueSettled).toBe(false);
    expect(storedBurnPassword).toBeNull();
    releaseSave!();
    const continued = await continuedPromise;
    expect(continued).toMatchObject({ accepted: true, saved: true, route: "pro", status: { burnPasswordSet: true } });
    expect(storedBurnPassword).toBe(matching.alternate);

    console.log(
      `TASK0309 controls=password,show,confirm,Continue,Skip,Back show_types=${shown.type},${hidden.type} `
      + `mismatch_save_count=0 mismatch_route=${mismatch.route} unconfirmed_save_count=0 unconfirmed_route=${unconfirmed.route} `
      + `before_save_route=none continue_save_count=${writes} continue_route=${continued.route} `
      + `skip_save_count=0 skip_route=${skipped.route} back_route=${back.route}`,
    );
  });
});
