/// D-256 — the aggregate blob capacity gate must stay atomic (audit HIGH-2).
///
/// `src/lib/blob-limits.ts` exists because "budget per address" is not a
/// storage bound. The gate that replaced it was one write statement, and the
/// in-code comment said why in as many words: "One SQLite write statement, so
/// the COUNT/SUM predicates cannot race another insert." `cc619a55e` split it
/// into `SELECT COUNT/SUM` followed by an unconditional `INSERT`, which is a
/// textbook TOCTOU: concurrent uploads all read the same pre-insert aggregate,
/// all decide there is room, and all write.
///
/// The limit constants survived that change, so a sequential test still passes.
/// Only a concurrent one can tell the two designs apart, which is exactly what
/// the audit finding was about.
///
/// This runs against real D1 under @cloudflare/vitest-pool-workers, not a
/// hand-rolled double: the interleaving below is D1's own, and
/// `test/d1-meta-changes-contract.test.ts` measures the INSERT..SELECT..WHERE
/// admission shape in this same pool.

import { env } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";
import { handleUpload } from "../src/endpoints/blob.js";
import { MAX_LIVE_BLOB_BYTES } from "../src/lib/blob-limits.js";
import { sha256Hex } from "../src/lib/digest.js";
import { workerEnv } from "./helpers/workerd.js";

/// One byte of headroom: each upload below fits on its own and no two fit
/// together, so admitting both is over-admission and nothing else.
const BODY = new Uint8Array([9]);
const SEEDED_BYTES = MAX_LIVE_BLOB_BYTES - BODY.byteLength;

async function upload(idChar: string): Promise<Request> {
  const id = idChar.repeat(32);
  return new Request("https://cipher.test/v1/blob", {
    method: "PUT",
    headers: {
      "x-osl-ttl-seconds": "3600",
      "x-osl-expiry-mode": "absolute",
      "x-osl-blob-id": id,
      "x-osl-fetch-digest": await sha256Hex(`fetch-${idChar}`),
      "x-osl-ack-digest": await sha256Hex(`ack-${idChar}`),
      "x-osl-manage-digest": await sha256Hex(`manage-${idChar}`),
      "x-osl-delivery-tag": idChar.repeat(32),
      "x-osl-object-class": "single-ack",
      "content-length": String(BODY.byteLength),
    },
    body: BODY,
  });
}

async function seedToOneByteOfHeadroom(): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  await env.DB.prepare(
    `INSERT INTO blob_capability_index (
      blob_id, fetch_digest_sha256_hex, ack_digest_sha256_hex,
      manage_digest_sha256_hex, object_class, pool, delivery_tag,
      size_bytes, expires_at, created_at
    ) VALUES (?, ?, ?, ?, 'single-ack', 'undelivered', ?, ?, ?, ?)`,
  ).bind(
    "0".repeat(32),
    await sha256Hex("seed-fetch"),
    await sha256Hex("seed-ack"),
    await sha256Hex("seed-manage"),
    "0".repeat(32),
    SEEDED_BYTES,
    now + 3600,
    now,
  ).run();
}

async function storedBytes(): Promise<number> {
  const row = await env.DB.prepare(
    "SELECT COALESCE(SUM(size_bytes), 0) AS bytes FROM blob_capability_index",
  ).first<{ bytes: number }>();
  return Number(row?.bytes ?? 0);
}

describe("D-256 the blob capacity gate admits atomically (HIGH-2)", () => {
  beforeEach(seedToOneByteOfHeadroom);

  it("refuses the second of two uploads that race for the last byte", async () => {
    const requests = [await upload("a"), await upload("b")];
    const statuses = (
      await Promise.all(requests.map((request) => handleUpload(request, workerEnv())))
    ).map((response) => response.status);

    // The ceiling is the property under test. A non-atomic gate lands at
    // MAX_LIVE_BLOB_BYTES + 1 here because both readers saw the same
    // pre-insert SUM.
    expect(await storedBytes()).toBeLessThanOrEqual(MAX_LIVE_BLOB_BYTES);
    expect(statuses.filter((status) => status === 503)).toHaveLength(1);
    expect(statuses.filter((status) => status === 201)).toHaveLength(1);
  });

  it("still admits the one upload the headroom genuinely allows", async () => {
    // Starves the assertion above: if the seeded row already blocked every
    // upload, "no over-admission" would be true for the wrong reason.
    const first = await handleUpload(await upload("a"), workerEnv());
    expect(first.status).toBe(201);
    expect(await storedBytes()).toBe(MAX_LIVE_BLOB_BYTES);

    const second = await handleUpload(await upload("b"), workerEnv());
    expect(second.status).toBe(503);
    await expect(second.json()).resolves.toMatchObject({ error: "storage_capacity" });
    expect(await storedBytes()).toBe(MAX_LIVE_BLOB_BYTES);
  });
});
