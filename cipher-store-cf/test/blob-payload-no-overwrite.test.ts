/// D-264 — an upload may not overwrite another blob's payload bytes.
///
/// `putByDigest` is the only write on this Worker whose R2 object key comes
/// from the request (`x-osl-fetch-digest`). Winning the D1 insert proves the
/// caller owns the *id*; it proves nothing about the *digest*, because the two
/// are independent header values. So a caller with a fresh id it genuinely owns
/// could name a victim's fetch digest and replace the victim's ciphertext.
///
/// The sibling attachment path already refuses this shape with
/// `onlyIf: { etagDoesNotMatch: "*" }` on a key the *server* generates. This
/// spec holds the stricter path to at least the weaker one's precondition.

import { env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { handleFetch, handleUpload } from "../src/endpoints/blob.js";
import { sha256Hex } from "../src/lib/digest.js";
import { workerEnv } from "./helpers/workerd.js";

const VICTIM_ID = "a".repeat(32);
const ATTACKER_ID = "b".repeat(32);
const SHARED_FETCH_CAP = "c".repeat(32);

const VICTIM_BYTES = new Uint8Array([1]);
const ATTACKER_BYTES = new Uint8Array([2]);

async function upload(
  blobId: string,
  fetchCap: string,
  body: Uint8Array,
): Promise<Response> {
  return handleUpload(
    new Request("https://cipher.test/v1/blob", {
      method: "PUT",
      headers: {
        "x-osl-ttl-seconds": "3600",
        "x-osl-expiry-mode": "absolute",
        "x-osl-blob-id": blobId,
        "x-osl-fetch-digest": await sha256Hex(fetchCap),
        "x-osl-ack-digest": await sha256Hex(`${blobId}-ack`),
        "x-osl-manage-digest": await sha256Hex(`${blobId}-manage`),
        "x-osl-object-class": "single-ack",
        "x-osl-delivery-tag": blobId,
      },
      body,
    }),
    workerEnv(),
  );
}

describe("D-264 a caller-named payload key is written only when it is free", () => {
  it("refuses to replace an existing payload under a digest the caller supplied", async () => {
    expect((await upload(VICTIM_ID, SHARED_FETCH_CAP, VICTIM_BYTES)).status)
      .toBe(201);
    const objectKey = await sha256Hex(SHARED_FETCH_CAP);
    await expect(env.PAYLOADS.head(objectKey)).resolves.toMatchObject({
      size: VICTIM_BYTES.byteLength,
    });

    // The attacker's id is genuinely unused, so D1 admission cannot be what
    // stops this: the row IS written and the response IS the ordinary 201.
    const attack = await upload(ATTACKER_ID, SHARED_FETCH_CAP, ATTACKER_BYTES);
    expect(attack.status).toBe(201);
    await expect(
      env.DB.prepare(
        "SELECT blob_id FROM blob_capability_index WHERE blob_id = ?",
      ).bind(ATTACKER_ID).first(),
    ).resolves.toEqual({ blob_id: ATTACKER_ID });

    // Assert the EFFECT, not the call: the victim's bytes are still the bytes
    // R2 returns, at the victim's length.
    const stored = await env.PAYLOADS.get(objectKey);
    expect(stored).not.toBeNull();
    expect(new Uint8Array(await stored!.arrayBuffer())).toEqual(VICTIM_BYTES);

    // And through the live fetch path, which is what a reader actually sees.
    const served = await handleFetch(
      new Request(`https://cipher.test/v1/blob/${VICTIM_ID}`, {
        headers: { "x-osl-fetch-cap": SHARED_FETCH_CAP },
      }),
      workerEnv(),
      VICTIM_ID,
    );
    expect(served.status).toBe(200);
    expect(new Uint8Array(await served.arrayBuffer()).subarray(0, 1))
      .toEqual(VICTIM_BYTES);
  });

  it("still stores bytes under a digest no object occupies", async () => {
    // Starves the gate: "nothing was overwritten" must not be reachable by
    // refusing every write. A free key still gets the caller's bytes.
    const freeCap = "d".repeat(32);
    const freeId = "e".repeat(32);
    expect((await upload(freeId, freeCap, ATTACKER_BYTES)).status).toBe(201);
    const stored = await env.PAYLOADS.get(await sha256Hex(freeCap));
    expect(stored).not.toBeNull();
    expect(new Uint8Array(await stored!.arrayBuffer())).toEqual(ATTACKER_BYTES);
  });
});
