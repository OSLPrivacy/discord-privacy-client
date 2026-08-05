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

    // This loop used to run all four allowlisted TTLs through a
    // metadata-less request and expect `bad_blob_metadata` from each. That was
    // the pre-floor contract: commit c7eb15c42 ("T6-W7 enforce seven-day
    // default TTL floor", Aug 2) landed DEFAULT_DELIVERY_TTL_FLOOR the day
    // AFTER this gate was written (48425edd8, Aug 1) and did not update it, so
    // 3600/86400/259200 now stop at `bad_ttl` in handleUpload before the
    // metadata check is reached. Nobody saw it because this suite is the
    // second half of `npm test` and the cipher-store lane never ran to it.
    //
    // The code is right and the assertion was stale. Split into the two claims
    // handleUpload actually makes, so both are covered rather than one being
    // asserted wrongly:
    //   1. a default-mode-valid TTL with no metadata -> bad_blob_metadata
    //   2. a TTL below the seven-day default floor -> bad_ttl, metadata or not
    const missingMetadata = await handleUpload(uploadRequest("604800", false), undefined as never);
    expect(missingMetadata.status).toBe(400);
    await expect(missingMetadata.json()).resolves.toMatchObject({ error: "bad_blob_metadata" });

    for (const belowFloor of ["3600", "86400", "259200"]) {
      const response = await handleUpload(uploadRequest(belowFloor), undefined as never);
      expect(response.status).toBe(400);
      await expect(response.json()).resolves.toMatchObject({ error: "bad_ttl" });
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
    // Same stale TTL as the loop above, and this one matters more: with "3600"
    // handleUpload refused at `bad_ttl` and returned 400 long before it counted
    // capacity, so the storage-capacity refusal -- and `put` never being called
    // once the pool is full -- has been asserted against a request that never
    // reached either check since the floor landed.
    const quotaExceeded = await handleUpload(uploadRequest("604800"), env as never);
    expect(quotaExceeded.status).toBe(503);
    await expect(quotaExceeded.json()).resolves.toMatchObject({ error: "storage_capacity" });
    expect(put).not.toHaveBeenCalled();
  });
});
