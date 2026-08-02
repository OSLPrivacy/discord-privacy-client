import { env } from "cloudflare:test";
import { describe, expect, it } from "vitest";

const encoder = new TextEncoder();

async function objectKeys(bucket: R2Bucket, prefix: string): Promise<string[]> {
  const keys: string[] = [];
  let cursor: string | undefined;
  do {
    const page = await bucket.list({ prefix, cursor });
    keys.push(...page.objects.map((object) => object.key));
    cursor = page.truncated ? page.cursor : undefined;
  } while (cursor);
  return keys.sort();
}

describe("retained archive / undelivered-store separation", () => {
  it("exhausts archive eviction without a cipher-store D1 or R2 binding", async () => {
    const namespace = `t6-k4-${crypto.randomUUID()}`;
    const archivePrefix = `${namespace}/archive/`;
    const cipherPrefix = `${namespace}/cipher-store/`;

    // This Worker deliberately has no cipher-store binding. If somebody adds
    // one in order to evict undelivered messages for archive capacity, this
    // topology assertion fails before that cross-store path can ship.
    const cipherBindings = Object.keys(env).filter((name) => /CIPHER_STORE/i.test(name));
    expect(cipherBindings).toEqual([]);

    // A cipher-store fixture gives the proof an observable D1/R2 baseline.
    // Archive receives neither a table name nor an object prefix for it.
    const db = (env as unknown as { DB: D1Database }).DB;
    await db.prepare(
      "CREATE TABLE IF NOT EXISTS t6_k4_cipher_store_rows (id TEXT PRIMARY KEY)",
    ).run();
    await db.prepare("INSERT OR REPLACE INTO t6_k4_cipher_store_rows (id) VALUES (?), (?)")
      .bind(`${namespace}-a`, `${namespace}-b`).run();
    await env.ARCHIVE_PAYLOADS.put(`${cipherPrefix}a`, encoder.encode("cipher-a"));
    await env.ARCHIVE_PAYLOADS.put(`${cipherPrefix}b`, encoder.encode("cipher-b"));
    const cipherStoreBefore = {
      d1Rows: (await db.prepare(
        "SELECT COUNT(*) AS count FROM t6_k4_cipher_store_rows WHERE id LIKE ?",
      ).bind(`${namespace}%`).first<{ count: number }>())?.count,
      r2Objects: await objectKeys(env.ARCHIVE_PAYLOADS, cipherPrefix),
    };

    // Put archive payloads where Archive itself is allowed to operate, then
    // exceed the six-byte budget twice. Both old entries must be evicted;
    // this exercises the full destructive archive path rather than merely
    // inspecting policy in isolation.
    const archive = env.ARCHIVE.getByName(namespace);
    for (const id of ["oldest", "middle", "newest"]) {
      await env.ARCHIVE_PAYLOADS.put(`${archivePrefix}${id}`, encoder.encode(id));
    }
    await archive.store({
      id: "oldest", objectKey: `${archivePrefix}oldest`, receivedAt: 1,
      expiresAt: Date.now() + 60_000, byteLength: 3,
    }, 6);
    await archive.store({
      id: "middle", objectKey: `${archivePrefix}middle`, receivedAt: 2,
      expiresAt: Date.now() + 60_000, byteLength: 3,
    }, 6);
    const result = await archive.store({
      id: "newest", objectKey: `${archivePrefix}newest`, receivedAt: 3,
      expiresAt: Date.now() + 60_000, byteLength: 3,
    }, 3);

    expect(result.evicted.map((entry) => entry.id)).toEqual(["oldest", "middle"]);
    expect(await objectKeys(env.ARCHIVE_PAYLOADS, archivePrefix)).toEqual([`${archivePrefix}newest`]);

    // Archive has exhausted its own eviction path. The cipher-store snapshot
    // is byte-identical: production exposes no cipher-store D1/R2 binding to
    // this Worker, and archive operations only address archive-entry keys.
    const cipherStoreAfter = {
      d1Rows: (await db.prepare(
        "SELECT COUNT(*) AS count FROM t6_k4_cipher_store_rows WHERE id LIKE ?",
      ).bind(`${namespace}%`).first<{ count: number }>())?.count,
      r2Objects: await objectKeys(env.ARCHIVE_PAYLOADS, cipherPrefix),
    };
    expect(cipherStoreAfter).toEqual(cipherStoreBefore);
  });
});
