import { describe, expect, it } from "vitest";
import {
  MAX_LIVE_BLOB_BYTES,
  MAX_LIVE_BLOB_ROWS,
} from "../src/lib/blob-limits.js";
import { MAX_BLOB_BYTES } from "../src/endpoints/blob.js";

const DEVICE_CAP = 5;
const MANIFEST_ROWS = 1;
const PADDED_TEXT_BYTES = 256;

function rowsPerMessage(members: number): number {
  return members * DEVICE_CAP + MANIFEST_ROWS;
}

function messagesBeforeRowCap(members: number): number {
  return Math.floor(MAX_LIVE_BLOB_ROWS / rowsPerMessage(members));
}

describe("Space fan-out capacity finding", () => {
  it.each([
    [5, 26, 3_846],
    [20, 101, 990],
    [100, 501, 199],
    [500, 2_501, 39],
  ])("accounts for every recipient device and the manifest at %i members", (members, rows, messages) => {
    expect(rowsPerMessage(members)).toBe(rows);
    expect(messagesBeforeRowCap(members)).toBe(messages);
  });

  it("shows that rows bind materially earlier than bytes for padded text", () => {
    const rows = rowsPerMessage(100);
    const byteLimitedMessages = Math.floor(
      MAX_LIVE_BLOB_BYTES / (rows * PADDED_TEXT_BYTES),
    );

    expect(byteLimitedMessages).toBeGreaterThan(messagesBeforeRowCap(100) * 80);
    expect(MAX_BLOB_BYTES).toBe(64 * 1024);
  });

  it("accounts for offline recipient-device copies without counting the manifest repeatedly", () => {
    expect(30 * DEVICE_CAP * 200).toBe(30_000);
    expect(30_000 / MAX_LIVE_BLOB_ROWS).toBe(0.3);
  });
});
