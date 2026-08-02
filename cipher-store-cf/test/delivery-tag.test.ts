import { describe, expect, it } from "vitest";
import { handleUpload } from "../src/endpoints/blob.js";
import { handleAck } from "../src/endpoints/receipt.js";
import { findDeliveryMatches } from "../src/lib/delivery-tag.js";
import { sha256Hex } from "../src/lib/digest.js";
import { workerEnv } from "./helpers/workerd.js";

const tag = "a".repeat(32);
const blobId = "b".repeat(32);
const fetchCap = "c".repeat(32);
const ackCap = "d".repeat(32);

async function upload(): Promise<void> {
  const response = await handleUpload(new Request("https://cipher.test/v1/blob", {
    method: "PUT",
    headers: {
      "x-osl-ttl-seconds": "604800", "x-osl-blob-id": blobId,
      "x-osl-fetch-digest": await sha256Hex(fetchCap),
      "x-osl-ack-digest": await sha256Hex(ackCap),
      "x-osl-manage-digest": await sha256Hex("e".repeat(32)),
      "x-osl-delivery-tag": tag, "x-osl-object-class": "single-ack",
    },
    body: new Uint8Array([1]),
  }), workerEnv());
  expect(response.status).toBe(201);
}

describe("opaque delivery-tag index", () => {
  it("returns only a matching tag and blob pointer", async () => {
    await upload();
    const matches = await findDeliveryMatches(workerEnv(), [tag, "f".repeat(32)]);
    expect(matches).toEqual([{ tag, blob_id: blobId }]);
    expect(Object.keys(matches[0]!)).toEqual(["tag", "blob_id"]);
    expect(JSON.stringify(matches)).not.toMatch(/size|sender|recipient|count/i);
  });

  it("keeps rotating tags structurally uncorrelated and removes the index row on ack", async () => {
    await upload();
    const stored = await workerEnv().DB.prepare(
      "SELECT delivery_tag, blob_id, fetch_digest_sha256_hex, ack_digest_sha256_hex, manage_digest_sha256_hex FROM blob_capability_index",
    ).all<Record<string, string>>();
    expect(Object.keys(stored.results?.[0] ?? {}).sort()).toEqual([
      "ack_digest_sha256_hex", "blob_id", "delivery_tag", "fetch_digest_sha256_hex", "manage_digest_sha256_hex",
    ]);
    expect((await handleAck(new Request("https://cipher.test/v1/blob/" + blobId, {
      method: "POST", headers: { "x-osl-ack-cap": ackCap },
    }), workerEnv(), blobId)).status).toBe(204);
    await expect(findDeliveryMatches(workerEnv(), [tag])).resolves.toEqual([]);
  });
});
