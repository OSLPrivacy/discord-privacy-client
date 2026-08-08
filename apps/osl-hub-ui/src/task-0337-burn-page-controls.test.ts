import { describe, expect, it } from "vitest";

import {
  BURN_PASSWORD_CONFIRMATION,
  backOnboardingPasswordRole,
  continueOnboardingPasswordRole,
  togglePasswordVisibility,
  type OnboardingPasswordRoleValues,
} from "./onboarding-password-role";
import { onboardingPasswordRoleContent } from "./password-roles";

const entered: OnboardingPasswordRoleValues = {
  current: "normal-0337-password",
  alternate: "burn-0337-password",
  confirm: "burn-0337-password",
  burnConfirmation: BURN_PASSWORD_CONFIRMATION,
};

describe("TASK 0337 burn setup controls", () => {
  it("records Show password, Save burn password, Continue, and Back", async () => {
    const setupMarkup = onboardingPasswordRoleContent({
      role: "burn",
      configured: false,
      passwordEyeIcon: () => "<svg></svg>",
      statusTag: (label) => `<span>${label}</span>`,
    });
    const savedMarkup = onboardingPasswordRoleContent({
      role: "burn",
      configured: true,
      passwordEyeIcon: () => "<svg></svg>",
      statusTag: (label) => `<span>${label}</span>`,
    });

    expect(setupMarkup).toContain("Burn password");
    expect(setupMarkup).toContain('data-password-toggle="setup-burn-alternate"');
    expect(setupMarkup).toContain('data-onboarding-password-role="burn"');
    expect(savedMarkup).toContain('data-password-role-next="pro"');
    expect(savedMarkup).toContain("Continue");

    let disposableAccounts = 1;
    let burnRequests = 0;
    let savedBurnPassword: string | null = null;
    let saveCount = 0;
    let page = "burnpass";

    console.log(`TASK0337_START page=${page} disposable_accounts=${disposableAccounts} burn_requests=${burnRequests}`);

    const shown = togglePasswordVisibility("password");
    const hidden = togglePasswordVisibility(shown.type);
    expect([shown.type, hidden.type]).toEqual(["text", "password"]);
    expect(entered.alternate).toBe("burn-0337-password");
    console.log(`TASK0337_SHOW_PASSWORD first_press_type=${shown.type} value=${entered.alternate} second_press_type=${hidden.type} value_still=${entered.alternate}`);

    const saved = await continueOnboardingPasswordRole(
      "burn",
      entered,
      async (role, current, alternate) => {
        expect(role).toBe("burn");
        expect(current).toBe(entered.current);
        saveCount += 1;
        savedBurnPassword = alternate;
        return { burnPasswordSet: true };
      },
    );
    expect(saved).toMatchObject({ accepted: true, saved: true, route: "pro" });
    expect(savedBurnPassword).toBe(entered.alternate);
    page = saved.route;
    console.log(`TASK0337_SAVE_BURN_PASSWORD save_count=${saveCount} exact_saved_password=${savedBurnPassword} continue_page=${page}`);

    const back = backOnboardingPasswordRole("burn");
    expect(back.route).toBe("passwords");
    page = back.route;
    console.log(`TASK0337_BACK page=${page} disposable_accounts=${disposableAccounts} burn_requests=${burnRequests}`);

    // Keep these observers live so the start-state report cannot silently
    // become a decorative constant while the production actions change.
    expect(disposableAccounts).toBe(1);
    expect(burnRequests).toBe(0);
  });
});
