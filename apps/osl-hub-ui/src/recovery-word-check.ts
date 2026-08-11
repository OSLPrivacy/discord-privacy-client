/**
 * Recovery-kit retype gate.
 *
 * The words typed here are not treated as proof by this module. Continue is
 * enabled only after the trusted native check returns one positive match for
 * every requested position. Any edit discards that proof immediately.
 */

export const RECOVERY_WORD_CHECK_COUNT = 3;
export const RECOVERY_WORD_COUNT = 12;

export type RecoveryWordCheckPosition = number;

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
  readonly positions: readonly RecoveryWordCheckPosition[];
  readonly answers: Readonly<Record<RecoveryWordCheckPosition, string>>;
  /** Null means there is no native proof for the current input values. */
  readonly nativeResult: RecoveryWordRetypeResult | null;
}

function secureRandomUint32(): number {
  const values = new Uint32Array(1);
  globalThis.crypto.getRandomValues(values);
  return values[0];
}

/** Choose three fresh, distinct positions for this recovery journey. */
export function selectRecoveryWordCheckPositions(
  randomUint32: () => number = secureRandomUint32,
): RecoveryWordCheckPosition[] {
  const available = Array.from({ length: RECOVERY_WORD_COUNT }, (_, index) => index + 1);
  for (let index = available.length - 1; index > 0; index -= 1) {
    const swap = randomUint32() % (index + 1);
    [available[index], available[swap]] = [available[swap], available[index]];
  }
  return available.slice(0, RECOVERY_WORD_CHECK_COUNT).sort((left, right) => left - right);
}

export function initialRecoveryWordCheckState(
  positions: readonly RecoveryWordCheckPosition[] = selectRecoveryWordCheckPositions(),
): RecoveryWordCheckState {
  const normalized = [...positions];
  if (normalized.length !== RECOVERY_WORD_CHECK_COUNT
    || new Set(normalized).size !== RECOVERY_WORD_CHECK_COUNT
    || normalized.some((position) => !Number.isInteger(position) || position < 1 || position > RECOVERY_WORD_COUNT)) {
    throw new Error("Recovery word check requires three distinct positions");
  }
  return {
    positions: normalized,
    answers: Object.fromEntries(normalized.map((position) => [position, ""])),
    nativeResult: null,
  };
}

function requestedPosition(state: RecoveryWordCheckState, position: number): position is RecoveryWordCheckPosition {
  return state.positions.includes(position);
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
  if (!requestedPosition(state, position)) {
    throw new Error(`Recovery word ${position} was not requested`);
  }
  return {
    positions: state.positions,
    answers: { ...state.answers, [position]: word },
    nativeResult: null,
  };
}

export function recoveryWordRetypeAnswers(state: RecoveryWordCheckState): RecoveryWordRetypeAnswer[] {
  return state.positions.map((position) => ({
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
    selectedPositions: [...state.positions],
    answers: recoveryWordRetypeAnswers(state),
  };
}

export function everyRecoveryWordAnswered(state: RecoveryWordCheckState): boolean {
  return state.positions.every((position) => state.answers[position].trim().length > 0);
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
    || result.checkedCount !== state.positions.length
    || result.failedPositions.length !== 0
    || result.prompts.length !== state.positions.length) return false;

  return result.prompts.every((prompt, index) =>
    prompt.position === state.positions[index]);
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
  const inputs = state.positions.map((position) => `
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
