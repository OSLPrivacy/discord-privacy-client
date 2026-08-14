const COOLDOWN_ATTEMPT_LIMIT = 10;

export const COOLDOWN_LIMIT_DISCLOSURE = "The 15-minute unlock cooldown can be bypassed by restoring or modifying OSL data files; it is not protection against someone with access to those files.";

/**
 * Give an exact count after every ordinary failure. The tenth submission is
 * represented by the cooldown response itself, rather than another warning.
 */
export function unlockAttemptWarning(attemptsUsed: number): string | null {
  if (
    !Number.isSafeInteger(attemptsUsed)
    || attemptsUsed < 1
    || attemptsUsed >= COOLDOWN_ATTEMPT_LIMIT
  ) return null;

  const attemptsLeft = COOLDOWN_ATTEMPT_LIMIT - attemptsUsed;
  const noun = attemptsLeft === 1 ? "attempt" : "attempts";
  return `${attemptsLeft} ${noun} remaining before a 15-minute cooldown.`;
}
