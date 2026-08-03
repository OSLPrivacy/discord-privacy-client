import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { sha256Hex } from "../src/lib/digest.js";
import { d1Run } from "./helpers/workerd.js";

const LIVE_ID = "d3c0a00000000017";
const DECOY_ID = "d3c0a00000000018";
const FETCH_CAP = "0123456789abcdef0123456789abcdef";
const WRONG_FETCH_CAP = "fedcba9876543210fedcba9876543210";

async function fingerprint(response: Response): Promise<{
  status: number;
  headers: [string, string][];
  body: Uint8Array;
}> {
  return {
    status: response.status,
    headers: [...response.headers.entries()].sort(([left], [right]) => left.localeCompare(right)),
    body: new Uint8Array(await response.arrayBuffer()),
  };
}

describe("decoy fetch activity", () => {
  it("is indistinguishable from every capability-negative fetch", async () => {
    const now = Math.floor(Date.now() / 1000);
    await d1Run(
      `INSERT INTO blob_capability_index (
         blob_id, fetch_digest_sha256_hex, ack_digest_sha256_hex,
         manage_digest_sha256_hex, object_class, pool, delivery_tag,
         size_bytes, expires_at, created_at
       ) VALUES (?, ?, ?, ?, 'single-ack', 'undelivered', ?, ?, ?, ?)`,
      LIVE_ID.padStart(32, "0"),
      await sha256Hex(FETCH_CAP),
      "a".repeat(64),
      "b".repeat(64),
      "c".repeat(32),
      1,
      now + 3600,
      now,
    );

    // A decoy fetch is just a normal-looking miss. It carries neither a
    // marker nor a special capability that could reveal its purpose.
    const decoyMiss = await SELF.fetch(`https://cipher.test/v1/blob/${DECOY_ID.padStart(32, "0")}`, {
      headers: { "cf-connecting-ip": "198.51.100.117", "x-osl-fetch-cap": WRONG_FETCH_CAP },
    });
    const missingCap = await SELF.fetch(`https://cipher.test/v1/blob/${LIVE_ID.padStart(32, "0")}`, {
      headers: { "cf-connecting-ip": "198.51.100.118" },
    });
    const wrongCap = await SELF.fetch(`https://cipher.test/v1/blob/${LIVE_ID.padStart(32, "0")}`, {
      headers: {
        "cf-connecting-ip": "198.51.100.119",
        "x-osl-fetch-cap": WRONG_FETCH_CAP,
      },
    });

    const decoy = await fingerprint(decoyMiss);
    expect(decoy.status).toBe(404);
    expect(await fingerprint(missingCap)).toEqual(decoy);
    expect(await fingerprint(wrongCap)).toEqual(decoy);
  });
});
