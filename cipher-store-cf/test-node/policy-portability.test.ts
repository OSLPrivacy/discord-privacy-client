import { describe, expect, it, vi } from "vitest";

import { handleUpload } from "../src/endpoints/blob.js";
import { MAX_LIVE_BLOB_BYTES, MAX_LIVE_BLOB_ROWS, isPadmeLength } from "../src/lib/blob-limits.js";
import { constantTimeEqualHex, sha256Hex } from "../src/lib/digest.js";

const BLOB_ID = "a".repeat(32);
const DIGEST = "b".repeat(64);

function uploadRequest(ttl: string, includeMetadata = true): Request {
  return new Request("https://cipher.example/v1/blobs", {
    method: "POST",
    headers: {
      "content-length": "1",
      "x-osl-ttl-seconds": ttl,
      ...(includeMetadata ? {
        "x-osl-blob-id": BLOB_ID,
        "x-osl-fetch-digest": DIGEST,
        "x-osl-ack-digest": DIGEST,
        "x-osl-manage-digest": DIGEST,
        "x-osl-delivery-tag": BLOB_ID,
        "x-osl-object-class": "single-ack",
      } : {}),
    },
    body: new Uint8Array([1]),
  });
}

describe("payload policy portability", () => {
  it("runs Padmé, TTL, capability, and quota policy under plain Node", async () => {
    expect(isPadmeLength(1_024)).toBe(true);
    expect(isPadmeLength(1_001)).toBe(false);

    for (const ttl of ["3600", "86400", "259200", "604800"]) {
      const response = await handleUpload(uploadRequest(ttl, false), undefined as never);
      expect(response.status).toBe(400);
      await expect(response.json()).resolves.toMatchObject({ error: "bad_blob_metadata" });
    }
    const unsupportedTtl = await handleUpload(uploadRequest("3601"), undefined as never);
    expect(unsupportedTtl.status).toBe(400);
    await expect(unsupportedTtl.json()).resolves.toMatchObject({ error: "bad_ttl" });

    const digest = await sha256Hex("capability");
    expect(constantTimeEqualHex(digest, digest)).toBe(true);
    expect(constantTimeEqualHex(digest, DIGEST)).toBe(false);

    const put = vi.fn();
    const env = {
      DB: {
        prepare: vi.fn()
          .mockReturnValueOnce({ bind: () => ({ first: async () => null }) })
          .mockReturnValueOnce({ first: async () => ({ rows: MAX_LIVE_BLOB_ROWS, bytes: MAX_LIVE_BLOB_BYTES }) }),
      },
      PAYLOADS: { put },
    };
    const quotaExceeded = await handleUpload(uploadRequest("3600"), env as never);
    expect(quotaExceeded.status).toBe(503);
    await expect(quotaExceeded.json()).resolves.toMatchObject({ error: "storage_capacity" });
    expect(put).not.toHaveBeenCalled();
  });
});
