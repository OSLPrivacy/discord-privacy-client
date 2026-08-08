/**
 * Recovery-kit retype gate.
 *
 * The words typed here are not treated as proof by this module. Continue is
 * enabled only after the trusted native check returns one positive match for
 * every requested position. Any edit discards that proof immediately.
 */

export const RECOVERY_WORD_CHECK_POSITIONS = [1, 11, 12] as const;

export type RecoveryWordCheckPosition = (typeof RECOVERY_WORD_CHECK_POSITIONS)[number];

export interface RecoveryWordRetypeAnswer {
  position: number;
  word: string;
}

/** Shape sent to the trusted `check_hub_recovery_word_retype` adapter. */
export interface RecoveryWordRetypeRequest {
  recoveryPhrase: string;
  selectedPositions: number[];
  answers: RecoveryWordRetypeAnswer[];
}

/** Shape returned by the trusted `check_hub_recovery_word_retype` adapter. */
export interface RecoveryWordRetypePrompt {
  position: number;
}

export interface RecoveryWordRetypeResult {
  prompts: RecoveryWordRetypePrompt[];
  checkedCount: number;
  passed: boolean;
  failedPositions: number[];
}

export interface RecoveryWordCheckState {
  readonly answers: Readonly<Record<RecoveryWordCheckPosition, string>>;
  /** Null means there is no native proof for the current input values. */
  readonly nativeResult: RecoveryWordRetypeResult | null;
}

export function initialRecoveryWordCheckState(): RecoveryWordCheckState {
  return {
    answers: { 1: "", 11: "", 12: "" },
    nativeResult: null,
  };
}

function requestedPosition(position: number): position is RecoveryWordCheckPosition {
  return (RECOVERY_WORD_CHECK_POSITIONS as readonly number[]).includes(position);
}

/**
 * Change one answer and invalidate any earlier native proof. Unknown positions
 * are rejected so a wiring mistake cannot silently weaken the three-word gate.
 */
export function setRecoveryWordCheckAnswer(
  state: RecoveryWordCheckState,
  position: number,
  word: string,
): RecoveryWordCheckState {
  if (!requestedPosition(position)) {
    throw new Error(`Recovery word ${position} was not requested`);
  }
  return {
    answers: { ...state.answers, [position]: word },
    nativeResult: null,
  };
}

export function recoveryWordRetypeAnswers(state: RecoveryWordCheckState): RecoveryWordRetypeAnswer[] {
  return RECOVERY_WORD_CHECK_POSITIONS.map((position) => ({
    position,
    word: state.answers[position].trim(),
  }));
}

export function recoveryWordRetypeRequest(
  state: RecoveryWordCheckState,
  recoveryPhrase: string,
): RecoveryWordRetypeRequest {
  return {
    recoveryPhrase,
    selectedPositions: [...RECOVERY_WORD_CHECK_POSITIONS],
    answers: recoveryWordRetypeAnswers(state),
  };
}

export function everyRecoveryWordAnswered(state: RecoveryWordCheckState): boolean {
  return RECOVERY_WORD_CHECK_POSITIONS.every((position) => state.answers[position].trim().length > 0);
}

/** Record the native answer for the current input values. */
export function applyRecoveryWordRetypeResult(
  state: RecoveryWordCheckState,
  result: RecoveryWordRetypeResult,
): RecoveryWordCheckState {
  return { ...state, nativeResult: result };
}

/**
 * Require the native result to describe exactly the requested positions and
 * all three answers. `passed: true` alone is insufficient.
 */
export function recoveryWordCheckPassed(state: RecoveryWordCheckState): boolean {
  const result = state.nativeResult;
  if (!result?.passed
    || result.checkedCount !== RECOVERY_WORD_CHECK_POSITIONS.length
    || result.failedPositions.length !== 0
    || result.prompts.length !== RECOVERY_WORD_CHECK_POSITIONS.length) return false;

  return result.prompts.every((prompt, index) =>
    prompt.position === RECOVERY_WORD_CHECK_POSITIONS[index]);
}

export function recoveryWordCheckContinueDisabled(state: RecoveryWordCheckState): boolean {
  return !recoveryWordCheckPassed(state);
}

function defaultEscape(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

/** Markup kept pure so main.ts can place this page in the onboarding route. */
export function recoveryWordCheckMarkup(
  state: RecoveryWordCheckState,
  escape: (value: string) => string = defaultEscape,
): string {
  const inputs = RECOVERY_WORD_CHECK_POSITIONS.map((position) => `
    <label class="recovery-word-check-row" for="recovery-word-${position}">
      <span>Word ${position}</span>
      <input id="recovery-word-${position}" data-recovery-word-position="${position}" type="text" autocomplete="off" autocapitalize="none" spellcheck="false" value="${escape(state.answers[position])}"/>
    </label>`).join("");
  const disabled = recoveryWordCheckContinueDisabled(state)
    ? ' disabled aria-disabled="true"'
    : "";
  return `<section class="recovery-word-check-screen" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1">Check your recovery words</h1>
    <p>Retype the requested words from the recovery kit you saved.</p>
    <div class="recovery-word-check-list">${inputs}
    </div>
    <p class="recovery-word-check-status" id="recovery-word-check-status" role="status" aria-live="polite"></p>
    <button class="button primary" id="recovery-word-check-continue" type="button"${disabled}>Continue</button>
  </section>`;
}
