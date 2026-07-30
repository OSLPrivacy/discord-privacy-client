import path from "node:path";
import { fileURLToPath } from "node:url";
import { readD1Migrations } from "@cloudflare/vitest-pool-workers";
import { Miniflare } from "miniflare";
import { afterEach, describe, expect, it } from "vitest";

const miniflareInstances = new Set<Miniflare>();
let databaseSequence = 0;

const keyserverRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);

async function applyMigration(db: D1Database, name: string): Promise<void> {
  const migrations = await readD1Migrations(
    path.join(keyserverRoot, "migrations"),
  );
  const migration = migrations.find((candidate) => candidate.name === name);
  if (!migration) throw new Error(`migration not found: ${name}`);
  await db.batch(migration.queries.map((query) => db.prepare(query)));
}

async function preProofRequiredDb(): Promise<D1Database> {
  const mf = new Miniflare({
    modules: true,
    script: "export default { fetch() { return new Response('ok') } }",
    d1Databases: { DB: `migration-0036-proof-required-${databaseSequence++}` },
  });
  miniflareInstances.add(mf);
  const db = await mf.getD1Database("DB");
  await applyMigration(db, "0001_keyserver_baseline.sql");
  await applyMigration(db, "0036_account_ownership_challenges.sql");
  return db;
}

async function seedUser(db: D1Database, userId: string): Promise<void> {
  await db.prepare(
    `INSERT INTO users
       (user_id, ik_x25519_pub, ik_ed25519_pub, ik_mlkem768_pub,
        ik_x25519_signature, registered_at)
     VALUES (?, 'x', 'ed', 'mlkem', 'sig', '2026-01-01T00:00:00Z')`,
  ).bind(userId).run();
}

async function insertChallenge(
  db: D1Database,
  args: {
    nonce: string;
    binding: string;
    issuedAt?: number;
    expiresAt?: number;
    spentAt?: number | null;
  },
): Promise<void> {
  await db.prepare(
    `INSERT INTO account_ownership_challenges (
       nonce_sha256, binding_sha256, service,
       issued_at_unix_seconds, expires_at_unix_seconds, spent_at_unix_seconds
     ) VALUES (?, ?, 'discord', ?, ?, ?)`,
  ).bind(
    args.nonce,
    args.binding,
    args.issuedAt ?? 1_900_000_000,
    args.expiresAt ?? 1_900_000_300,
    args.spentAt ?? null,
  ).run();
}

async function spendChallenge(
  db: D1Database,
  nonce: string,
  spentAt = 1_900_000_030,
): Promise<void> {
  await db.prepare(
    `UPDATE account_ownership_challenges
        SET spent_at_unix_seconds = ?
      WHERE nonce_sha256 = ?`,
  ).bind(spentAt, nonce).run();
}

function proofBindingSql(): string {
  return `INSERT INTO account_ownership_proof_bindings (
            binding_sha256, nonce_sha256, owner_user_id, service, proof_type,
            verified_at_unix_seconds
          ) VALUES (?, ?, ?, 'discord', 'ed25519_identity_challenge_v1', ?)`;
}

async function insertProofBinding(
  db: D1Database,
  args: {
    nonce: string;
    binding: string;
    ownerUserId?: string;
    verifiedAt?: number;
  },
) {
  return await db.prepare(proofBindingSql()).bind(
    args.binding,
    args.nonce,
    args.ownerUserId ?? "owner-osl-id",
    args.verifiedAt ?? 1_900_000_040,
  ).run();
}

afterEach(async () => {
  await Promise.all(
    [...miniflareInstances].map((instance) => instance.dispose()),
  );
  miniflareInstances.clear();
});

