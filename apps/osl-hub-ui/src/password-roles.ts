export type OnboardingPasswordRole = "stealth" | "burn";

type PasswordRoleContentOptions = {
  role: OnboardingPasswordRole;
  configured: boolean | undefined;
  passwordEyeIcon: () => string;
  statusTag: (label: string) => string;
};

export function onboardingPasswordRoleContent({ role, configured, passwordEyeIcon, statusTag }: PasswordRoleContentOptions): string {
  const stealth = role === "stealth";
  const title = stealth ? "Stealth password" : "Burn password";
  const detail = stealth
    ? "Opens an empty workspace without loading your private data. It does not hide that OSL is installed. OSL only marks its folder hidden, and anyone viewing hidden files can still find it."
    : "Erases OSL data from this device when entered at sign in.";
  const next = stealth ? "burnpass" : "mullvad";
  if (configured) {
    return `<h1 id="route-heading" tabindex="-1">${title}</h1><div class="password-role-ready">${statusTag("Set")}<p>${detail}</p></div><div class="setup-footer onboarding-actions"><button class="button primary" data-password-role-next="${next}" type="button">Continue</button></div>`;
  }
  return `<h1 id="route-heading" tabindex="-1">${title}</h1><p class="compact-lead onboarding-centered-copy">${detail}</p><form class="setup-surface password-form onboarding-role-form" data-onboarding-password-role="${role}" data-onboarding-password-next="${next}" novalidate><label for="setup-${role}-current">Current password</label><div class="password-input-row"><input id="setup-${role}-current" name="current" type="password" minlength="6" maxlength="128" autocomplete="current-password" required/><button class="password-eye" type="button" data-password-toggle="setup-${role}-current" aria-label="Show current password">${passwordEyeIcon()}</button></div><label for="setup-${role}-alternate">New ${stealth ? "stealth" : "burn"} password</label><div class="password-input-row"><input id="setup-${role}-alternate" name="alternate" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="setup-${role}-alternate" aria-label="Show new password">${passwordEyeIcon()}</button></div><label for="setup-${role}-confirm">Confirm</label><div class="password-input-row"><input id="setup-${role}-confirm" name="confirm" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="setup-${role}-confirm" aria-label="Show password confirmation">${passwordEyeIcon()}</button></div><p class="unlock-error" data-onboarding-role-error role="alert"></p><button class="button primary" type="submit" disabled>Set password</button></form><button class="text-button onboarding-role-skip" type="button" data-skip-onboarding-password-role="${next}">Not now</button>`;
}
