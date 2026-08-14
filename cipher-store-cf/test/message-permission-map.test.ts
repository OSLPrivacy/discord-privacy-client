import { env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import worker from "../src/index.js";
import { handleUpload } from "../src/endpoints/blob.js";
import { sha256Hex } from "../src/lib/digest.js";
import { sweepExpired } from "../src/lib/sweep.js";
import { blobCapabilities, blobUploadHeaders, DEFAULT_BLOB_BYTES } from "./helpers/blob.js";
import { d1Count, d1Run, workerEnv } from "./helpers/workerd.js";

declare const process: { stdout?: { write: (chunk: string) => void } };

const ORIGIN = "https://cipher.test";

async function uploadBlob(
  id: string,
  objectClass: "single-ack" | "multi-fetch" = "single-ack",
) {
  const caps = blobCapabilities(id);
  const response = await handleUpload(new Request(`${ORIGIN}/v1/blob`, {
    method: "POST",
    headers: await blobUploadHeaders(caps, objectClass),
    body: DEFAULT_BLOB_BYTES,
  }), workerEnv());
  expect(response.status).toBe(201);
  return { ...caps, objectKey: await sha256Hex(caps.fetchCap) };
}

async function readStatus(id: string, fetchCap: string): Promise<number> {
  return (await worker.fetch(
    new Request(`${ORIGIN}/v1/blob/${id}`, {
      headers: { "x-osl-fetch-cap": fetchCap },
    }),
    workerEnv(),
    {} as ExecutionContext,
  )).status;
}

describe("TASK 0400 message read/delete permission map", () => {
  it("prints the current accepted secret for read, sender delete, recipient delete, and server delete", async () => {
    const read = await uploadBlob("1".repeat(32));
    const readOk = await worker.fetch(
      new Request(`${ORIGIN}/v1/blob/${read.id}`, {
        headers: { "x-osl-fetch-cap": read.fetchCap },
      }),
      workerEnv(),
      {} as ExecutionContext,
    );
    expect(readOk.status).toBe(200);
    expect(new Uint8Array(await readOk.arrayBuffer())).toEqual(DEFAULT_BLOB_BYTES);
    expect(await readStatus(read.id, read.manageCap)).toBe(404);
    expect(await readStatus(read.id, read.ackCap)).toBe(404);

    const senderDelete = await uploadBlob("2".repeat(32));
    const wrongSenderDelete = await worker.fetch(
      new Request(`${ORIGIN}/v1/blob/${senderDelete.id}`, {
        method: "DELETE",
        headers: { "x-osl-fetch-cap": senderDelete.fetchCap },
      }),
      workerEnv(),
      {} as ExecutionContext,
    );
    expect(wrongSenderDelete.status).toBe(204);
    expect(await readStatus(senderDelete.id, senderDelete.fetchCap)).toBe(200);
    const senderDeleteOk = await worker.fetch(
      new Request(`${ORIGIN}/v1/blob/${senderDelete.id}`, {
        method: "DELETE",
        headers: { "x-osl-manage-cap": senderDelete.manageCap },
      }),
      workerEnv(),
      {} as ExecutionContext,
    );
    expect(senderDeleteOk.status).toBe(204);
    expect(await readStatus(senderDelete.id, senderDelete.fetchCap)).toBe(404);

    const recipientDelete = await uploadBlob("3".repeat(32), "single-ack");
    const wrongRecipientDelete = await worker.fetch(
      new Request(`${ORIGIN}/v1/blob/${recipientDelete.id}/ack`, {
        method: "POST",
        headers: { "x-osl-manage-cap": recipientDelete.manageCap },
      }),
      workerEnv(),
      {} as ExecutionContext,
    );
    expect(wrongRecipientDelete.status).toBe(404);
    expect(await readStatus(recipientDelete.id, recipientDelete.fetchCap)).toBe(200);
    const recipientDeleteOk = await worker.fetch(
      new Request(`${ORIGIN}/v1/blob/${recipientDelete.id}/ack`, {
        method: "POST",
        headers: { "x-osl-ack-cap": recipientDelete.ackCap },
      }),
      workerEnv(),
      {} as ExecutionContext,
    );
    expect(recipientDeleteOk.status).toBe(204);
    expect(await readStatus(recipientDelete.id, recipientDelete.fetchCap)).toBe(404);

    const serverId = "4".repeat(32);
    const serverFetchCap = blobCapabilities(serverId).fetchCap;
    const serverFetchDigest = await sha256Hex(serverFetchCap);
    const now = Math.floor(Date.now() / 1000);
    await env.PAYLOADS.put(serverFetchDigest, DEFAULT_BLOB_BYTES);
    await d1Run(
      `INSERT INTO blob_capability_index (
        blob_id, fetch_digest_sha256_hex, ack_digest_sha256_hex,
        manage_digest_sha256_hex, object_class, pool, delivery_tag,
        size_bytes, expires_at, created_at
      ) VALUES (?, ?, ?, ?, 'single-ack', 'undelivered', ?, ?, ?, ?)`,
      serverId,
      serverFetchDigest,
      await sha256Hex(blobCapabilities(serverId).ackCap),
      await sha256Hex(blobCapabilities(serverId).manageCap),
      blobCapabilities(serverId).deliveryTag,
      DEFAULT_BLOB_BYTES.byteLength,
      now - 1,
      now - 2,
    );
    expect(await sweepExpired(workerEnv())).toBe(1);
    expect(await env.PAYLOADS.head(serverFetchDigest)).toBeNull();
    expect(await d1Count(
      "SELECT COUNT(*) AS c FROM blob_capability_index WHERE blob_id = ?",
      serverId,
    )).toBe(0);

    const checklist = [
      `[x] read input: GET /v1/blob/:id_hex accepts x-osl-fetch-cap=${read.fetchCap}`,
      `[x] sender delete input: DELETE /v1/blob/:id_hex accepts x-osl-manage-cap=${senderDelete.manageCap}`,
      `[x] recipient delete input: POST /v1/blob/:id_hex/ack accepts x-osl-ack-cap=${recipientDelete.ackCap}`,
      "[x] server delete input: scheduled sweep accepts no client secret; input is stored expires_at < now plus fetch_digest_sha256_hex",
    ].join("\n");
    process.stdout?.write(`${checklist}\n`);
  });
});
