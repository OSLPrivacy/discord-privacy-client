import { describe, expect, it } from "vitest";
import {
  enforceArchiveByteBudget,
  expiredArchiveEntries,
  nextArchiveWakeAt,
  type ArchiveEntry,
} from "../../src/archive/policy.js";

const entry = (
  id: string,
  receivedAt: number,
  expiresAt: number,
  byteLength: number,
): ArchiveEntry => ({ id, objectKey: `archive/${id}`, receivedAt, expiresAt, byteLength });

describe("Archive retained-pool policy", () => {
  it("evicts the oldest entries first, retaining the new entry when a byte budget is exceeded", () => {
    const oldest = entry("oldest", 100, 10_000, 4);
    const middle = entry("middle", 200, 10_000, 4);
    const newest = entry("newest", 300, 10_000, 4);

    expect(enforceArchiveByteBudget([oldest, middle, newest], 8).map((row) => row.id))
      .toEqual(["middle", "newest"]);
  });

  it("expires only rows whose deadline has arrived and schedules the earliest remaining deadline", () => {
    const expired = entry("expired", 100, 999, 4);
    const firstLive = entry("first-live", 200, 1_000, 4);
    const laterLive = entry("later-live", 300, 1_500, 4);

    expect(expiredArchiveEntries([expired, firstLive, laterLive], 1_000).map((row) => row.id))
      .toEqual(["expired", "first-live"]);
    expect(nextArchiveWakeAt([firstLive, laterLive])).toBe(1_000);
  });
});
