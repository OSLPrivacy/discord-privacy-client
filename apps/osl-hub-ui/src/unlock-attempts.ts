const AUTO_DESTRUCT_ATTEMPT_LIMIT = 10;
const WARNING_START_ATTEMPT = 7;

/**
 * Explain the remaining safe entries immediately before the gate's
 * auto-destruct boundary. The destructive attempt itself never reaches the
 * locked screen, so it deliberately has no warning copy.
 */
export function unlockAttemptWarning(attemptsUsed: number): string | null {
  if (
    !Number.isSafeInteger(attemptsUsed)
    || attemptsUsed < WARNING_START_ATTEMPT
    || attemptsUsed >= AUTO_DESTRUCT_ATTEMPT_LIMIT
  ) return null;

  const attemptsLeft = AUTO_DESTRUCT_ATTEMPT_LIMIT - attemptsUsed;
  const noun = attemptsLeft === 1 ? "attempt" : "attempts";
  return `${attemptsLeft} ${noun} left before this device is erased.`;
}
