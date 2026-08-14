/** Recovery-kit retype gate. Native proof, rather than filled inputs, enables Continue. */
export const RECOVERY_WORD_CHECK_POSITIONS = [1, 11, 12] as const;

export type RecoveryWordCheckPosition = (typeof RECOVERY_WORD_CHECK_POSITIONS)[number];

export interface RecoveryWordRetypeAnswer {
  position: number;
  word: string;
}

export interface RecoveryWordRetypeRequest {
  recoveryPhrase: string;
  selectedPositions: number[];
  answers: RecoveryWordRetypeAnswer[];
}

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
  readonly nativeResult: RecoveryWordRetypeResult | null;
}

export function initialRecoveryWordCheckState(): RecoveryWordCheckState {
  return { answers: { 1: "", 11: "", 12: "" }, nativeResult: null };
}

function requestedPosition(position: number): position is RecoveryWordCheckPosition {
  return (RECOVERY_WORD_CHECK_POSITIONS as readonly number[]).includes(position);
}

export function setRecoveryWordCheckAnswer(
  state: RecoveryWordCheckState,
  position: number,
  word: string,
): RecoveryWordCheckState {
  if (!requestedPosition(position)) throw new Error(`Recovery word ${position} was not requested`);
  return { answers: { ...state.answers, [position]: word }, nativeResult: null };
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

export function applyRecoveryWordRetypeResult(
  state: RecoveryWordCheckState,
  result: RecoveryWordRetypeResult,
): RecoveryWordCheckState {
  return { ...state, nativeResult: result };
}

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
