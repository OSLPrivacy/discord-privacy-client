/**
 * T15-A8 — the recovery step must survive a restart.
 *
 * `pendingOnboardingRoute()` accepted ten routes and `recovery` was not one of
 * them, so relaunching OSL resumed at `pro` and the recovery step was skipped
 * silently. The phrases were module-local state, so they were gone too: the
 * owner ended up with an account nobody could ever recover, and nothing on
 * screen said so.
 *
 * The fix is a single durable boolean — "a recovery kit exists that the owner
 * never confirmed saving". While it is set, resume always lands on `recovery`,
 * where the kit is re-read from the encrypted marker via the backend rather
 * than from anything this layer wrote down.
 *
 * **Nothing secret is persisted here.** The only value written is the string
 * `"1"` under one key. The phrases themselves stay encrypted at rest inside
 * the password marker and are only ever decrypted in memory, on demand,
 * against the owner's password.
 */

export const RECOVERY_KIT_UNSAVED_STORAGE_KEY = "osl-recovery-kit-unsaved-v1";

export const RESUMABLE_ONBOARDING_ROUTES = [
  "pro",
  "privacy",
  "defaults",
  "tor",
  "sending",
  "cover",
  "passwords",
  "burnpass",
  "mullvad",
  "browser",
  "tutorial",
] as const;

export type ResumableOnboardingRoute = (typeof RESUMABLE_ONBOARDING_ROUTES)[number];
export type ResumedOnboardingRoute = ResumableOnboardingRoute | "recovery";

export interface OnboardingResumeStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export function isResumableOnboardingRoute(value: unknown): value is ResumableOnboardingRoute {
  return typeof value === "string"
    && (RESUMABLE_ONBOARDING_ROUTES as readonly string[]).includes(value);
}

export function recoveryKitUnsaved(storage: OnboardingResumeStorage): boolean {
  return storage.getItem(RECOVERY_KIT_UNSAVED_STORAGE_KEY) === "1";
}

export function markRecoveryKitUnsaved(storage: OnboardingResumeStorage): void {
  storage.setItem(RECOVERY_KIT_UNSAVED_STORAGE_KEY, "1");
}

export function clearRecoveryKitUnsaved(storage: OnboardingResumeStorage): void {
  storage.removeItem(RECOVERY_KIT_UNSAVED_STORAGE_KEY);
}

/**
 * Where a relaunch resumes.
 *
 * An unsaved recovery kit outranks every other pending step, and the pending
 * step is deliberately left in storage so finishing recovery returns the owner
 * to where they actually were.
 */
export function resumeOnboardingRoute(
  storage: OnboardingResumeStorage,
  resumeKey: string,
): ResumedOnboardingRoute | null {
  if (recoveryKitUnsaved(storage)) return "recovery";
  const pending = storage.getItem(resumeKey);
  if (isResumableOnboardingRoute(pending)) return pending;
  if (pending !== null) storage.removeItem(resumeKey);
  return null;
}
