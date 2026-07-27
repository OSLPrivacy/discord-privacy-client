import path from "node:path";
import { readD1Migrations } from "@cloudflare/vitest-pool-workers";
import { Miniflare } from "miniflare";
import { afterEach, describe, expect, it } from "vitest";
import { handleRegister } from "../src/endpoints/register.js";
import type { Env } from "../src/env.js";
import {
  generateEd25519Pair,
  signedRegisterBody,
} from "../test/integration/helpers.js";

const miniflareInstances = new Set<Miniflare>();
let identitySequence = 0;

async function applyMigration(db: D1Database, name: string): Promise<void> {
  const migrations = await readD1Migrations(
    path.join(process.cwd(), "migrations"),
  );
  const migration = migrations.find((candidate) => candidate.name === name);
  if (!migration) throw new Error(`migration not found: ${name}`);
  await db.batch(migration.queries.map((query) => db.prepare(query)));
}

async function pre0030Db(): Promise<D1Database> {
  const mf = new Miniflare({
    modules: true,
    script: "export default { fetch() { return new Response('ok') } }",
    d1Databases: { DB: `migration-0030-${identitySequence++}` },
  });
  miniflareInstances.add(mf);
  const db = await mf.getD1Database("DB");
  await applyMigration(db, "0001_keyserver_baseline.sql");
  await applyMigration(db, "0026_rn_capability_advertisement.sql");
  await applyMigration(db, "0029_authoritative_osl_identity.sql");
  return db;
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

async function register(
  db: D1Database,
  body: Record<string, unknown>,
): Promise<Response> {
  return handleRegister(
    new Request("https://keyserver.invalid/v1/register", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "cf-connecting-ip": "192.0.2.30",
      },
      body: JSON.stringify(body),
    }),
    workerEnv(db),
  );
}

async function seedLegacy(db: D1Database, userId: string): Promise<void> {
  await db
    .prepare(
      `INSERT INTO users
       (user_id, ik_x25519_pub, ik_ed25519_pub, ik_mlkem768_pub,
        ik_x25519_signature, ik_ratchet_initial_pub, registered_at,
        last_rotated_at, rn_capabilities, identity_lookup_enabled)
       VALUES (?, 'x', 'ed', 'mlkem', 'sig', NULL, '2026-01-01T00:00:00Z',
               NULL, 0, 1)`,
    )
    .bind(userId)
    .run();
}

afterEach(async () => {
  await Promise.all(
    [...miniflareInstances].map((instance) => instance.dispose()),
  );
  miniflareInstances.clear();
});

describe("migration 0030 preparatory derived-identity namespace", () => {
  it("is backward-compatible with legacy rows and old-worker writes", async () => {
    const db = await pre0030Db();
    await seedLegacy(db, "legacy-before");
    await applyMigration(db, "0030_reserve_derived_identity_namespace.sql");

    const migrated = await db
      .prepare(
        `SELECT identity_scheme, ik_root_ed25519_pub
           FROM users WHERE user_id = 'legacy-before'`,
      )
      .first<{ identity_scheme: number; ik_root_ed25519_pub: string | null }>();
    expect(migrated).toEqual({
      identity_scheme: 0,
      ik_root_ed25519_pub: null,
    });

    // Exact compatibility property relied on by a pre-0030 Worker: inserts
    // and updates name only the legacy columns and take the new defaults.
    await seedLegacy(db, "legacy-after");
    await db
      .prepare(
        `UPDATE users SET ik_x25519_pub = 'x2'
          WHERE user_id = 'legacy-before'`,
      )
      .run();
    const after = await db
      .prepare(
        `SELECT COUNT(*) AS rows,
                SUM(identity_scheme = 0) AS scheme_zero,
                SUM(ik_root_ed25519_pub IS NULL) AS roots_null
           FROM users`,
      )
      .first<{ rows: number; scheme_zero: number; roots_null: number }>();
    expect(after).toEqual({ rows: 2, scheme_zero: 2, roots_null: 2 });
  });

  it("worker-first refuses the reserved namespace on a pre-0030 schema", async () => {
    const db = await pre0030Db();
    const pair = await generateEd25519Pair();
    const derivedId = `osl1_${"a".repeat(32)}`;
    const body = await signedRegisterBody(derivedId, pair);

    // Attacker-supplied fields are not a root proof and cannot turn the
    // preparatory refusal into scheme-1 registration.
    body.identity_scheme = 1;
    body.ik_root_ed25519_pub = pair.publicKeyB64;
    const refused = await register(db, body);
    expect(refused.status).toBe(400);
    expect(await refused.json()).toEqual({
      error: "canonical identity proofs are required",
    });
    const count = await db
      .prepare("SELECT COUNT(*) AS count FROM users WHERE user_id = ?")
      .bind(derivedId)
      .first<{ count: number }>();
    expect(count?.count).toBe(0);

    // The same Worker has no dependency on the not-yet-present columns for
    // ordinary scheme-0 registration.
    const ordinary = await signedRegisterBody("ordinary-before-0030", pair);
    expect((await register(db, ordinary)).status).toBe(201);
  });

  it("keeps current signed scheme-0 registration stable after migration", async () => {
    const db = await pre0030Db();
    await applyMigration(db, "0030_reserve_derived_identity_namespace.sql");
    const pair = await generateEd25519Pair();
    const body = await signedRegisterBody("ordinary-after-0030", pair);

    // The legacy signed shape remains scheme 0. Scheme 1 is a distinct
    // canonical proof-bearing branch, never a flag silently ignored here.
    expect((await register(db, body)).status).toBe(201);
    expect((await register(db, body)).status).toBe(200);

    const stored = await db
      .prepare(
        `SELECT identity_scheme, ik_root_ed25519_pub
           FROM users WHERE user_id = 'ordinary-after-0030'`,
      )
      .first<{ identity_scheme: number; ik_root_ed25519_pub: string | null }>();
    expect(stored).toEqual({
      identity_scheme: 0,
      ik_root_ed25519_pub: null,
    });
  });

  it("refuses a flag-only promotion to scheme 1", async () => {
    const db = await pre0030Db();
    await seedLegacy(db, "legacy");
    await applyMigration(db, "0030_reserve_derived_identity_namespace.sql");
    await expect(
      db
        .prepare(
          "UPDATE users SET identity_scheme = 1 WHERE user_id = 'legacy'",
        )
        .run(),
    ).rejects.toThrow(/verified root proof/);
  });

  it("refuses storing an unverified root on a scheme-0 row", async () => {
    const db = await pre0030Db();
    await seedLegacy(db, "legacy");
    await applyMigration(db, "0030_reserve_derived_identity_namespace.sql");
    await expect(
      db
        .prepare(
          "UPDATE users SET ik_root_ed25519_pub = 'unverified' WHERE user_id = 'legacy'",
        )
        .run(),
    ).rejects.toThrow(/verified root proof/);
  });

  it("refuses a direct scheme-1 insert even when root bytes are present", async () => {
    const db = await pre0030Db();
    await applyMigration(db, "0030_reserve_derived_identity_namespace.sql");
    await expect(
      db
        .prepare(
          `INSERT INTO users
           (user_id, ik_x25519_pub, ik_ed25519_pub, ik_mlkem768_pub,
            ik_x25519_signature, registered_at, rn_capabilities,
            identity_lookup_enabled, identity_scheme, ik_root_ed25519_pub)
           VALUES ('osl1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                   'x', 'ed', 'mlkem', 'sig', '2026-01-01T00:00:00Z',
                   0, 1, 1, 'unverified')`,
        )
        .run(),
    ).rejects.toThrow(/verified root proof/);
  });
});
