/// HIGH-2 regression suite — "KV read/modify/write does not enforce mutation
/// rate limits" (docs/security/osl-audit-2026-07-26-codex.md).
///
/// The old limiter did `KV.get` → compare → `KV.put(used + 1)` across two
/// awaits. Concurrent invocations all observe the same value and collapse to a
/// single increment, so the ceiling the comment claims is not enforced even
/// within one POP. The `racingKv` below models exactly that: reads settle
/// before any write becomes visible.

import { describe, expect, it, vi } from "vitest";
import type { Env } from "../src/env.js";
import { handleUpload } from "../src/endpoints/blob.js";
import { sha256Hex } from "../src/lib/digest.js";
import { rateLimit } from "../src/lib/rate-limit.js";
import {
  d1All,
  d1Run,
  workerEnv,
} from "./helpers/workerd.js";

const SECRET = "s".repeat(48);
/// Mirrors the `delete` budget in `src/lib/rate-limit.ts`.
const DELETE_BUDGET = 600;
/// Aggregate ceiling for the generic blob table.
const MAX_LIVE_BLOB_BYTES = 2 * 1024 * 1024 * 1024;

function harness() {
  return { env: workerEnv({ RATE_LIMIT_HASH_KEY: SECRET }) };
}

async function blobRequest(bytes: Uint8Array): Promise<Request> {
  const id = "1".repeat(32);
  const fetchCap = "2".repeat(32);
  const ackCap = "3".repeat(32);
  const manageCap = "4".repeat(32);
  return new Request("https://cipher.test/v1/blob", {
    method: "POST",
    headers: {
      "x-osl-ttl-seconds": "3600",
      "x-osl-expiry-mode": "absolute",
      "x-osl-blob-id": id,
      "x-osl-fetch-digest": await sha256Hex(fetchCap),
      "x-osl-ack-digest": await sha256Hex(ackCap),
      "x-osl-manage-digest": await sha256Hex(manageCap),
      "x-osl-delivery-tag": "5".repeat(32),
      "x-osl-object-class": "single-ack",
      "content-length": String(bytes.byteLength),
    },
    body: bytes,
  });
}

describe("mutation rate limiting is atomic (HIGH-2)", () => {
  it("admits no more than the budget when requests race", async () => {
    const { env } = harness();
    const attempts = DELETE_BUDGET + 100;

    const results = await Promise.all(
      Array.from({ length: attempts }, () => rateLimit(env, "203.0.113.9", "delete")),
    );
    const admitted = results.filter((result) => result.allowed).length;

    // Before the fix every one of these is admitted: they all read `used = 0`.
    expect(admitted).toBe(DELETE_BUDGET);
    // DELETE_BUDGET + 100 limiter round trips fired at once is inherently one
    // of the heaviest specs here, and on CI it lands at 15722ms / 18319ms /
    // 20825ms across runs against a 20s budget -- it has been failing on
    // variance alone, not on a hang, and the admitted-count assertion above is
    // untouched. 60s is ~3x the worst observed time and still fails a genuine
    // hang promptly.
  }, 60_000);

  it("keeps a separate, much smaller budget for multipart session creation", async () => {
    const { env } = harness();

    const bulk = await Promise.all(
      Array.from({ length: 200 }, () => rateLimit(env, "203.0.113.10", "attachment-session")),
    );
    const admittedSessions = bulk.filter((result) => result.allowed).length;
    expect(admittedSessions).toBeLessThanOrEqual(24);

    // Exhausting session creation must not exhaust the part-upload budget the
    // caller needs to finish an upload it already started.
    const part = await rateLimit(env, "203.0.113.10", "attachment-upload");
    expect(part.allowed).toBe(true);
    // Same shape, 200 concurrent round trips, and it was inheriting vitest's
    // 5s default: it measured 4730ms on the run that passed and timed out at
    // exactly 5000ms on the run that did not. Give it a budget of its own
    // rather than leave it one bad scheduling slice from red.
  }, 30_000);

  it("fails closed for anonymous writes and every ciphertext read when the limiter is down", async () => {
    const broken = {
      DB: {
        prepare: () => ({
          bind: () => ({
            run: async () => {
              throw new Error("d1 down");
            },
            first: async () => {
              throw new Error("d1 down");
            },
          }),
        }),
      } as unknown as D1Database,
      ATTACHMENTS: {} as R2Bucket,
      RATE_LIMIT: { get: vi.fn().mockRejectedValue(new Error("kv down")) } as unknown as KVNamespace,
      RATE_LIMIT_HASH_KEY: SECRET,
    } as Env;

    await expect(rateLimit(broken, "203.0.113.11", "upload")).resolves.toMatchObject({
      allowed: false,
    });
    await expect(rateLimit(broken, "203.0.113.11", "attachment-session")).resolves.toMatchObject({
      allowed: false,
    });
    await expect(rateLimit(broken, "203.0.113.11", "fetch")).resolves.toMatchObject({
      allowed: false,
    });
  });

  it("never records a raw client address in the limiter's durable state", async () => {
    const { env } = harness();
    await rateLimit(env, "203.0.113.77", "upload");
    const dump = JSON.stringify(await d1All<Record<string, unknown>>("SELECT * FROM rate_counters"));
    expect(dump).not.toContain("203.0.113.77");
  });
});

describe("generic blob storage has a database-level backstop (HIGH-2)", () => {
  it("refuses new blobs once the aggregate byte budget is reached", async () => {
    const { env } = harness();
    const now = Math.floor(Date.now() / 1000);

    await d1Run(
      `INSERT INTO blob_capability_index (
        blob_id, fetch_digest_sha256_hex, ack_digest_sha256_hex,
        manage_digest_sha256_hex, object_class, pool, delivery_tag,
        size_bytes, expires_at, created_at
      ) VALUES (?, ?, ?, ?, 'single-ack', 'undelivered', ?, ?, ?, ?)`,
      "a".repeat(32),
      "b".repeat(64),
      "c".repeat(64),
      "d".repeat(64),
      "e".repeat(32),
      MAX_LIVE_BLOB_BYTES,
      now + 3600,
      now,
    );

    // Before the fix `handleUpload` has no aggregate predicate at all, so this
    // is a 201 and the table grows without bound.
    const response = await handleUpload(await blobRequest(new Uint8Array([9])), env);
    expect(response.status).toBe(503);
    expect(await response.json()).toMatchObject({ error: "storage_capacity" });
  });

  it("still accepts ordinary blobs well under the budget", async () => {
    const { env } = harness();
    const response = await handleUpload(await blobRequest(new Uint8Array([1])), env);
    expect(response.status).toBe(201);
  });
});
