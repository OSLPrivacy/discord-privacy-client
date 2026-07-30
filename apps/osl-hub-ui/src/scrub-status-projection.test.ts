import { describe, expect, it } from "vitest";
import {
  completedScrubRunStatusMarkup,
  parseCompletedScrubRunStatusProjection,
  scrubRunStatusProjectionCounts,
} from "./scrub";

const completedRun = {
  runState: "complete",
  completedAtUnixMs: 1_785_283_200_000,
  userReviewed: true,
  accountBinding: "verified",
  cleanupAuthority: "user_confirmed",
  receipts: [
    { itemOrdinal: 2, status: "still_present", verifiedAtUnixMs: 1_785_283_199_000 },
    { itemOrdinal: 1, status: "verified_gone", verifiedAtUnixMs: 1_785_283_198_000 },
    { itemOrdinal: 3, status: "unknown", verifiedAtUnixMs: null },
  ],
} as const;

describe("completed Scrub run status projection", () => {
  it("strictly parses the completed run receipt projection", () => {
    const parsed = parseCompletedScrubRunStatusProjection(completedRun);

    expect(parsed?.runState).toBe("complete");
    expect(parsed?.receipts.map((receipt) => receipt.status)).toEqual([
      "still_present",
      "verified_gone",
      "unknown",
    ]);
    expect(parsed && scrubRunStatusProjectionCounts(parsed)).toEqual({
      verified_gone: 1,
      still_present: 1,
      unknown: 1,
    });
  });

  it("renders completed receipts as plain cleanup results", () => {
    const markup = completedScrubRunStatusMarkup(completedRun);

    expect(markup).toContain("Cleanup results");
    expect(markup).toContain("1 gone");
    expect(markup).toContain("1 still there");
    expect(markup).toContain("1 unknown");
    expect(markup.indexOf("Item 1")).toBeLessThan(markup.indexOf("Item 2"));
    expect(markup).toContain("Gone");
    expect(markup).toContain("Still there");
    expect(markup).toContain("Unknown");
    expect(markup).toContain("Treat it as still needing review.");
    expect(markup).not.toMatch(/keyservers?|ratchets?|browser profiles?|provider adapters?/i);
  });

  it("refuses missing review, missing binding, malformed states, and impossible rows", () => {
    expect(parseCompletedScrubRunStatusProjection({ ...completedRun, userReviewed: false })).toBeNull();
    expect(parseCompletedScrubRunStatusProjection({ ...completedRun, accountBinding: "unknown" })).toBeNull();
    expect(parseCompletedScrubRunStatusProjection({ ...completedRun, cleanupAuthority: "missing" })).toBeNull();
    expect(parseCompletedScrubRunStatusProjection({ ...completedRun, runState: "running" })).toBeNull();
    expect(parseCompletedScrubRunStatusProjection({
      ...completedRun,
      receipts: [{ itemOrdinal: 1, status: "verified_gone", verifiedAtUnixMs: null }],
    })).toBeNull();
    expect(parseCompletedScrubRunStatusProjection({
      ...completedRun,
      receipts: [
        { itemOrdinal: 1, status: "verified_gone", verifiedAtUnixMs: 1_785_283_198_000 },
        { itemOrdinal: 1, status: "unknown", verifiedAtUnixMs: null },
      ],
    })).toBeNull();
  });

  it("does not echo secret identifiers, account handles, or arbitrary fields", () => {
    const forged = {
      ...completedRun,
      accountId: "private-account-123",
      receipts: [
        {
          itemOrdinal: 1,
          status: "verified_gone",
          verifiedAtUnixMs: 1_785_283_198_000,
          messageLocator: "discord.com/channels/private",
          displayName: "@private_handle",
        },
      ],
    };
    const markup = completedScrubRunStatusMarkup(forged);

    expect(markup).toContain("Cleanup status unavailable");
    expect(markup).not.toContain("private-account-123");
    expect(markup).not.toContain("discord.com/channels/private");
    expect(markup).not.toContain("@private_handle");
  });
});
