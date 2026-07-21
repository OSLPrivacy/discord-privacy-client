import { describe, expect, it } from "vitest";
import { TwoStepBurnConfirmation } from "./two-step-burn";

describe("native overlay two-step burn confirmation", () => {
  it("requires two trusted gestures inside the confirmation window", () => {
    const confirmation = new TwoStepBurnConfirmation();
    expect(confirmation.step(0, false)).toBe("ignored");
    expect(confirmation.step(0, true)).toBe("armed");
    expect(confirmation.step(9_999, true)).toBe("confirmed");
    expect(confirmation.step(10_000, true)).toBe("armed");
  });

  it("expires and resets without confirming", () => {
    const confirmation = new TwoStepBurnConfirmation();
    expect(confirmation.step(100, true)).toBe("armed");
    expect(confirmation.expire(10_100)).toBe(true);
    expect(confirmation.step(10_101, true)).toBe("armed");
    confirmation.reset();
    expect(confirmation.step(10_102, true)).toBe("armed");
  });
});
