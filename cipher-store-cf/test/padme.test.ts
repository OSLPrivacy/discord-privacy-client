import { describe, expect, it } from "vitest";
import { handleUpload } from "../src/endpoints/blob.js";
import { isPadmeLength, padme } from "../src/lib/padme.js";
import type { Env } from "../src/env.js";

const DIGEST = "0123456789abcdef".repeat(4);
const BLOB_ID = "0123456789abcdef".repeat(2);
const DELIVERY_TAG = "fedcba9876543210".repeat(2);

function uploadHeaders(): HeadersInit {
  return {
    "x-osl-ttl-seconds": "3600",
    "x-osl-blob-id": BLOB_ID,
    "x-osl-fetch-digest": DIGEST,
    "x-osl-ack-digest": DIGEST,
    "x-osl-manage-digest": DIGEST,
    "x-osl-delivery-tag": DELIVERY_TAG,
    "x-osl-object-class": "single-ack",
  };
}

describe("Padmé blob lengths", () => {
  it.each([
    [1, 1],
    [2, 2],
    [100, 104],
    [200, 208],
    [1_000, 1_024],
    [65_536, 65_536],
  ])("matches the reference length %i → %i", (length, expected) => {
    expect(padme(length)).toBe(expected);
    expect(isPadmeLength(expected)).toBe(true);
  });

  it("never incurs more than 12% overhead", () => {
    for (let length = 1; length <= 65_536; length += 1) {
      expect(padme(length) / length).toBeLessThanOrEqual(1.12);
    }
  });

  it("refuses an unpadded upload before storage is touched", async () => {
    const env = {
      get DB(): never {
        throw new Error("storage must not be touched for invalid padding");
      },
      get PAYLOADS(): never {
        throw new Error("storage must not be touched for invalid padding");
      },
    } as Env;
    const request = new Request("https://cipher.test/v1/blob", {
      method: "PUT",
      headers: uploadHeaders(),
      body: new Uint8Array(1_001),
    });

    const response = await handleUpload(request, env);

    expect(response.status).toBe(400);
    await expect(response.json()).resolves.toMatchObject({ error: "invalid_padding" });
  });
});
