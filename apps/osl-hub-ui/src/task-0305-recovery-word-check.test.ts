import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import {
  RECOVERY_WORD_CHECK_POSITIONS,
  applyRecoveryWordRetypeResult,
  everyRecoveryWordAnswered,
  initialRecoveryWordCheckState,
  recoveryWordCheckContinueDisabled,
  recoveryWordCheckMarkup,
  recoveryWordRetypeRequest,
  setRecoveryWordCheckAnswer,
  type RecoveryWordCheckState,
  type RecoveryWordRetypeResult,
} from "./recovery-word-check";

const WORDS = {
  1: "abandon",
  11: "abandon",
  12: "about",
} as const;

function answer(state: RecoveryWordCheckState, count: number): RecoveryWordCheckState {
  return RECOVERY_WORD_CHECK_POSITIONS.slice(0, count).reduce(
    (current, position) => setRecoveryWordCheckAnswer(current, position, WORDS[position]),
    state,
  );
}

const exactNativeResult: RecoveryWordRetypeResult = {
  prompts: RECOVERY_WORD_CHECK_POSITIONS.map((position) => ({ position })),
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
    const initial = initialRecoveryWordCheckState();
    expect(RECOVERY_WORD_CHECK_POSITIONS).toEqual([1, 11, 12]);
    expect(recoveryWordCheckContinueDisabled(initial)).toBe(true);
    expect(recoveryWordCheckMarkup(initial)).toContain('id="recovery-word-check-continue" type="button" disabled aria-disabled="true"');

    for (const count of [0, 1, 2]) {
      const state = answer(initial, count);
      expect(recoveryWordCheckContinueDisabled(state), `${count}/3 answers must stay disabled`).toBe(true);
      expect(everyRecoveryWordAnswered(state)).toBe(false);
    }
  });

  it("does not accept a correct word checked at the wrong position", () => {
    const filled = answer(initialRecoveryWordCheckState(), 3);
    expect(everyRecoveryWordAnswered(filled)).toBe(true);
    expect(recoveryWordCheckContinueDisabled(filled), "filled text is not native proof").toBe(true);

    const wrongPosition = applyRecoveryWordRetypeResult(filled, {
      prompts: RECOVERY_WORD_CHECK_POSITIONS.map((position) => ({ position })),
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
    const filled = answer(initialRecoveryWordCheckState(), 3);
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
});
