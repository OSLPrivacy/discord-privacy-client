import { env } from "cloudflare:test";
import { describe, expect, it } from "vitest";

const BLOB_ID = "1".repeat(32);
const DIGEST = "2".repeat(64);
const DELIVERY_TAG = "3".repeat(32);

async function insertR2Blob(overrides: Record<string, string> = {}): Promise<void> {
  const row = {
    blob_id: BLOB_ID,
    fetch_digest_sha256_hex: DIGEST,
    ack_digest_sha256_hex: DIGEST,
    manage_digest_sha256_hex: DIGEST,
    object_class: "single-ack",
    pool: "undelivered",
    delivery_tag: DELIVERY_TAG,
    ...overrides,
  };

  await env.DB.prepare(
    `INSERT INTO blob_capability_index (
       blob_id, fetch_digest_sha256_hex, ack_digest_sha256_hex,
       manage_digest_sha256_hex, object_class, pool, delivery_tag,
       size_bytes, expires_at, created_at
     ) VALUES (?, ?, ?, ?, ?, ?, ?, 1, 2, 1)`,
  ).bind(
    row.blob_id,
    row.fetch_digest_sha256_hex,
    row.ack_digest_sha256_hex,
    row.manage_digest_sha256_hex,
    row.object_class,
    row.pool,
    row.delivery_tag,
  ).run();
}

describe("migration 0011 blob capability index", () => {
  it("requires the three capability digests, r2 id width, object class, pool, and delivery tag", async () => {
    await expect(insertR2Blob()).resolves.toBeUndefined();
    await expect(insertR2Blob({ blob_id: "1".repeat(16) })).rejects.toThrow();
    await expect(insertR2Blob({
      blob_id: "a".repeat(32),
      fetch_digest_sha256_hex: "2".repeat(63),
    })).rejects.toThrow();
    await expect(insertR2Blob({
      blob_id: "b".repeat(32),
      ack_digest_sha256_hex: "2".repeat(63),
    })).rejects.toThrow();
    await expect(insertR2Blob({
      blob_id: "c".repeat(32),
      manage_digest_sha256_hex: "2".repeat(63),
    })).rejects.toThrow();
    await expect(insertR2Blob({
      blob_id: "d".repeat(32),
      object_class: "view-once",
    })).rejects.toThrow();
    await expect(insertR2Blob({
      blob_id: "e".repeat(32),
      pool: "retained",
    })).rejects.toThrow();
  });
});
