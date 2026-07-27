import { describe, expect, it } from "vitest";
import type { Env } from "../src/env.js";
import {
  handleUpload,
  MAX_BLOB_BYTES,
  readBoundedBody,
} from "../src/endpoints/blob.js";
import { BLOB_SWEEP_BATCH_SIZE, sweepExpired } from "../src/lib/sweep.js";
import { d1Count, d1Run, workerEnv } from "./helpers/workerd.js";

function writableEnv(): Env {
  return workerEnv();
}

function blobId(index: number): Uint8Array {
  const id = new Uint8Array(8);
  new DataView(id.buffer).setBigUint64(0, BigInt(index));
  return id;
}

describe("cipher upload body bounds", () => {
  it("accepts exactly the maximum streamed byte count", async () => {
    const request = new Request("https://cipher.test/v1/blob", {
      method: "POST",
      body: new Uint8Array(MAX_BLOB_BYTES),
    });
    const result = await readBoundedBody(request, MAX_BLOB_BYTES);
    expect(result.status).toBe("ok");
    if (result.status === "ok") {
      expect(result.bytes.byteLength).toBe(MAX_BLOB_BYTES);
    }
  });

  it("stops a streamed body as soon as it exceeds the cap", async () => {
    const request = new Request("https://cipher.test/v1/blob", {
      method: "POST",
      body: new Uint8Array(MAX_BLOB_BYTES + 1),
    });
    await expect(readBoundedBody(request, MAX_BLOB_BYTES)).resolves.toEqual({
      status: "too_large",
    });
  });

  it("rejects an oversized Content-Length before touching storage", async () => {
    const request = new Request("https://cipher.test/v1/blob", {
      method: "POST",
      headers: { "content-length": String(MAX_BLOB_BYTES + 1) },
      body: new Uint8Array([1]),
    });
    const env = {
      get DB(): never {
        throw new Error("storage must not be touched");
      },
    } as unknown as Env;
    const response = await handleUpload(request, env);
    expect(response.status).toBe(413);
    expect(await response.json()).toEqual({
      error: "too_large",
      message: `blob exceeds ${MAX_BLOB_BYTES} bytes`,
    });
  });

  it("rejects a malformed Content-Length before reading the body", async () => {
    const request = new Request("https://cipher.test/v1/blob", {
      method: "POST",
      headers: { "content-length": "1junk" },
      body: new Uint8Array([1]),
    });
    const response = await handleUpload(request, {} as Env);
    expect(response.status).toBe(400);
  });

  it.each([
    ["3600", 3600],
    ["86400", 86400],
    ["259200", 259200],
    ["604800", 604800],
  ])("accepts exact allowlisted TTL %s", async (ttlHeader, ttlSeconds) => {
    const before = Math.floor(Date.now() / 1000);
    const request = new Request("https://cipher.test/v1/blob", {
      method: "POST",
      headers: {
        "x-osl-ttl-seconds": ttlHeader,
        "x-osl-fetch-token": "0123456789abcdef0123456789abcdef",
      },
      body: new Uint8Array([1]),
    });

    const response = await handleUpload(request, writableEnv());
    const after = Math.floor(Date.now() / 1000);
    expect(response.status).toBe(201);
    const payload = await response.json() as { expires_at: number };
    expect(payload.expires_at).toBeGreaterThanOrEqual(before + ttlSeconds);
    expect(payload.expires_at).toBeLessThanOrEqual(after + ttlSeconds);
  });

  it.each(["3599", "3601", "03600", "+3600", "3600.0", "3600junk"])(
    "rejects non-allowlisted TTL %s before touching storage",
    async (ttlHeader) => {
      const request = new Request("https://cipher.test/v1/blob", {
        method: "POST",
        headers: { "x-osl-ttl-seconds": ttlHeader },
        body: new Uint8Array([1]),
      });
      const env = {
        get DB(): never {
          throw new Error("storage must not be touched");
        },
      } as unknown as Env;

      const response = await handleUpload(request, env);
      expect(response.status).toBe(400);
      expect(await response.json()).toEqual({
        error: "bad_ttl",
        message: "X-OSL-TTL-Seconds must be 3600 (1h), 86400 (24h), 259200 (72h), or 604800 (7d)",
      });
    },
  );

  it("reclaims expired blobs in bounded batches while preserving live blobs", async () => {
    const now = Math.floor(Date.now() / 1000);
    const expiredRows = BLOB_SWEEP_BATCH_SIZE + 1;
    for (let index = 0; index < expiredRows; index++) {
      await d1Run(
        "INSERT INTO blobs (id, data, size_bytes, expires_at, created_at, fetch_token) VALUES (?, ?, ?, ?, ?, ?)",
        blobId(index),
        new Uint8Array([index % 256]),
        1,
        now - 60,
        now - 120,
        null,
      );
    }
    await d1Run(
      "INSERT INTO blobs (id, data, size_bytes, expires_at, created_at, fetch_token) VALUES (?, ?, ?, ?, ?, ?)",
      blobId(expiredRows),
      new Uint8Array([255]),
      1,
      now + 60,
      now - 120,
      null,
    );

    await expect(sweepExpired(writableEnv())).resolves.toBe(expiredRows);
    expect(await d1Count("SELECT COUNT(*) AS c FROM blobs WHERE expires_at < ?", now)).toBe(0);
    expect(await d1Count("SELECT COUNT(*) AS c FROM blobs WHERE expires_at >= ?", now)).toBe(1);
  });
});
