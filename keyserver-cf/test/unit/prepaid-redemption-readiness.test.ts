import { describe, expect, it } from "vitest";
import { prepaidRedemptionReady } from "../../src/lib/prepaid-redemption-readiness.js";

describe("prepaid-code redemption readiness", () => {
  it("cannot be enabled by deployment configuration", () => {
    expect(prepaidRedemptionReady()).toBe(false);
  });
});
