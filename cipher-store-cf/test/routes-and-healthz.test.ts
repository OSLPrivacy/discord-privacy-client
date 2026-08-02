import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { sha256Hex } from "../src/lib/digest.js";

const ORIGIN = "https://cipher.test";
const TTL = "604800";

async function attemptedUpload(id: string) {
  const fetchCap = `1${id.slice(1)}`;
  const ackCap = `2${id.slice(1)}`;
  const manageCap = `3${id.slice(1)}`;
  const response = await SELF.fetch(`${ORIGIN}/v1/blob`, {
    method: "PUT",
    headers: {
      "cf-connecting-ip": "198.51.100.10",
      "x-osl-ttl-seconds": TTL,
      "x-osl-blob-id": id,
      "x-osl-fetch-digest": await sha256Hex(fetchCap),
      "x-osl-ack-digest": await sha256Hex(ackCap),
      "x-osl-manage-digest": await sha256Hex(manageCap),
      "x-osl-object-class": "single-ack",
      "x-osl-delivery-tag": "f".repeat(32),
    },
    body: new Uint8Array([7]),
  });
  expect(response.status).toBe(401);
  return { fetchCap, ackCap, manageCap };
}

describe("receipt-aware blob routes and health capability", () => {
  it("makes the real PUT route consume a storage grant before it can create a blob", async () => {
    const fetchId = "a".repeat(32);
    await attemptedUpload(fetchId);
    expect(await env.DB.prepare("SELECT 1 FROM blob_capability_index WHERE blob_id = ?")
      .bind(fetchId).first()).toBeNull();
    expect(await env.PAYLOADS.head(await sha256Hex(`1${fetchId.slice(1)}`))).toBeNull();

    expect((await SELF.fetch(`${ORIGIN}/v1/blob`, { method: "POST" })).status).toBe(404);
    expect((await SELF.fetch(`${ORIGIN}/v1/blob/${fetchId}/status`)).status).toBe(404);
  });

  it("advertises storage_ack_v1 only when the marker schema is present", async () => {
    const current = await SELF.fetch(`${ORIGIN}/v1/healthz`);
    await expect(current.json()).resolves.toMatchObject({
      ok: true,
      capabilities: ["storage_ack_v1"],
    });

    await env.DB.exec(`
      DROP TABLE blob_capability_index;
      DROP TABLE worker_schema_capabilities;
    `);
    const before0011 = await SELF.fetch(`${ORIGIN}/v1/healthz`);
    await expect(before0011.json()).resolves.toMatchObject({
      ok: true,
      capabilities: [],
    });
  });
});
