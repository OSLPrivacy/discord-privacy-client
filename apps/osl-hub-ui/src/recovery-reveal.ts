/**
 * Reading the recovery kit back, as a flow that always ends.
 *
 * The screen this drives is the last one standing between an owner and their
 * account: after "Remind me later" every launch lands on "Finish saving your
 * recovery kit", and the only control on it used to be this form. That made
 * every way this flow could fail to finish an unrecoverable brick — press the
 * button, the busy latch goes up, and if the backend never answers it never
 * comes down again, so every later press is swallowed by the `isBusy()` guard
 * and the app is unreachable.
 *
 * The rule here is therefore not "handle the failures we know about". It is
 * that the flow reaches a terminal outcome and drops the busy latch *whatever*
 * the injected work does — resolve, reject, or never settle at all. A hang is
 * bounded by a deadline; a rejection is caught; both end with a message the
 * owner can read. Nothing in this module can leave `setBusy(true)` standing.
 *
 * The escape that makes the screen survivable even when this flow fails is the
 * separate half of the invariant and lives in `recovery-kit.ts`: the
 * `reveal-required` mode always offers "Remind me later" beside the form, and
 * that control is never disabled by the busy latch.
 *
 * Nothing is persisted here. The password is passed straight through to the
 * injected reader and the phrase is handed straight back to the caller.
 */

/**
 * How long a single reveal attempt may stay outstanding before the screen gives
 * the owner an answer instead of a spinner.
 *
 * A deadline abandons the answer, it does not cancel the backend call, which is
 * why a late success is simply discarded rather than applied: by then the owner
 * has been told it failed, and a phrase appearing under a message that says it
 * did not is worse than asking them to press again.
 */
export const RECOVERY_REVEAL_DEADLINE_MS = 20_000;

export const RECOVERY_REVEAL_NO_PASSWORD_ERROR =
  "Enter your password to show your recovery kit.";

export const RECOVERY_REVEAL_WRONG_PASSWORD_ERROR =
  "That password did not open your recovery kit. Nothing was shown.";

export const RECOVERY_REVEAL_UNANSWERED_ERROR =
  "OSL did not answer in time, so your recovery kit was not shown. Try again, or choose “Remind me later” to carry on — OSL will ask again next time it opens.";

export const RECOVERY_REVEAL_FAILED_ERROR =
  "OSL could not open your recovery kit, so nothing was shown. Try again, or choose “Remind me later” to carry on — OSL will ask again next time it opens.";

export type RecoveryRevealOutcome =
  /** No password was typed. Nothing was attempted. */
  | { kind: "no-password" }
  /** An attempt is already outstanding; this press was ignored. */
  | { kind: "already-running" }
  /** The backend returned the phrase. */
  | { kind: "revealed"; passwordPhrase: string }
  /** The backend answered, and said no. */
  | { kind: "rejected" }
  /** The backend did not answer inside the deadline. */
  | { kind: "unanswered" }
  /** The backend, or the transport under it, raised. */
  | { kind: "failed" };

export interface RecoveryRevealActions {
  isBusy(): boolean;
  setBusy(busy: boolean): void;
  setError(message: string | null): void;
  render(): void;
  /**
   * Best-effort capture proof. Its answer is deliberately not consulted: the
   * recovery-kit state machine reads the proof latch itself and holds the
   * secrets back behind the escapable refusal screen when it is not set. All
   * this flow needs is that a stalled proof cannot stall the reveal.
   */
  proveCaptureProtection(): Promise<unknown>;
  readRecoveryPhrase(password: string): Promise<string | null>;
}

export interface RecoveryRevealTimers {
  deadlineMs?: number;
  setTimer?: (fire: () => void, ms: number) => unknown;
  clearTimer?: (handle: unknown) => void;
}

const UNANSWERED = Symbol("recovery-reveal-unanswered");

/**
 * Race one promise against a deadline.
 *
 * The timer is always cleared, including when `work` rejects, so a caller that
 * finishes early never leaves a pending timer holding the process (or a fake
 * clock in a test) open.
 */
async function bounded<T>(
  work: Promise<T>,
  timers: RecoveryRevealTimers,
): Promise<T | typeof UNANSWERED> {
  const deadlineMs = timers.deadlineMs ?? RECOVERY_REVEAL_DEADLINE_MS;
  const setTimer = timers.setTimer ?? ((fire, ms) => setTimeout(fire, ms));
  const clearTimer = timers.clearTimer ?? ((handle) => clearTimeout(handle as ReturnType<typeof setTimeout>));
  let handle: unknown;
  let armed = false;
  try {
    return await Promise.race([
      work,
      new Promise<typeof UNANSWERED>((resolve) => {
        handle = setTimer(() => resolve(UNANSWERED), deadlineMs);
        armed = true;
      }),
    ]);
  } finally {
    if (armed) clearTimer(handle);
  }
}

/**
 * One press of "Show my recovery kit".
 *
 * Every exit from this function has already called `setBusy(false)` and
 * `render()`, and every exit except `revealed` has already put a readable
 * sentence on the screen.
 */
export async function runRecoveryReveal(
  password: string,
  actions: RecoveryRevealActions,
  timers: RecoveryRevealTimers = {},
): Promise<RecoveryRevealOutcome> {
  if (actions.isBusy()) return { kind: "already-running" };
  if (!password) {
    // Silently doing nothing here was itself a dead end: the owner presses the
    // only button on the screen and the app gives no sign it noticed.
    actions.setError(RECOVERY_REVEAL_NO_PASSWORD_ERROR);
    actions.render();
    return { kind: "no-password" };
  }

  actions.setBusy(true);
  actions.setError(null);
  actions.render();

  let outcome: RecoveryRevealOutcome;
  try {
    // A stalled or raising capture proof must not decide whether the owner ever
    // sees this screen again, so it is bounded and its answer is discarded.
    await bounded(actions.proveCaptureProtection(), timers).catch(() => undefined);
    const phrase = await bounded(actions.readRecoveryPhrase(password), timers);
    if (phrase === UNANSWERED) {
      actions.setError(RECOVERY_REVEAL_UNANSWERED_ERROR);
      outcome = { kind: "unanswered" };
    } else if (!phrase) {
      actions.setError(RECOVERY_REVEAL_WRONG_PASSWORD_ERROR);
      outcome = { kind: "rejected" };
    } else {
      actions.setError(null);
      outcome = { kind: "revealed", passwordPhrase: phrase };
    }
  } catch {
    // Deliberately not `localActionError`: nothing derived from the backend's
    // message reaches the screen on the one surface that is holding a secret.
    actions.setError(RECOVERY_REVEAL_FAILED_ERROR);
    outcome = { kind: "failed" };
  } finally {
    actions.setBusy(false);
    actions.render();
  }
  return outcome;
}

/**
 * Whether this keystroke should submit the reveal form.
 *
 * The form has a submit button, so a browser would normally submit on Enter by
 * itself; binding it explicitly means the one screen an owner can be stranded
 * on does not depend on implicit submission surviving a WebView, a modifier
 * being held, or an IME composition being in flight.
 */
export function submitsRecoveryReveal(event: {
  key: string;
  isComposing?: boolean;
  altKey?: boolean;
  ctrlKey?: boolean;
  metaKey?: boolean;
  shiftKey?: boolean;
}): boolean {
  if (event.key !== "Enter") return false;
  if (event.isComposing) return false;
  return !event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey;
}
