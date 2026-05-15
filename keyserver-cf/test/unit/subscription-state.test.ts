import { describe, expect, it } from "vitest";
import {
  deriveStatus,
  readCurrentPeriodEnd,
} from "../../src/lib/subscription-state.js";

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

// F2.0: the helper that resolves Stripe's billing period across
// the 2025-03-31 API-version cutover, where current_period_end
// migrated from the Subscription to its SubscriptionItems.
describe("readCurrentPeriodEnd", () => {
  it("reads items.data[0].current_period_end (2025-03-31 API shape)", () => {
    const period = 1_800_000_000;
    expect(
      readCurrentPeriodEnd({
        items: { data: [{ current_period_end: period }] },
      }),
    ).toBe(period);
  });

  it("falls back to top-level current_period_end (legacy shape)", () => {
    const period = 1_700_000_000;
    expect(readCurrentPeriodEnd({ current_period_end: period })).toBe(period);
  });

  it("prefers items.data[0] when both paths are present", () => {
    expect(
      readCurrentPeriodEnd({
        current_period_end: 100,
        items: { data: [{ current_period_end: 9_000_000_000 }] },
      }),
    ).toBe(9_000_000_000);
  });

  it("returns null when neither path has a value", () => {
    expect(readCurrentPeriodEnd({})).toBe(null);
    expect(readCurrentPeriodEnd({ items: { data: [] } })).toBe(null);
    expect(readCurrentPeriodEnd({ items: { data: [{}] } })).toBe(null);
  });

  it("ignores non-number values in items.data[0]", () => {
    // Defensive: Stripe wire-shape changes shouldn't crash us; an
    // unexpected string flows through to the legacy fallback.
    const malformed = {
      items: { data: [{ current_period_end: undefined }] },
      current_period_end: 42,
    };
    expect(readCurrentPeriodEnd(malformed)).toBe(42);
  });
});
