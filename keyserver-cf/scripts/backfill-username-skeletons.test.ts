// D-248 — the backfill instrument, driven against the real migration files.
//
// Two things are proved here that a hand-written assertion could not:
//   1. the node loader and the Worker loader of the pinned UTS #39 artifact
//      agree, vector for vector, on `test/fixtures/username-skeletons.json`
//      (`test/unit/username.test.ts` asserts the other side of the same file);
//   2. the SQL this tool emits, applied to a database built from
//      `migrations/0025_username_directory.sql` +
//      `migrations/0038_username_identity_hardening.sql`, satisfies the guard
//      that `migrations-contract/0100_username_identity_contract.sql` runs —
//      and that the same database REFUSES a confusable claim afterwards.

import { execFileSync } from "node:child_process";
import { DatabaseSync } from "node:sqlite";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
// @ts-expect-error — plain-JS operator script, imported for its pure planner.
import { planBackfill, readRows } from "./backfill-username-skeletons.mjs";
// @ts-expect-error — plain-JS loader for the pinned UTS #39 wasm artifact.
import { usernameSkeleton } from "./username-skeleton-node.mjs";

const ROOT = process.cwd();
const SCRIPT = join(ROOT, "scripts/backfill-username-skeletons.mjs");

function migratedDb(): InstanceType<typeof DatabaseSync> {
  const db = new DatabaseSync(":memory:");
  db.exec("CREATE TABLE users (user_id TEXT PRIMARY KEY)");
  db.exec("CREATE TABLE worker_schema_capabilities (capability TEXT PRIMARY KEY, version INTEGER)");
  db.exec(readFileSync(join(ROOT, "migrations/0025_username_directory.sql"), "utf8"));
  return db;
}

/// Everything in 0038 except the backfill UPDATE, so a test can decide for
/// itself what state the pre-hardening rows are in.
function applyExpand(db: InstanceType<typeof DatabaseSync>): void {
  const sql = readFileSync(join(ROOT, "migrations/0038_username_identity_hardening.sql"), "utf8");
  db.exec(sql);
}

/// The guard half of 0100, verbatim in behaviour: it must see zero rows.
function contractGuardRowCount(db: InstanceType<typeof DatabaseSync>): number {
  const row = db.prepare(
    `SELECT COUNT(*) AS n FROM username_directory
      WHERE username_skeleton IS NULL OR display_username IS NULL`,
  ).get() as { n: number };
  return row.n;
}

function seed(db: InstanceType<typeof DatabaseSync>, username: string, userId: string): void {
  db.exec(`INSERT INTO users (user_id) VALUES ('${userId}')`);
  db.exec(
    `INSERT INTO username_directory (username, user_id, friend_code, claimed_at, updated_at)
     VALUES ('${username}', '${userId}', 'OSLFR1.code', '2026-08-04T00:00:00Z', '2026-08-04T00:00:00Z')`,
  );
}

function directoryRows(db: InstanceType<typeof DatabaseSync>): unknown[] {
  return db.prepare(
    "SELECT username, username_skeleton, display_username FROM username_directory ORDER BY username",
  ).all();
}

function runCli(args: string[]): { status: number; stdout: string } {
  try {
    const stdout = execFileSync("node", [SCRIPT, ...args], { encoding: "utf8", stdio: "pipe" });
    return { status: 0, stdout };
  } catch (error) {
    const e = error as { status?: number; stdout?: string; stderr?: string };
    return { status: e.status ?? 1, stdout: (e.stdout ?? "") + (e.stderr ?? "") };
  }
}

describe("D-248 · the pinned UTS #39 artifact, loaded from node", () => {
  it("reproduces every cross-runtime vector the Worker side asserts", () => {
    const fixture = JSON.parse(
      readFileSync(join(ROOT, "test/fixtures/username-skeletons.json"), "utf8"),
    ) as { vectors: [string, string][] };
    expect(fixture.vectors.length).toBeGreaterThan(20);
    for (const [name, skeleton] of fixture.vectors) {
      expect([name, usernameSkeleton(name)]).toEqual([name, skeleton]);
    }
  });
});

