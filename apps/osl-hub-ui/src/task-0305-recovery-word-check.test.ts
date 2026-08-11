import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import {
  RECOVERY_WORD_CHECK_COUNT,
  applyRecoveryWordRetypeResult,
  everyRecoveryWordAnswered,
  initialRecoveryWordCheckState,
  recoveryWordCheckContinueDisabled,
  recoveryWordCheckMarkup,
  recoveryWordRetypeRequest,
  selectRecoveryWordCheckPositions,
  setRecoveryWordCheckAnswer,
  type RecoveryWordCheckState,
  type RecoveryWordRetypeResult,
} from "./recovery-word-check";

const POSITIONS = [1, 11, 12] as const;
const WORDS: Record<number, string> = { 1: "abandon", 11: "abandon", 12: "about" };

function answer(state: RecoveryWordCheckState, count: number): RecoveryWordCheckState {
  return state.positions.slice(0, count).reduce(
    (current, position) => setRecoveryWordCheckAnswer(current, position, WORDS[position]),
    state,
  );
}

const exactNativeResult: RecoveryWordRetypeResult = {
  prompts: POSITIONS.map((position) => ({ position })),
  checkedCount: 3,
  passed: true,
  failedPositions: [],
};

describe("TASK0305 recovery-word retype gate", () => {
  it("is wired between the recovery-kit page and later setup", () => {
    const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const sequence = readFileSync(new URL("./onboarding-sequence.ts", import.meta.url), "utf8");
    expect(sequence).toMatch(/"recovery",\s*"recovery-check",\s*"pro"/u);
    expect(main).toContain('if (onboardingRoute === "recovery-check") return recoveryWordCheckMarkup');
    expect(main).toMatch(/recoveryContinue\?\.addEventListener\("click"[\s\S]*?onboardingRoute = "recovery-check"/u);
    expect(main).toMatch(/#recovery-word-check-continue[\s\S]*?recoveryWordCheckContinueDisabled\(recoveryWordCheckState\)[\s\S]*?applyRecoveryKitAction\(\{ kind: "continue" \}\)/u);
    expect(main).toContain("epoch !== recoveryWordCheckEpoch");
    console.info("TASK0305 route=recovery->recovery-check->pro requested=3");
  });

  it("keeps Continue disabled through 0, 1, and 2 answers", () => {
    const initial = initialRecoveryWordCheckState(POSITIONS);
    expect(RECOVERY_WORD_CHECK_COUNT).toBe(3);
    expect(recoveryWordCheckContinueDisabled(initial)).toBe(true);
    expect(recoveryWordCheckMarkup(initial)).toContain('id="recovery-word-check-continue" type="button" disabled aria-disabled="true"');

    for (const count of [0, 1, 2]) {
      const state = answer(initial, count);
      expect(recoveryWordCheckContinueDisabled(state), `${count}/3 answers must stay disabled`).toBe(true);
      expect(everyRecoveryWordAnswered(state)).toBe(false);
    }
  });

  it("does not accept a correct word checked at the wrong position", () => {
    const filled = answer(initialRecoveryWordCheckState(POSITIONS), 3);
    expect(everyRecoveryWordAnswered(filled)).toBe(true);
    expect(recoveryWordCheckContinueDisabled(filled), "filled text is not native proof").toBe(true);

    const wrongPosition = applyRecoveryWordRetypeResult(filled, {
      prompts: POSITIONS.map((position) => ({ position })),
      checkedCount: 3,
      passed: false,
      failedPositions: [1],
    });
    expect(recoveryWordCheckContinueDisabled(wrongPosition)).toBe(true);

    // Even a malformed `passed: true` response cannot substitute an unrequested
    // position for one of the three prompts.
    const substitutedPosition = applyRecoveryWordRetypeResult(filled, {
      prompts: [
        { position: 1 },
        { position: 11 },
        { position: 10 },
      ],
      checkedCount: 3,
      passed: true,
      failedPositions: [],
    });
    expect(recoveryWordCheckContinueDisabled(substitutedPosition)).toBe(true);
  });

  it("enables only for all three exact native matches, then an edit disables it again", () => {
    const filled = answer(initialRecoveryWordCheckState(POSITIONS), 3);
    const phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    expect(recoveryWordRetypeRequest(filled, phrase)).toEqual({
      recoveryPhrase: phrase,
      selectedPositions: [1, 11, 12],
      answers: [
        { position: 1, word: "abandon" },
        { position: 11, word: "abandon" },
        { position: 12, word: "about" },
      ],
    });

    const passed = applyRecoveryWordRetypeResult(filled, exactNativeResult);
    expect(recoveryWordCheckContinueDisabled(passed)).toBe(false);
    expect(recoveryWordCheckMarkup(passed)).not.toContain('id="recovery-word-check-continue" type="button" disabled');

    const edited = setRecoveryWordCheckAnswer(passed, 11, "abstract");
    expect(edited.nativeResult).toBeNull();
    expect(recoveryWordCheckContinueDisabled(edited)).toBe(true);

    console.info(
      "TASK0305 requested=3 disabled_at=initial,0,1,2,wrong-position enabled_at=3-exact disabled_after_edit=1",
    );
  });

  it("chooses fresh unpredictable positions for each journey", () => {
    const firstValues = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    const secondValues = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    const first = selectRecoveryWordCheckPositions(() => firstValues.shift() ?? 0);
    const second = selectRecoveryWordCheckPositions(() => secondValues.shift() ?? 0);
    expect(first).toHaveLength(3);
    expect(new Set(first).size).toBe(3);
    expect(second).toHaveLength(3);
    expect(new Set(second).size).toBe(3);
    expect(second).not.toEqual(first);
    console.info(`TASK0334A_UNPREDICTABLE first=${first.join(",")} second=${second.join(",")} count=3`);
  });
});
