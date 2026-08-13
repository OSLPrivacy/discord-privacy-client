const COOLDOWN_ATTEMPT_LIMIT = 10;

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
