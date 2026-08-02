import { describe, expect, it } from "vitest";
import { expiryDurationWords, expiryMomentWords, expiryPickerMarkup, expiryPresets, isExpirySeconds, parseExpirySeconds, type ExpiryBounds } from "./expiry-picker";

const EXPIRY_CONTRACT: ExpiryBounds = {
  minSeconds: 1,
  maxSeconds: 30 * 24 * 60 * 60,
};

describe("expiry picker", () => {
  it("round-trips every contract-permitted second without a component ceiling", () => {
    for (let seconds = EXPIRY_CONTRACT.minSeconds; seconds <= EXPIRY_CONTRACT.maxSeconds; seconds += 1) {
      if (parseExpirySeconds(String(seconds), EXPIRY_CONTRACT) !== seconds) {
        throw new Error(`expiry ${seconds} did not round-trip`);
      }
    }

    const widenedContract: ExpiryBounds = { minSeconds: 1, maxSeconds: EXPIRY_CONTRACT.maxSeconds + 1 };
    expect(parseExpirySeconds(String(widenedContract.maxSeconds), widenedContract)).toBe(widenedContract.maxSeconds);
    expect(isExpirySeconds(EXPIRY_CONTRACT.maxSeconds + 1, EXPIRY_CONTRACT)).toBe(false);
  });

  it("offers coarse buttons, exact numeric entry, and a resulting moment without a select", () => {
    const markup = expiryPickerMarkup({ bounds: EXPIRY_CONTRACT, valueSeconds: 90, nowMs: Date.UTC(2026, 0, 2, 3, 4, 5) });

    expect(expiryPresets(EXPIRY_CONTRACT)).toEqual([1, 60, 3_600, 86_400, EXPIRY_CONTRACT.maxSeconds]);
    expect(markup).toContain('type="number"');
    expect(markup).toContain('min="1"');
    expect(markup).toContain(`max="${EXPIRY_CONTRACT.maxSeconds}"`);
    expect(markup).not.toContain("<select");
    expect(markup).toContain(expiryMomentWords(90, Date.UTC(2026, 0, 2, 3, 4, 5)));
    expect(expiryDurationWords(90)).toBe("90 seconds");
  });
});
