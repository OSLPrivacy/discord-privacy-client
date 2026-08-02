import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { sha256Hex } from "../src/lib/digest.js";

const ORIGIN = "https://cipher.test";
const TTL = "604800";

async function upload(id: string) {
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
  expect(response.status).toBe(201);
  return { fetchCap, ackCap, manageCap };
}

describe("receipt-aware blob routes and health capability", () => {
  it("wires the frozen PUT, GET, ACK, and DELETE authorities without a status route", async () => {
    const fetchId = "a".repeat(32);
    const fetch = await upload(fetchId);
    expect((await SELF.fetch(`${ORIGIN}/v1/blob`, { method: "POST" })).status).toBe(404);
    expect((await SELF.fetch(`${ORIGIN}/v1/blob/${fetchId}`, {
      headers: { "x-osl-fetch-cap": "0".repeat(32) },
    })).status).toBe(404);
    expect((await SELF.fetch(`${ORIGIN}/v1/blob/${fetchId}`, {
      headers: { "x-osl-fetch-cap": fetch.fetchCap },
    })).status).toBe(200);

    const ackId = "b".repeat(32);
    const ack = await upload(ackId);
    expect((await SELF.fetch(`${ORIGIN}/v1/blob/${ackId}/ack`, {
      method: "POST",
      headers: { "x-osl-ack-cap": "0".repeat(32) },
    })).status).toBe(404);
    expect((await SELF.fetch(`${ORIGIN}/v1/blob/${ackId}/ack`, {
      method: "POST",
      headers: { "x-osl-ack-cap": ack.ackCap },
    })).status).toBe(204);

    const burnId = "c".repeat(32);
    const burn = await upload(burnId);
    expect((await SELF.fetch(`${ORIGIN}/v1/blob/${burnId}`, {
      method: "DELETE",
      headers: { "x-osl-manage-cap": burn.manageCap },
    })).status).toBe(204);
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
