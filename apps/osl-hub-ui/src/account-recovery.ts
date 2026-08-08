/** The JSON form of crates/ipc `LockoutStatusDto`. */
export interface LockoutStatusDto {
  passwordLockedUntil: number | null;
  passwordAttemptsUsed: number;
  phraseLockedUntil: number | null;
  phraseAttemptsUsed: number;
  now: number;
}

// Imported by the shipping recovery flow so legacy-marker refusals retain the
// explicit migration vocabulary rather than becoming a generic reset error.
export {
  addLegacyPhraseWrap,
  legacyMarkerRecoveryRefused,
  legacyRecoveryMigrationMarkup,
  type LegacyRecoveryMigration,
  type RecoveryMigrationDependencies,
} from "./recovery-migration";

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

/** Same arrow as every other redesigned screen's forward action. */
const RECOVERY_ARROW = `<svg class="signin-icon signin-arrow" viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12 h14"/><path d="M13 6 l6 6 -6 6"/></svg>`;
const RECOVERY_EYE = `<svg viewBox="0 0 20 20" aria-hidden="true"><path d="M1.8 10s2.9-4.7 8.2-4.7 8.2 4.7 8.2 4.7-2.9 4.7-8.2 4.7S1.8 10 1.8 10Z"/><circle cx="10" cy="10" r="2.25"/><path d="M3 3l14 14"/></svg>`;

export function recoveryScreenMarkup(flow: AccountRecoveryFlow): string {
  if (flow.step === "complete") {
    return `<section class="setup-surface recovery-surface"><h1 id="route-heading" tabindex="-1">Password reset</h1><p>Your password was reset. Sign in with the new password.</p></section>`;
  }

  const error = flow.error ? `<p class="unlock-error" role="alert">${flow.error}</p>` : "";
  if (flow.step === "password") {
    return `<section class="stealth-screen restore-screen forgot-screen" aria-labelledby="route-heading">
      <h1 id="route-heading" tabindex="-1" class="stealth-title forgot-title">Choose a new password</h1>
      <p class="stealth-quiet">Use it to unlock this device after the reset</p>
      <form class="password-form stealth-form" data-account-recovery-password novalidate>
        <span class="restore-label-row"><label for="account-recovery-new-password">New password</label><em>6 minimum · 12+ suggested</em></span>
        <div class="password-input-row"><input id="account-recovery-new-password" name="newPassword" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="account-recovery-new-password" aria-controls="account-recovery-new-password" aria-label="Show new password">${RECOVERY_EYE}</button></div>
        <span class="restore-label-row"><label for="account-recovery-confirm-password">Confirm password</label></span>
        <div class="password-input-row"><input id="account-recovery-confirm-password" name="confirmPassword" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="account-recovery-confirm-password" aria-controls="account-recovery-confirm-password" aria-label="Show confirmed password">${RECOVERY_EYE}</button></div>
        ${error}
        <button class="stealth-submit restore-submit" id="account-recovery-continue" type="submit" disabled><span>Continue</span>${RECOVERY_ARROW}</button>
      </form>
      <div class="setup-footer onboarding-actions stealth-links restore-links"><button class="text-button" type="button" data-account-recovery-back>← Back</button></div>
    </section>`;
  }

  // 2026-08-06 restyle. Built from the same parts as the restore and password
  // screens. The paragraph became one quiet line, and it keeps the sentence
  // that actually matters: this is NOT the identity phrase. Two phrases with
  // similar names and different consequences is the confusion worth spending a
  // line on.
  return `<section class="stealth-screen restore-screen forgot-screen" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1" class="stealth-title forgot-title">Password reset</h1>
    <p class="stealth-quiet">Your password recovery phrase sets a new password. It is not your identity phrase</p>
    <form class="password-form stealth-form" data-account-recovery-phrase novalidate>
      <span class="restore-label-row"><label for="account-recovery-phrase">Password recovery phrase</label><em>stays on this device</em></span>
      <textarea class="restore-phrase" id="account-recovery-phrase" name="recoveryPhrase" rows="3" autocomplete="off" autocapitalize="none" spellcheck="false" required></textarea>
      ${error}
      <button class="stealth-submit restore-submit" type="submit"><span>Verify phrase</span>${RECOVERY_ARROW}</button>
    </form>
    <div class="setup-footer onboarding-actions stealth-links restore-links"><button class="text-button" type="button" data-onboarding="welcome">← Back</button></div>
  </section>`;
}
