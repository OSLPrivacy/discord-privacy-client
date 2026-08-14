/// D81 — fetch authority is a header-only bearer capability, not identity.
///
/// Migration 0014 retired legacy D1 payloads. These checks therefore pin the
/// current boundary: R2 ciphertext plus a capability-index row, while any
/// leftover row in the retired `blobs` table remains unreachable.
import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { handleUpload } from "../src/endpoints/blob.js";
import { d1All, d1Count, d1First, d1Run, workerEnv } from "./helpers/workerd.js";
import { blobCapabilities, blobFetchHeaders, blobUploadHeaders } from "./helpers/blob.js";

const TOKEN = "0123456789abcdef0123456789abcdef";

function blobId(index: number): string {
  return index.toString(16).padStart(32, "0");
}

async function insertRetiredLegacyRow(index: number): Promise<string> {
  const id = blobId(index);
  const now = Math.floor(Date.now() / 1000);
  await d1Run(
    "INSERT INTO blobs (id, size_bytes, expires_at, created_at) VALUES (?, ?, ?, ?)",
    new Uint8Array([...id.matchAll(/../g)].map(([hex]) => Number.parseInt(hex, 16))),
    17,
    now + 3600,
    now,
  );
  return id;
}

async function upload(body: Uint8Array, id = blobId(0xd8100000)): Promise<string> {
  const caps = blobCapabilities(id);
  // Pin a known fetch bearer rather than deriving one from the id.
  caps.fetchCap = TOKEN;
  const response = await handleUpload(new Request("https://cipher.test/v1/blob", {
    method: "PUT",
    body,
    headers: await blobUploadHeaders(caps),
  }), workerEnv());
  expect(response.status).toBe(201);
  return id;
}

function fetch(id: string, headers: Record<string, string>) {
  return SELF.fetch(`https://cipher.test/v1/blob/${id}`, { headers });
}

describe("D81 — a cipher-store fetch carries no identity", () => {
  it("serves a blob to a caller who presents the capability and nothing else", async () => {
    const id = await upload(new TextEncoder().encode("hello-d81!"));
    const response = await fetch(id, {
      ...blobFetchHeaders(TOKEN),
      "cf-connecting-ip": "198.51.100.99",
    });
    expect(response.status).toBe(200);
    expect(new TextDecoder().decode(await response.arrayBuffer())).toBe("hello-d81!");
  });

  it("writes no blob access receipt or fetcher identity", async () => {
    const id = await upload(new Uint8Array([7]));
    const before = await d1First<Record<string, unknown>>(
      `SELECT size_bytes, expires_at, created_at, fetch_digest_sha256_hex,
              ack_digest_sha256_hex, manage_digest_sha256_hex
         FROM blob_capability_index WHERE blob_id = ?`,
      id,
    );
    const rowsBefore = await d1Count("SELECT COUNT(*) FROM blob_capability_index");
    const countersBefore = await d1Count("SELECT COUNT(*) FROM rate_counters");

    for (let i = 0; i < 10; i++) {
      const response = await fetch(id, {
        ...blobFetchHeaders(TOKEN),
        "cf-connecting-ip": `198.51.100.${i + 1}`,
      });
      expect(response.status).toBe(200);
    }

    expect(await d1First<Record<string, unknown>>(
      `SELECT size_bytes, expires_at, created_at, fetch_digest_sha256_hex,
              ack_digest_sha256_hex, manage_digest_sha256_hex
         FROM blob_capability_index WHERE blob_id = ?`,
      id,
    )).toEqual(before);
    expect(await d1Count("SELECT COUNT(*) FROM blob_capability_index")).toBe(rowsBefore);
    expect(await d1Count("SELECT COUNT(*) FROM rate_counters")).toBe(countersBefore + 10);
    const rateCounters = JSON.stringify(await d1All<Record<string, unknown>>("SELECT * FROM rate_counters"));
    expect(rateCounters).not.toContain("198.51.100.");
    expect(rateCounters).not.toContain(id);
  });

  it("refuses a capability presented in the URL instead of the header", async () => {
    const id = await upload(new Uint8Array([7]));
    const viaQuery = await SELF.fetch(
      `https://cipher.test/v1/blob/${id}?t=${TOKEN}&fetch_token=${TOKEN}&cap=${TOKEN}`,
      { headers: { "cf-connecting-ip": "198.51.100.50" } },
    );
    expect(viaQuery.status).toBe(404);
    const viaPath = await SELF.fetch(`https://cipher.test/v1/blob/${id}/${TOKEN}`, {
      headers: { "cf-connecting-ip": "198.51.100.51" },
    });
    expect(viaPath.status).toBe(404);
  });
});

describe("D81 — a retired legacy row is treated as absent", () => {
  it("does not serve a retired row to a caller holding only its id", async () => {
    const id = await insertRetiredLegacyRow(1);
    expect((await fetch(id, { "cf-connecting-ip": "198.51.100.60" })).status).toBe(404);
    expect((await fetch(id, {
      ...blobFetchHeaders(TOKEN), "cf-connecting-ip": "198.51.100.61",
    })).status).toBe(404);
  });

  it("answers a retired row exactly as it answers an id that was never stored", async () => {
    const id = await insertRetiredLegacyRow(2);
    const legacy = await fetch(id, { ...blobFetchHeaders(TOKEN), "cf-connecting-ip": "198.51.100.62" });
    const absent = await fetch(blobId(9999), { ...blobFetchHeaders(TOKEN), "cf-connecting-ip": "198.51.100.63" });
    expect(legacy.status).toBe(absent.status);
    expect(await legacy.text()).toBe(await absent.text());
  });

  it("does not let an id-only caller destroy a retired row", async () => {
    const id = await insertRetiredLegacyRow(3);
    expect(await d1Count("SELECT COUNT(*) FROM blobs WHERE id = ?", new Uint8Array([...id.matchAll(/../g)].map(([hex]) => Number.parseInt(hex, 16))))).toBe(1);
    const response = await SELF.fetch(`https://cipher.test/v1/blob/${id}`, {
      method: "DELETE", headers: { "cf-connecting-ip": "198.51.100.64" },
    });
    expect(response.status).toBe(204);
    expect(await d1Count("SELECT COUNT(*) FROM blobs WHERE id = ?", new Uint8Array([...id.matchAll(/../g)].map(([hex]) => Number.parseInt(hex, 16))))).toBe(1);
  });

  it("rejects a tokenless upload, so no new capability-less index row can be created", async () => {
    const response = await handleUpload(new Request("https://cipher.test/v1/blob", {
      method: "PUT", body: new Uint8Array([7]), headers: { "x-osl-ttl-seconds": "604800" },
    }), workerEnv());
    expect(response.status).toBe(400);
    expect(await d1Count("SELECT COUNT(*) FROM blob_capability_index")).toBe(0);
  });
});