describe("migration 0036 account ownership proof required", () => {
  it("requires a matching spent challenge before durable proof binding", async () => {
    const db = await preProofRequiredDb();
    await seedUser(db, "owner-osl-id");
    await applyMigration(db, "0036_account_ownership_proof_required.sql");

    const capability = await db.prepare(
      `SELECT version FROM worker_schema_capabilities
        WHERE capability = 'account_ownership_proof_required'`,
    ).first<number>("version");
    expect(capability).toBe(1);

    const nonce = "a".repeat(64);
    const binding = "b".repeat(64);
    await expect(
      insertProofBinding(db, { nonce, binding }),
    ).rejects.toThrow(/proof challenge is required/);

    await insertChallenge(db, { nonce, binding });
    await expect(
      insertProofBinding(db, { nonce, binding }),
    ).rejects.toThrow(/proof challenge is required/);

    await spendChallenge(db, nonce);
    await expect(
      insertProofBinding(db, { nonce, binding }),
    ).resolves.toMatchObject({ success: true });

    const stored = await db.prepare(
      `SELECT binding_sha256, nonce_sha256, owner_user_id, service, proof_type,
              verified_at_unix_seconds
         FROM account_ownership_proof_bindings
        WHERE nonce_sha256 = ?`,
    ).bind(nonce).first<Record<string, unknown>>();
    expect(stored).toMatchObject({
      binding_sha256: binding,
      nonce_sha256: nonce,
      owner_user_id: "owner-osl-id",
      service: "discord",
      proof_type: "ed25519_identity_challenge_v1",
      verified_at_unix_seconds: 1_900_000_040,
    });
  });

  it("refuses mismatched, expired, and non-user proof bindings", async () => {
    const db = await preProofRequiredDb();
    await seedUser(db, "owner-osl-id");
    await applyMigration(db, "0036_account_ownership_proof_required.sql");

    const nonceA = "c".repeat(64);
    const bindingA = "d".repeat(64);
    await insertChallenge(db, { nonce: nonceA, binding: bindingA });
    await spendChallenge(db, nonceA);
    await expect(
      insertProofBinding(db, {
        nonce: nonceA,
        binding: "e".repeat(64),
      }),
    ).rejects.toThrow(/proof challenge is required/);

    const nonceB = "1".repeat(64);
    const bindingB = "2".repeat(64);
    await insertChallenge(db, {
      nonce: nonceB,
      binding: bindingB,
      issuedAt: 1_900_001_000,
      expiresAt: 1_900_001_100,
      spentAt: 1_900_001_120,
    });
    await expect(
      insertProofBinding(db, {
        nonce: nonceB,
        binding: bindingB,
        verifiedAt: 1_900_001_120,
      }),
    ).rejects.toThrow(/proof challenge is required/);

    const nonceC = "3".repeat(64);
    const bindingC = "4".repeat(64);
    await insertChallenge(db, {
      nonce: nonceC,
      binding: bindingC,
      spentAt: 1_900_000_030,
    });
    await expect(
      insertProofBinding(db, {
        nonce: nonceC,
        binding: bindingC,
        ownerUserId: "missing-owner",
      }),
    ).rejects.toThrow(/proof challenge is required/);
  });

  it("makes durable proof binding history immutable", async () => {
    const db = await preProofRequiredDb();
    await seedUser(db, "owner-osl-id");
    await applyMigration(db, "0036_account_ownership_proof_required.sql");

    const nonce = "5".repeat(64);
    const binding = "6".repeat(64);
    await insertChallenge(db, {
      nonce,
      binding,
      spentAt: 1_900_000_030,
    });
    await insertProofBinding(db, { nonce, binding });

    await expect(
      db.prepare(
        `UPDATE account_ownership_proof_bindings
            SET verified_at_unix_seconds = verified_at_unix_seconds + 1
          WHERE nonce_sha256 = ?`,
      ).bind(nonce).run(),
    ).rejects.toThrow(/binding is immutable/);
    await expect(
      db.prepare(
        `DELETE FROM account_ownership_proof_bindings
          WHERE nonce_sha256 = ?`,
      ).bind(nonce).run(),
    ).rejects.toThrow(/binding is immutable/);
  });
});