describe("D-248 · backfill planning", () => {
  // The plan is judged by what applying it DOES, never by what its text says.
  // A `toContain` over generated SQL is a source-text assertion: it would pass
  // for a statement that never reaches a database.
  it("rewrites a raw skeleton to the real one and never drops a row", () => {
    const store = migratedDb();
    seed(store, "michael", "u-1");
    seed(store, "quiet_name", "u-2");
    applyExpand(store);
    const plan = planBackfill({ directory: directoryRows(store) });
    expect(plan.refused).toBe(false);
    expect(plan.collisions).toEqual([]);
    expect(plan.updateCount).toBe(2);

    store.exec(plan.sql);
    const after = directoryRows(store);
    expect(after).toEqual([
      { username: "michael", username_skeleton: "rnichael", display_username: "michael" },
      { username: "quiet_name", username_skeleton: "quiet_narne", display_username: "quiet_name" },
    ]);
    store.close();
  });

  it("REFUSES a collision, names the whole set, and emits nothing to apply", () => {
    const plan = planBackfill({
      directory: [
        { username: "michael", username_skeleton: "michael", display_username: "michael" },
        { username: "michae1", username_skeleton: "michae1", display_username: "michae1" },
        { username: "supp0rt", username_skeleton: "supp0rt", display_username: "supp0rt" },
        { username: "support", username_skeleton: "support", display_username: "support" },
      ],
    });
    expect(plan.refused).toBe(true);
    expect(plan.updateCount).toBe(0);
    expect(plan.collisions).toEqual([
      { skeleton: "rnichael", usernames: ["michae1", "michael"] },
      { skeleton: "support", usernames: ["supp0rt", "support"] },
    ]);
  });

  it("REFUSES a handle the artifact will not analyze instead of skipping it", () => {
    const plan = planBackfill({ directory: [{ username: "PAYPAL", username_skeleton: null }] });
    expect(plan.refused).toBe(true);
    expect(plan.updateCount).toBe(0);
    expect(plan.problems).toEqual([
      'username_directory: "PAYPAL" — username is not an acceptable identifier: not canonical under UTS #39 normalization',
    ]);
  });

  it("REFUSES two tombstones that fold together", () => {
    const plan = planBackfill({
      directory: [],
      tombstones: [
        { username: "paypal", skeleton: "paypal" },
        { username: "paypa1", skeleton: "paypa1" },
      ],
    });
    expect(plan.refused).toBe(true);
    expect(plan.collisions[0]).toMatchObject({ table: "username_tombstones", skeleton: "paypal" });
  });

  it("is a no-op the second time", () => {
    const rows = [{ username: "michael", username_skeleton: "rnichael", display_username: "michael" }];
    expect(planBackfill({ directory: rows }).updateCount).toBe(0);
  });

  it("accepts all three shapes wrangler and operators produce", () => {
    const one = readRows(JSON.stringify([{ results: [{ username: "a" }] }]), "x");
    const two = readRows(JSON.stringify({ results: [{ username: "a" }] }), "x");
    const three = readRows(JSON.stringify([{ username: "a" }]), "x");
    expect([one, two, three]).toEqual([[{ username: "a" }], [{ username: "a" }], [{ username: "a" }]]);
  });
});

describe("D-248 · the emitted SQL against the real migrations", () => {
  it("turns a pre-hardening directory into one the 0100 guard accepts, and the skeleton index then bites", () => {
    const store = migratedDb();
    seed(store, "michael", "u-1");
    seed(store, "quiet_name", "u-2");
    applyExpand(store);

    // Pre-hardening rows: 0038 no longer fabricates a skeleton for them, so the
    // contract guard would refuse right now. That refusal is the point.
    const unskeletonedBefore = contractGuardRowCount(store);
    expect(unskeletonedBefore).toBe(2);

    const plan = planBackfill({ directory: directoryRows(store) });
    expect(plan.refused).toBe(false);
    store.exec(plan.sql);

    const unskeletonedAfter = contractGuardRowCount(store);
    expect(unskeletonedAfter).toBe(0);

    // The whole point of the column: a confusable of a backfilled row is now
    // refused by `idx_username_directory_skeleton`, which is what a raw
    // skeleton could never do.
    store.exec("INSERT INTO users (user_id) VALUES ('u-3')");
    const claimConfusable = () => store.exec(
      `INSERT INTO username_directory
         (username, username_skeleton, display_username, user_id, friend_code, claimed_at, updated_at)
       VALUES ('michae1', '${usernameSkeleton("michae1")}', 'michae1', 'u-3', 'OSLFR1.code', 'now', 'now')`,
    );
    expect(claimConfusable).toThrow(/UNIQUE/i);

    // and a genuinely different name still lands
    const claimDistinct = () => store.exec(
      `INSERT INTO username_directory
         (username, username_skeleton, display_username, user_id, friend_code, claimed_at, updated_at)
       VALUES ('wholly_other', '${usernameSkeleton("wholly_other")}', 'wholly_other', 'u-3', 'OSLFR1.code', 'now', 'now')`,
    );
    expect(claimDistinct).not.toThrow();
    store.close();
  });
});

describe("D-248 · backfill CLI exit codes", () => {
  it("exits 0 on a clean plan, 3 on a collision, 2 on a bad invocation", () => {
    const dir = mkdtempSync(join(tmpdir(), "d248-backfill-"));
    try {
      const clean = join(dir, "clean.json");
      writeFileSync(clean, JSON.stringify([{ results: [
        { username: "michael", username_skeleton: "michael", display_username: "michael" },
      ] }]));
      const ok = runCli(["--directory", clean]);
      expect(ok.status).toBe(0);
      expect(ok.stdout).toContain("statements to apply     : 1");

      const collide = join(dir, "collide.json");
      writeFileSync(collide, JSON.stringify([{ results: [
        { username: "michael", username_skeleton: "michael", display_username: "michael" },
        { username: "michae1", username_skeleton: "michae1", display_username: "michae1" },
      ] }]));
      const refused = runCli(["--directory", collide]);
      expect(refused.status).toBe(3);
      expect(refused.stdout).toContain("COLLISION SET");
      expect(refused.stdout).toContain('"michae1", "michael"');
      expect(refused.stdout).toContain("REFUSED. No SQL was emitted");

      expect(runCli([]).status).toBe(2);
      expect(runCli(["--directory", join(dir, "missing.json")]).status).toBe(2);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
