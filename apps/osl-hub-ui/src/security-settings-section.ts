export interface SecuritySettingsState {
  bootstrapStatus: string;
  passwordRoleStatus: {
    stealthPasswordSet: boolean;
    stealthActionWired: boolean;
    burnPasswordSet: boolean;
    burnActionWired: boolean;
  } | null;
}

export function passwordSecuritySettingsContent(
  state: SecuritySettingsState,
  passwordEyeIcon: () => string,
): string {
  const passwordAction = state.bootstrapStatus === "setupRequired"
    ? `<button class="button primary" data-onboarding-action="create">Create password</button>`
    : state.bootstrapStatus === "passwordRequired"
      ? `<button class="button primary" data-onboarding-action="unlock">Unlock OSL</button>`
      : state.bootstrapStatus === "identityKeyLost"
        ? `<span class="setting-status"><span class="dot"></span>This device can no longer open this account</span><button class="button primary" data-onboarding-action="import">Restore with recovery phrase</button>`
        : `<span class="setting-status"><span class="dot"></span>Password configured and unlocked</span><button class="button" type="button" data-lock-session="now">Lock now</button>`;
  const roleForm = (role: "stealth" | "burn", configured: boolean, wired: boolean): string => {
    const title = role === "stealth" ? "Stealth password" : "Burn password";
    const consequence = role === "stealth" ? "decoy screen" : "account burn";
    if (!wired) {
      return `<section class="password-role unavailable" aria-disabled="true"><div><strong>${title}</strong><small>${configured ? "Stored but inactive" : "Unavailable"}</small></div><p>The ${consequence} login action is not available in this build. OSL will not let you create or rely on it.</p></section>`;
    }
    return `<details class="password-role"><summary><span><strong>${title}</strong><small>${configured ? "Configured" : "Not set"}</small></span><span>›</span></summary><form data-password-role="${role}" data-password-remove="${configured}"><label>Current password<div class="password-input-row"><input id="${role}-current" name="current" type="password" minlength="6" maxlength="128" autocomplete="current-password" required/><button class="password-eye" type="button" data-password-toggle="${role}-current" aria-label="Show current password">${passwordEyeIcon()}</button></div></label>${configured ? "" : `<label>New ${role} password<div class="password-input-row"><input id="${role}-alternate" name="alternate" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="${role}-alternate" aria-label="Show new password">${passwordEyeIcon()}</button></div></label>`}<button class="button ${configured ? "danger" : "primary"}" type="submit">${configured ? "Remove" : "Set password"}</button><p class="password-role-note">Active at login for ${consequence}.</p></form></details>`;
  };
  const roles = state.passwordRoleStatus
    ? `<div class="security-shortcuts">${roleForm("stealth", state.passwordRoleStatus.stealthPasswordSet, state.passwordRoleStatus.stealthActionWired)}${roleForm("burn", state.passwordRoleStatus.burnPasswordSet, state.passwordRoleStatus.burnActionWired)}</div>`
    : `<div class="settings-unavailable"><strong>Password roles unavailable</strong><span>Unlock OSL and reopen Settings.</span></div>`;
  return `<section class="settings-section password-security"><header><div><h3>Password & security</h3><p>Protects encrypted storage on this device.</p></div><div class="settings-actions">${passwordAction}</div></header><details class="settings-disclosure"><summary>Alternate passwords</summary><div>${roles}</div></details></section>`;
}
