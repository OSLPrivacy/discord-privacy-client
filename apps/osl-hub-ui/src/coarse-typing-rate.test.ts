import { describe, expect, it } from "vitest";
import { CoarseTypingRate } from "./coarse-typing-rate";

describe("coarse current-message typing rate", () => {
  it("ignores untrusted and invalid samples", () => {
    const rate = new CoarseTypingRate();
    rate.recordTrustedInput(0, false, 1);
    rate.recordTrustedInput(Number.NaN, true, 2);
    rate.recordTrustedInput(100, true, 1_001);
    expect(rate.charsPerSecond()).toBe(0);
  });

  it("uses a bounded quantized aggregate instead of an interval sequence", () => {
    const rate = new CoarseTypingRate();
    rate.recordTrustedInput(0, true, 1);
    rate.recordTrustedInput(1_000, true, 6);
    expect(rate.charsPerSecond()).toBe(6);

    const fast = new CoarseTypingRate();
    fast.recordTrustedInput(0, true, 1);
    fast.recordTrustedInput(500, true, 100);
    expect(fast.charsPerSecond()).toBe(16);
  });

  it("clamps slow input and completely resets between messages", () => {
    const rate = new CoarseTypingRate();
    rate.recordTrustedInput(0, true, 1);
    rate.recordTrustedInput(10_000, true, 3);
    expect(rate.charsPerSecond()).toBe(2);
    rate.reset();
    expect(rate.charsPerSecond()).toBe(0);
  });
});
