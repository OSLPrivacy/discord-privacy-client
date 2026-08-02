import { describe, expect, it } from "vitest";
import { honestStateTone, type HonestState } from "./honest-state";

describe("honest state tones", () => {
  it("reserves the affirmative tone for confirmed evidence", () => {
    expect(honestStateTone("confirmed")).toBe("affirmative");
    expect(honestStateTone("refused")).toBe("refusal");
  });

  it("never presents unknown or unconfirmed evidence as affirmative", () => {
    const unconfirmedStates: HonestState[] = ["not-confirmed", "unknown"];

    for (const state of unconfirmedStates) {
      expect(honestStateTone(state)).not.toBe("affirmative");
      expect(honestStateTone(state)).toBe("neutral");
    }
  });
});
