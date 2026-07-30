import path from "node:path";
import { readD1Migrations } from "@cloudflare/vitest-pool-workers";
import { Miniflare } from "miniflare";
import { afterEach, describe, expect, it } from "vitest";
import {
  handleControlInboxDelete,
  handleControlInboxGet,
  handleControlInboxPost,
} from "../src/endpoints/control-inbox.js";
import { handleHealthz } from "../src/endpoints/healthz.js";
import type { Env } from "../src/env.js";
import {
  CONTROL_INBOX_EVICTION_SIGNAL_CAPABILITY,
  CONTROL_INBOX_RECONCILIATION_STARTED_CAPABILITY,
  controlInboxDispositionSchemaReady,
  reconcileControlInboxSenderStates,
} from "../src/lib/control-inbox-sweep.js";

const miniflareInstances = new Set<Miniflare>();
let databaseSequence = 0;

async function applyMigration(db: D1Database, name: string): Promise<void> {
  const migrations = await readD1Migrations(
    path.join(process.cwd(), "migrations"),
  );
  const migration = migrations.find((candidate) => candidate.name === name);
  if (!migration) throw new Error(`migration not found: ${name}`);
  await db.batch(migration.queries.map((query) => db.prepare(query)));
}

async function pre0031Db(): Promise<D1Database> {
  const mf = new Miniflare({
    modules: true,
    script: "export default { fetch() { return new Response('ok') } }",
    d1Databases: { DB: `migration-0031-${databaseSequence++}` },
  });
  miniflareInstances.add(mf);
  const db = await mf.getD1Database("DB");
  for (const migration of [
    "0001_keyserver_baseline.sql",
    "0005_control_inbox.sql",
    "0006_control_inbox_hardening.sql",
    "0016_recipient_storage_abuse_limits.sql",
    "0026_rn_capability_advertisement.sql",
    "0027_control_inbox_revocation_lane.sql",
    "0029_authoritative_osl_identity.sql",
    "0030_reserve_derived_identity_namespace.sql",
  ]) {
    await applyMigration(db, migration);
  }
  return db;
}

function id(byte: number): Uint8Array {
  return new Uint8Array(16).fill(byte);
}

function payload(byte: number): Uint8Array {
  return new Uint8Array([byte, byte + 1, byte + 2, byte + 3]);
}

function asBytes(value: unknown): Uint8Array {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (Array.isArray(value)) return Uint8Array.from(value as number[]);
  throw new Error("D1 did not return blob bytes");
}

async function seedUser(
  db: D1Database,
  userId: string,
  lookupEnabled: number,
): Promise<void> {
  await db.prepare(
    `INSERT INTO users
       (user_id, ik_x25519_pub, ik_ed25519_pub, ik_mlkem768_pub,
        ik_x25519_signature, registered_at, rn_capabilities,
        identity_lookup_enabled)
     VALUES (?, 'x', 'ed', 'mlkem', 'sig', '2026-01-01T00:00:00Z',
             0, ?)`,
  ).bind(userId, lookupEnabled).run();
}

async function oldWorkerInsert(
  db: D1Database,
  rowId: Uint8Array,
  recipientId: string,
  senderId: string,
  bytes: Uint8Array,
  expiresAt: number,
): Promise<void> {
  await db.prepare(
    `INSERT INTO control_inbox
       (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at,
        kind, collapse_key)
     VALUES (?, ?, ?, 'scope', ?, ?, ?, '', NULL)`,
  ).bind(
    rowId,
    recipientId,
    senderId,
    bytes,
    expiresAt,
    expiresAt - 60,
  ).run();
}

function workerEnv(db: D1Database): Env {
  const allow = { limit: async () => ({ success: true }) };
  return {
    DB: db,
    RATE_LIMIT_5: allow,
    RATE_LIMIT_10: allow,
    RATE_LIMIT_120: allow,
    RATE_LIMIT_1200: allow,
    RATE_LIMIT_3600: allow,
  } as unknown as Env;
}

function healthCapabilities(ready: 0 | 1) {
  return {
    control_inbox_sender_disposition: ready,
    [CONTROL_INBOX_EVICTION_SIGNAL_CAPABILITY]: ready,
  };
}

afterEach(async () => {
  await Promise.all(
    [...miniflareInstances].map((instance) => instance.dispose()),
  );
  miniflareInstances.clear();
});

