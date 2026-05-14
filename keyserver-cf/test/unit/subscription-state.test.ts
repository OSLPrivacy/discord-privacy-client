import { describe, expect, it } from "vitest";
import { deriveStatus } from "../../src/lib/subscription-state.js";

describe("deriveStatus", () => {
  it("active + !cancel → ACTIVE", () => {
    expect(deriveStatus("active", false)).toBe("ACTIVE");
  });

  it("trialing + !cancel → ACTIVE", () => {
    expect(deriveStatus("trialing", false)).toBe("ACTIVE");
  });

  it("active + cancel_at_period_end → CANCELLED", () => {
    expect(deriveStatus("active", true)).toBe("CANCELLED");
  });

  it("trialing + cancel_at_period_end → CANCELLED", () => {
    expect(deriveStatus("trialing", true)).toBe("CANCELLED");
  });

  it("past_due → GRACE", () => {
    expect(deriveStatus("past_due", false)).toBe("GRACE");
  });

  it("canceled → EXPIRED", () => {
    expect(deriveStatus("canceled", false)).toBe("EXPIRED");
  });

  it("unpaid → EXPIRED", () => {
    expect(deriveStatus("unpaid", false)).toBe("EXPIRED");
  });

  it("incomplete_expired → EXPIRED", () => {
    expect(deriveStatus("incomplete_expired", false)).toBe("EXPIRED");
  });

  it("incomplete → PENDING", () => {
    expect(deriveStatus("incomplete", false)).toBe("PENDING");
  });

  it("unknown status falls through to PENDING", () => {
    expect(deriveStatus("future_unknown_state", false)).toBe("PENDING");
  });
});
