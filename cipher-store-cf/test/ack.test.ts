import { env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import worker from "../src/index.js";
import { handleUpload } from "../src/endpoints/blob.js";
import { handleAck } from "../src/endpoints/receipt.js";
import { sha256Hex } from "../src/lib/digest.js";
import { workerEnv } from "./helpers/workerd.js";

const ORIGIN = "https://cipher.test";

async function upload(id: string, objectClass: "single-ack" | "multi-fetch") {
  const fetchCap = "2".repeat(31) + id[0]!;
  const ackCap = "3".repeat(31) + id[0]!;
  const response = await handleUpload(new Request(`${ORIGIN}/v1/blob`, {
    method: "POST",
    headers: {
      "x-osl-ttl-seconds": "3600",
      "x-osl-blob-id": id,
      "x-osl-fetch-digest": await sha256Hex(fetchCap),
      "x-osl-ack-digest": await sha256Hex(ackCap),
      "x-osl-manage-digest": await sha256Hex("4".repeat(31) + id[0]!),
      "x-osl-object-class": objectClass,
      "x-osl-delivery-tag": "5".repeat(31) + id[0]!,
    },
    body: new Uint8Array([7, 8, 9]),
  }), workerEnv());
  expect(response.status).toBe(201);
  return { fetchCap, ackCap, objectKey: await sha256Hex(fetchCap) };
}

function ackRequest(id: string, ackCap?: string) {
  return new Request(`${ORIGIN}/v1/blob/${id}/ack`, {
    method: "POST",
    headers: ackCap ? { "x-osl-ack-cap": ackCap } : undefined,
  });
}

describe("blob durable receipts", () => {
  it("rejects absent or wrong acknowledgement capability without destroying a single-ack payload", async () => {
    const id = "a".repeat(32);
    const stored = await upload(id, "single-ack");

    const absent = await handleAck(ackRequest(id), workerEnv(), id);
    const wrong = await handleAck(ackRequest(id, "f".repeat(32)), workerEnv(), id);

    expect([absent.status, wrong.status]).toEqual([404, 404]);
    expect(await absent.text()).toBe(await wrong.text());
    await expect(env.PAYLOADS.head(stored.objectKey)).resolves.not.toBeNull();
    await expect(env.DB.prepare("SELECT 1 FROM blob_capability_index WHERE blob_id = ?").bind(id).first()).resolves.not.toBeNull();
  });

  it("destroys a single-ack payload and index row, and permits a repeat acknowledgement", async () => {
    const id = "b".repeat(32);
    const stored = await upload(id, "single-ack");

    expect((await handleAck(ackRequest(id, stored.ackCap), workerEnv(), id)).status).toBe(204);
    await expect(env.PAYLOADS.head(stored.objectKey)).resolves.toBeNull();
    await expect(env.DB.prepare("SELECT 1 FROM blob_capability_index WHERE blob_id = ?").bind(id).first()).resolves.toBeNull();
    expect((await handleAck(ackRequest(id, stored.ackCap), workerEnv(), id)).status).toBe(204);
  });

  it("accepts a multi-fetch acknowledgement without deleting its payload", async () => {
    const id = "c".repeat(32);
    const stored = await upload(id, "multi-fetch");

    expect((await handleAck(ackRequest(id, stored.ackCap), workerEnv(), id)).status).toBe(204);
    await expect(env.PAYLOADS.head(stored.objectKey)).resolves.not.toBeNull();
    await expect(env.DB.prepare("SELECT 1 FROM blob_capability_index WHERE blob_id = ?").bind(id).first()).resolves.not.toBeNull();
  });

  it("does not expose a blob status route", async () => {
    const response = await worker.fetch(
      new Request(`${ORIGIN}/v1/blob/${"d".repeat(32)}/status`),
      workerEnv(),
      {} as ExecutionContext,
    );
    expect(response.status).toBe(404);
  });
});