describe("migration 0031 control-inbox sender retention", () => {
  it("verify deployed migration 0031 control_inbox_sender_retention behaves", async () => {
    const db = await pre0031Db();
    await seedUser(db, "disabled-retained-sender", 0);
    const retained = payload(60);
    await oldWorkerInsert(
      db,
      id(6),
      "recipient",
      "disabled-retained-sender",
      retained,
      1_000_000_000,
    );

    await applyMigration(db, "0031_control_inbox_sender_retention.sql");

    const capability = await db.prepare(
      `SELECT version FROM worker_schema_capabilities
        WHERE capability = 'control_inbox_sender_disposition'`,
    ).first<number>("version");
    expect(capability).toBe(1);

    const oldExpiryDelete = await db.prepare(
      "DELETE FROM control_inbox WHERE expires_at < ?",
    ).bind(1_900_000_000).run();
    expect(oldExpiryDelete.meta?.changes ?? 0).toBe(0);
    expect(
      asBytes(
        (await db.prepare(
          "SELECT bundle FROM control_inbox WHERE id = ?",
        ).bind(id(6)).first<{ bundle: unknown }>())?.bundle,
      ),
    ).toEqual(retained);

    await expect(
      db.prepare(
        `UPDATE control_inbox
            SET delivery_status = 'retryable',
                delivery_reason = NULL,
                delivery_attempts = 1,
                sender_disabled_first_seen_at = 1900000000,
                delivery_next_retry_at = 1900000060,
                delivery_retain_until = 2504800000
          WHERE id = ?`,
      ).bind(id(6)).run(),
    ).rejects.toThrow("control inbox delivery state is inconsistent");

    await applyMigration(db, "0035_control_inbox_eviction_signal.sql");
    const reconciled = await reconcileControlInboxSenderStates(db, 1_900_000_000);
    expect(reconciled.retryable).toBe(1);
    const classified = await db.prepare(
      `SELECT delivery_status,
              delivery_reason,
              delivery_attempts,
              sender_disabled_first_seen_at,
              delivery_next_retry_at,
              delivery_retain_until
         FROM control_inbox WHERE id = ?`,
    ).bind(id(6)).first<Record<string, unknown>>();
    expect(classified).toMatchObject({
      delivery_status: "retryable",
      delivery_reason: "sender_lookup_disabled",
      delivery_attempts: 1,
      sender_disabled_first_seen_at: 1_900_000_000,
      delivery_next_retry_at: 1_900_003_600,
      delivery_retain_until: 1_900_604_800,
    });

    const statusAwareDrain = await db.prepare(
      `SELECT COUNT(*) AS count FROM control_inbox
        WHERE recipient_id = 'recipient'
          AND delivery_status = 'live'
          AND expires_at >= 1900000000`,
    ).first<number>("count");
    expect(statusAwareDrain).toBe(0);
  });

  it("backfills legacy rows and keeps old-worker insert/update/select byte-compatible", async () => {
    const db = await pre0031Db();
    await seedUser(db, "legacy-disabled", 0);
    const before = payload(10);
    await oldWorkerInsert(
      db,
      id(1),
      "recipient",
      "legacy-disabled",
      before,
      2_000_000_000,
    );
    const expired = payload(15);
    await oldWorkerInsert(
      db,
      id(4),
      "recipient",
      "legacy-disabled",
      expired,
      1_000_000_000,
    );

    await applyMigration(db, "0031_control_inbox_sender_retention.sql");

    // Exact pre-0031 scheduled cleanup SQL. The migration-first window cannot
    // silently erase an expired row before the new Worker records why its
    // sender cannot be looked up.
    const oldSweep = await db.prepare(
      "DELETE FROM control_inbox WHERE expires_at < ?",
    ).bind(1_900_000_000).run();
    expect(oldSweep.meta?.changes ?? 0).toBe(0);
    expect(
      asBytes(
        (await db.prepare(
          "SELECT bundle FROM control_inbox WHERE id = ?",
        ).bind(id(4)).first<{ bundle: unknown }>())?.bundle,
      ),
    ).toEqual(expired);

    const migrated = await db.prepare(
      `SELECT bundle,
              delivery_status,
              delivery_reason,
              delivery_attempts,
              sender_disabled_first_seen_at,
              delivery_next_retry_at,
              delivery_retain_until
         FROM control_inbox WHERE id = ?`,
    ).bind(id(1)).first<Record<string, unknown>>();
    expect(asBytes(migrated?.bundle)).toEqual(before);
    expect(migrated).toMatchObject({
      delivery_status: "live",
      delivery_reason: null,
      delivery_attempts: 0,
      sender_disabled_first_seen_at: null,
      delivery_next_retry_at: null,
      delivery_retain_until: null,
    });

    const after = payload(20);
    await oldWorkerInsert(
      db,
      id(2),
      "recipient",
      "legacy-disabled",
      after,
      2_000_000_100,
    );
    const replacement = payload(30);
    await db.prepare(
      `UPDATE control_inbox
          SET bundle = ?, expires_at = ?
        WHERE id = ?`,
    ).bind(replacement, 2_000_000_200, id(2)).run();

    // Exact pre-0031 drain projection: it neither names nor depends on the new
    // metadata and still receives the opaque bytes unchanged.
    const oldRows = await db.prepare(
      `SELECT id, sender_id, scope_id, bundle, created_at, kind
         FROM control_inbox
        WHERE recipient_id = ? AND expires_at >= ?
        ORDER BY created_at ASC`,
    ).bind("recipient", 1_900_000_000).all<{ bundle: unknown }>();
    expect(oldRows.results).toHaveLength(2);
    expect(asBytes(oldRows.results?.[0]?.bundle)).toEqual(before);
    expect(asBytes(oldRows.results?.[1]?.bundle)).toEqual(replacement);
  });

  it("requires migration-first because the status-aware worker refuses the old schema", async () => {
    const db = await pre0031Db();
    await expect(
      reconcileControlInboxSenderStates(db, 1_900_000_000),
    ).rejects.toThrow("control inbox schema unavailable");

    const env = workerEnv(db);
    const refused = await Promise.all([
      handleControlInboxPost(
        new Request("https://keyserver.invalid/v1/control-inbox", {
          method: "POST",
          body: "{}",
        }),
        env,
      ),
      handleControlInboxGet(
        new Request("https://keyserver.invalid/v1/control-inbox/recipient"),
        env,
        "recipient",
      ),
      handleControlInboxDelete(
        new Request("https://keyserver.invalid/v1/control-inbox/00", {
          method: "DELETE",
          body: "{}",
        }),
        env,
        "00",
      ),
      handleHealthz(env),
    ]);
    expect(refused.map((response) => response.status)).toEqual([
      503,
      503,
      503,
      503,
    ]);
    for (const response of refused.slice(0, 3)) {
      expect(await response.json()).toEqual({
        error: "control inbox schema unavailable",
      });
    }
    expect(await refused[3]!.json()).toEqual({
      ok: false,
      capabilities: healthCapabilities(0),
    });
    expect(await controlInboxDispositionSchemaReady(db)).toBe(false);

    await applyMigration(db, "0031_control_inbox_sender_retention.sql");
    expect(await controlInboxDispositionSchemaReady(db)).toBe(false);

    await applyMigration(db, "0035_control_inbox_eviction_signal.sql");
    expect(await controlInboxDispositionSchemaReady(db)).toBe(true);
    const healthy = await handleHealthz(env);
    expect(healthy.status).toBe(200);
    expect(await healthy.json()).toEqual({
      ok: true,
      capabilities: healthCapabilities(1),
    });

    // The capability gate is no longer the reason for refusal: ordinary input
    // validation now answers these deliberately malformed requests.
    const after = await Promise.all([
      handleControlInboxPost(
        new Request("https://keyserver.invalid/v1/control-inbox", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: "{}",
        }),
        env,
      ),
      handleControlInboxGet(
        new Request("https://keyserver.invalid/v1/control-inbox/recipient"),
        env,
        "recipient",
      ),
      handleControlInboxDelete(
        new Request("https://keyserver.invalid/v1/control-inbox/00", {
          method: "DELETE",
          headers: { "content-type": "application/json" },
          body: "{}",
        }),
        env,
        "00",
      ),
    ]);
    expect(after.map((response) => response.status)).toEqual([400, 400, 400]);
  });

  it("refuses marker-only, wrong-version, and otherwise mixed schema states", async () => {
    const db = await pre0031Db();
    const env = workerEnv(db);
    await db.prepare(
      `CREATE TABLE worker_schema_capabilities (
         capability TEXT PRIMARY KEY,
         version INTEGER NOT NULL
       ) WITHOUT ROWID`,
    ).run();
    await db.prepare(
      `INSERT INTO worker_schema_capabilities (capability, version)
       VALUES ('control_inbox_sender_disposition', 1)`,
    ).run();

    // The marker alone is not authority: the zero-row projection must prove
    // every disposition column without reading a row or its payload.
    expect(await controlInboxDispositionSchemaReady(db)).toBe(false);
    const markerOnly = await handleHealthz(env);
    expect(markerOnly.status).toBe(503);
    expect(await markerOnly.json()).toEqual({
      ok: false,
      capabilities: healthCapabilities(0),
    });

    await db.prepare(
      `UPDATE worker_schema_capabilities SET version = 2
        WHERE capability = 'control_inbox_sender_disposition'`,
    ).run();
    expect(await controlInboxDispositionSchemaReady(db)).toBe(false);
    expect((await handleHealthz(env)).status).toBe(503);
  });

  it("proves a post-classification downgrade would expose retained rows", async () => {
    const db = await pre0031Db();
    await seedUser(db, "disabled-sender", 0);
    const bytes = payload(40);
    await oldWorkerInsert(
      db,
      id(3),
      "recipient",
      "disabled-sender",
      bytes,
      2_000_000_000,
    );
    await applyMigration(db, "0031_control_inbox_sender_retention.sql");
    await applyMigration(db, "0035_control_inbox_eviction_signal.sql");

    expect(
      (await reconcileControlInboxSenderStates(db, 1_900_000_000)).retryable,
    ).toBe(1);
    const statusAware = await db.prepare(
      `SELECT COUNT(*) AS count FROM control_inbox
        WHERE recipient_id = 'recipient'
          AND delivery_status = 'live'
          AND expires_at >= 1900000000`,
    ).first<number>("count");
    expect(statusAware).toBe(0);

    // This is the exact old-Worker predicate. The positive nonempty result is
    // why rollback is forbidden after the first reconciliation cycle.
    const oldWorker = await db.prepare(
      `SELECT bundle FROM control_inbox
        WHERE recipient_id = 'recipient'
          AND expires_at >= 1900000000`,
    ).first<{ bundle: unknown }>();
    expect(asBytes(oldWorker?.bundle)).toEqual(bytes);

    const oldDelete = await db.prepare(
      "DELETE FROM control_inbox WHERE recipient_id = 'recipient'",
    ).run();
    expect(oldDelete.meta?.changes ?? 0).toBe(0);
    expect(
      asBytes(
        (await db.prepare(
          "SELECT bundle FROM control_inbox WHERE id = ?",
        ).bind(id(3)).first<{ bundle: unknown }>())?.bundle,
      ),
    ).toEqual(bytes);
  });

  it("commits a nonempty rollback marker before the first status write", async () => {
    const db = await pre0031Db();
    await seedUser(db, "marker-disabled-sender", 0);
    await oldWorkerInsert(
      db,
      id(5),
      "recipient",
      "marker-disabled-sender",
      payload(50),
      2_000_000_000,
    );
    await applyMigration(db, "0031_control_inbox_sender_retention.sql");
    await applyMigration(db, "0035_control_inbox_eviction_signal.sql");
    await db.prepare(
      `CREATE TRIGGER reject_reconciliation_marker
       BEFORE INSERT ON worker_schema_capabilities
       WHEN NEW.capability = '${CONTROL_INBOX_RECONCILIATION_STARTED_CAPABILITY}'
       BEGIN
         SELECT RAISE(ABORT, 'rollback marker refused');
       END`,
    ).run();

    await expect(
      reconcileControlInboxSenderStates(db, 1_900_000_000),
    ).rejects.toThrow("rollback marker refused");
    expect(
      await db.prepare(
        "SELECT delivery_status FROM control_inbox WHERE id = ?",
      ).bind(id(5)).first<string>("delivery_status"),
    ).toBe("live");

    await db.prepare("DROP TRIGGER reject_reconciliation_marker").run();
    expect(
      (await reconcileControlInboxSenderStates(db, 1_900_000_000)).retryable,
    ).toBe(1);
    expect(
      await db.prepare(
        `SELECT version FROM worker_schema_capabilities
          WHERE capability = ?`,
      ).bind(CONTROL_INBOX_RECONCILIATION_STARTED_CAPABILITY)
        .first<number>("version"),
    ).toBe(1);
    expect(
      await db.prepare(
        "SELECT delivery_status FROM control_inbox WHERE id = ?",
      ).bind(id(5)).first<string>("delivery_status"),
    ).toBe("retryable");
  });
});
