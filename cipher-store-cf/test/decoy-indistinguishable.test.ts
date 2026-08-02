import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { d1Run } from "./helpers/workerd.js";

const LIVE_ID = "d3c0a00000000017";
const DECOY_ID = "d3c0a00000000018";
const FETCH_CAP = "0123456789abcdef0123456789abcdef";
const WRONG_FETCH_CAP = "fedcba9876543210fedcba9876543210";

function idBytes(hex: string): Uint8Array {
  const bytes = new Uint8Array(hex.length / 2);
  for (let index = 0; index < bytes.length; index++) {
    bytes[index] = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
  }
  return bytes;
}

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
      `INSERT INTO blobs (id, data, size_bytes, expires_at, created_at, fetch_token)
       VALUES (?, ?, ?, ?, ?, ?)`,
      idBytes(LIVE_ID),
      new Uint8Array([1]),
      1,
      now + 3600,
      now,
      FETCH_CAP,
    );

    // A decoy fetch is just a normal-looking miss. It carries neither a
    // marker nor a special capability that could reveal its purpose.
    const decoyMiss = await SELF.fetch(`https://cipher.test/v1/blob/${DECOY_ID}`, {
      headers: { "cf-connecting-ip": "198.51.100.117" },
    });
    const missingCap = await SELF.fetch(`https://cipher.test/v1/blob/${LIVE_ID}`, {
      headers: { "cf-connecting-ip": "198.51.100.118" },
    });
    const wrongCap = await SELF.fetch(`https://cipher.test/v1/blob/${LIVE_ID}`, {
      headers: {
        "cf-connecting-ip": "198.51.100.119",
        "x-osl-fetch-token": WRONG_FETCH_CAP,
      },
    });

    const decoy = await fingerprint(decoyMiss);
    expect(decoy.status).toBe(404);
    expect(await fingerprint(missingCap)).toEqual(decoy);
    expect(await fingerprint(wrongCap)).toEqual(decoy);
  });
});
