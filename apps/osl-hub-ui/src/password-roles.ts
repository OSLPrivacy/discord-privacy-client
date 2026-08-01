export type OnboardingPasswordRole = "stealth" | "burn";

type PasswordRoleContentOptions = {
  role: OnboardingPasswordRole;
  configured: boolean | undefined;
  passwordEyeIcon: () => string;
  statusTag: (label: string) => string;
};

import { BURN_PASSWORD_CONFIRMATION } from "./onboarding-password-role";

export function onboardingPasswordRoleContent({ role, configured, passwordEyeIcon, statusTag }: PasswordRoleContentOptions): string {
  const stealth = role === "stealth";
  const title = stealth ? "Stealth password" : "Burn password";
  const detail = stealth
    ? "Opens an empty workspace without loading your private data. It does not hide that OSL is installed. OSL only marks its folder hidden, and anyone viewing hidden files can still find it."
    : "Erases OSL data from this device when entered at sign in.";
  const next = stealth ? "burnpass" : "mullvad";
  // t15-b4 gates the burn password behind a typed confirmation, and
  // canSetOnboardingPasswordRole() now REQUIRES it. Without this input the
  // burn password could never be set at all -- the validator would refuse a
  // value the form gives the user no way to enter.
  const burnConfirmation = stealth
    ? ""
    : `<p class="password-role-warning">This password permanently erases OSL data from this device when used at sign in. There is no recovery.</p><label for="setup-burn-confirmation">Type ${BURN_PASSWORD_CONFIRMATION} to enable it</label><input id="setup-burn-confirmation" name="burnConfirmation" type="text" autocomplete="off" autocapitalize="none" spellcheck="false" required/>`;
  if (configured) {
    return `<h1 id="route-heading" tabindex="-1">${title}</h1><div class="password-role-ready">${statusTag("Set")}<p>${detail}</p></div><div class="setup-footer onboarding-actions"><button class="button primary" data-password-role-next="${next}" type="button">Continue</button></div>`;
  }
  return `<h1 id="route-heading" tabindex="-1">${title}</h1><p class="compact-lead onboarding-centered-copy">${detail}</p><form class="setup-surface password-form onboarding-role-form" data-onboarding-password-role="${role}" data-onboarding-password-next="${next}" novalidate><label for="setup-${role}-current">Current password</label><div class="password-input-row"><input id="setup-${role}-current" name="current" type="password" minlength="6" maxlength="128" autocomplete="current-password" required/><button class="password-eye" type="button" data-password-toggle="setup-${role}-current" aria-label="Show current password">${passwordEyeIcon()}</button></div><label for="setup-${role}-alternate">New ${stealth ? "stealth" : "burn"} password</label><div class="password-input-row"><input id="setup-${role}-alternate" name="alternate" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="setup-${role}-alternate" aria-label="Show new password">${passwordEyeIcon()}</button></div><label for="setup-${role}-confirm">Confirm</label><div class="password-input-row"><input id="setup-${role}-confirm" name="confirm" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="setup-${role}-confirm" aria-label="Show password confirmation">${passwordEyeIcon()}</button></div>${burnConfirmation}<p class="unlock-error" data-onboarding-role-error role="alert"></p><button class="button primary" type="submit" disabled>Set password</button></form><button class="text-button onboarding-role-skip" type="button" data-skip-onboarding-password-role="${next}">Not now</button>`;
}
