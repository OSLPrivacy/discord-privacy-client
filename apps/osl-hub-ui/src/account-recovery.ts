/** The JSON form of crates/ipc `LockoutStatusDto`. */
export interface LockoutStatusDto {
  passwordLockedUntil: number | null;
  passwordAttemptsUsed: number;
  phraseLockedUntil: number | null;
  phraseAttemptsUsed: number;
  now: number;
}

export type AccountRecoveryFlow = {
  step: "phrase" | "password" | "complete";
  recoveryToken: string | null;
  lockoutStatus: LockoutStatusDto | null;
  error: string | null;
};

export type RecoveryPhraseVerification =
  | { ok: true; recoveryToken: string; lockoutStatus: LockoutStatusDto }
  | { ok: false; lockoutStatus: LockoutStatusDto };

export type AccountRecoveryDependencies = {
  verifyPhrase: (phrase: string) => Promise<RecoveryPhraseVerification>;
  setPassword: (newPassword: string, recoveryToken: string) => Promise<void>;
};

export const initialAccountRecoveryFlow: AccountRecoveryFlow = {
  step: "phrase",
  recoveryToken: null,
  lockoutStatus: null,
  error: null,
};

function validNewPassword(password: string): boolean {
  return /^[\x20-\x7e]{6,128}$/.test(password);
}

function phraseLockoutMessage(status: LockoutStatusDto): string {
  const attempts = status.phraseAttemptsUsed;
  const lockedFor = status.phraseLockedUntil === null ? 0 : Math.max(0, status.phraseLockedUntil - status.now);
  const attemptWord = attempts === 1 ? "attempt" : "attempts";
  if (lockedFor > 0) {
    return `${attempts} recovery phrase ${attemptWord} recorded. Try again in ${lockedFor} seconds.`;
  }
  return `${attempts} recovery phrase ${attemptWord} recorded. You can try again now.`;
}

/**
 * Keep the reset form unavailable until the native verifier returns a usable,
 * opaque recovery token. A malformed or rejected response never advances.
 */
export async function submitRecoveryPhrase(
  flow: AccountRecoveryFlow,
  phrase: string,
  dependencies: AccountRecoveryDependencies,
): Promise<AccountRecoveryFlow> {
  if (flow.step !== "phrase" || !phrase.trim()) {
    return { ...initialAccountRecoveryFlow, lockoutStatus: flow.lockoutStatus, error: "Enter your password recovery phrase." };
  }

  try {
    const result = await dependencies.verifyPhrase(phrase.trim());
    if (result.ok && typeof result.recoveryToken === "string" && result.recoveryToken.length > 0) {
      return { step: "password", recoveryToken: result.recoveryToken, lockoutStatus: result.lockoutStatus, error: null };
    }
    return { ...initialAccountRecoveryFlow, lockoutStatus: result.lockoutStatus, error: phraseLockoutMessage(result.lockoutStatus) };
  } catch {
    return { ...initialAccountRecoveryFlow, lockoutStatus: flow.lockoutStatus, error: "We could not verify that recovery phrase. No password was changed." };
  }
}

/** Set the replacement password only for a live token returned by verification. */
export async function submitRecoveredPassword(
  flow: AccountRecoveryFlow,
  newPassword: string,
  confirmPassword: string,
  dependencies: AccountRecoveryDependencies,
): Promise<AccountRecoveryFlow> {
  if (flow.step !== "password" || !flow.recoveryToken) return initialAccountRecoveryFlow;
  if (!validNewPassword(newPassword)) {
    return { ...flow, error: "Choose a password with 6 to 128 printable characters." };
  }
  if (newPassword !== confirmPassword) return { ...flow, error: "The new passwords do not match." };

  try {
    await dependencies.setPassword(newPassword, flow.recoveryToken);
    return { ...flow, step: "complete", recoveryToken: null, error: null };
  } catch {
    return { ...flow, error: "We could not reset the password. Your existing data was not changed." };
  }
}

export function recoveryScreenMarkup(flow: AccountRecoveryFlow): string {
  if (flow.step === "complete") {
    return `<section class="setup-surface recovery-surface"><h1 id="route-heading" tabindex="-1">Password reset</h1><p>Your password was reset. Sign in with the new password.</p></section>`;
  }

  const error = flow.error ? `<p class="unlock-error" role="alert">${flow.error}</p>` : "";
  if (flow.step === "password") {
    return `<section class="setup-surface recovery-surface"><h1 id="route-heading" tabindex="-1">Choose a new password</h1><p>Use a new password to unlock this device. Your recovery phrase is not saved here.</p><form data-account-recovery-password novalidate><label for="account-recovery-new-password">New password</label><input id="account-recovery-new-password" name="newPassword" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><label for="account-recovery-confirm-password">Confirm new password</label><input id="account-recovery-confirm-password" name="confirmPassword" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/>${error}<button class="button primary" type="submit">Reset password</button></form></section>`;
  }

  return `<section class="setup-surface recovery-surface"><h1 id="route-heading" tabindex="-1">Forgot password?</h1><p>Enter your password recovery phrase to choose a new password. This does not replace your identity recovery phrase.</p><form data-account-recovery-phrase novalidate><label for="account-recovery-phrase">Password recovery phrase</label><textarea id="account-recovery-phrase" name="recoveryPhrase" autocomplete="off" autocapitalize="none" spellcheck="false" required></textarea>${error}<button class="button primary" type="submit">Verify phrase</button></form></section>`;
}
