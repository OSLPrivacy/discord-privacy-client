/**
 * D-287. Every spec here drives a real directory on disk or the real project —
 * none of them reads the rule's own source text, so a rule that returned early
 * would fail them rather than satisfy them (D-285).
 *
 * The order-consequence specs are the load-bearing half. Each recorded duplicate
 * in `MIGRATION_NUMBER_DUPLICATES` claims its run order is irrelevant; these
 * specs PROVE it by applying the whole real sequence to real SQLite twice, once
 * in wrangler's name order and once with that pair swapped, and comparing
 * `sqlite_master`. The last spec starves that harness — a pair that genuinely
 * depends on order must make it fail — because a proof that cannot fail is not
 * one.
 */
import { DatabaseSync } from "node:sqlite";
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, describe, expect, it } from "vitest";

import {
  assertMigrationSequences,
  assertProjectMigrationSequences,
  MIGRATION_NUMBER_DUPLICATES,
  MIGRATION_SEQUENCE_ORIGINS,
  MIGRATION_SEQUENCE_SKIPS,
  MIGRATION_SKIP_REASON_MIN_CHARS,
  readMigrationDirs,
  type MigrationNumberDuplicate,
  type MigrationSequenceSkip,
} from "./migration-sequence.js";

const KEYSERVER_ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const MIGRATIONS = join(KEYSERVER_ROOT, "migrations");

const scratch: string[] = [];
afterAll(() => {
  for (const dir of scratch) rmSync(dir, { force: true, recursive: true });
});

/** A project root holding one or more migrations dirs and a wrangler config. */
function fixture(dirs: Record<string, readonly string[]>): string {
  const root = mkdtempSync(join(tmpdir(), "d287-"));
  scratch.push(root);
  let toml = 'name = "fixture"\nmain = "src/index.ts"\n';
  for (const [dir, names] of Object.entries(dirs)) {
    mkdirSync(join(root, dir), { recursive: true });
    for (const name of names) {
      writeFileSync(join(root, dir, name), "-- fixture\n");
    }
    toml += `\n[[d1_databases]]\nbinding = "DB"\nmigrations_dir = "${dir}"\n`;
  }
  writeFileSync(join(root, "wrangler.toml"), toml);
  return root;
}

const ORIGIN_1 = { migrations: 1 } as const;
const REASON = "x".repeat(MIGRATION_SKIP_REASON_MIN_CHARS);

function duplicate(
  number: number,
  files: readonly string[],
  overrides: Partial<MigrationNumberDuplicate> = {},
): MigrationNumberDuplicate {
  return { dir: "migrations", number, files, reason: REASON, ...overrides };
}

function skip(
  number: number,
  overrides: Partial<MigrationSequenceSkip> = {},
): MigrationSequenceSkip {
  return { dir: "migrations", number, reason: REASON, ...overrides };
}

/** Apply .sql files in the given order to a fresh in-memory SQLite. */
function schemaAfter(order: readonly string[]): string {
  const db = new DatabaseSync(":memory:");
  try {
    db.exec("PRAGMA foreign_keys = ON;");
    for (const name of order) {
      db.exec(readFileSync(join(MIGRATIONS, name), "utf8"));
    }
    return JSON.stringify(
      db
        .prepare(
          "SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY type, name",
        )
        .all(),
    );
  } finally {
    db.close();
  }
}

function nameOrder(): string[] {
  return readdirSync(MIGRATIONS).filter((n) => n.endsWith(".sql")).sort();
}

function withSwapped(order: readonly string[], a: string, b: string): string[] {
  const out = [...order];
  const ia = out.indexOf(a);
  const ib = out.indexOf(b);
  expect(ia, `${a} present`).toBeGreaterThanOrEqual(0);
  expect(ib, `${b} present`).toBeGreaterThanOrEqual(0);
  out[ia] = b;
  out[ib] = a;
  return out;
}

describe("the real keyserver tree", () => {
  it("passes the rule with the recorded exceptions", () => {
    expect(() => assertProjectMigrationSequences(KEYSERVER_ROOT)).not.toThrow();
  });

  it("derives both migrations dirs from the wrangler configs", () => {
    expect(readMigrationDirs(KEYSERVER_ROOT)).toEqual([
      "migrations",
      "migrations-contract",
    ]);
  });

  it("pins the ordinary sequence to start at 0001", () => {
    expect(MIGRATION_SEQUENCE_ORIGINS["migrations"]).toBe(1);
  });

  it("records exactly the holes that exist, and no others", () => {
    expect(MIGRATION_NUMBER_DUPLICATES.map((d) => d.number)).toEqual([25, 26, 38, 44, 45, 46]);
    expect(MIGRATION_SEQUENCE_SKIPS.map((s) => s.number)).toEqual([42]);
  });
});

describe("run order of each recorded duplicate", () => {
  it("applies the whole real sequence in wrangler's name order", () => {
    const order = nameOrder();
    expect(order.length).toBeGreaterThan(40);
    expect(() => schemaAfter(order)).not.toThrow();
  });

  for (const recorded of MIGRATION_NUMBER_DUPLICATES) {
    const [first, ...rest] = recorded.files;
    for (const other of rest) {
      it(`is irrelevant for ${recorded.number}: swapping ${first} and ${other} yields one schema`, () => {
        const base = schemaAfter(nameOrder());
        const swapped = schemaAfter(withSwapped(nameOrder(), first!, other));
        expect(swapped).toBe(base);
      });
    }
  }

  it("would NOT report irrelevance for a pair that depends on order", () => {
    // Starve it: a mutant pair where the second file needs the first's table.
    const root = mkdtempSync(join(tmpdir(), "d287-order-"));
    scratch.push(root);
    writeFileSync(join(root, "a.sql"), "CREATE TABLE dependent_probe (x);\n");
    writeFileSync(join(root, "b.sql"), "ALTER TABLE dependent_probe ADD COLUMN y;\n");
    const apply = (order: string[]) => {
      const db = new DatabaseSync(":memory:");
      try {
        for (const n of order) db.exec(readFileSync(join(root, n), "utf8"));
      } finally {
        db.close();
      }
    };
    expect(() => apply(["a.sql", "b.sql"])).not.toThrow();
    expect(() => apply(["b.sql", "a.sql"])).toThrow(/no such table/i);
  });
});

describe("it refuses", () => {
  it("an unrecorded duplicate, naming the number and every colliding file", () => {
    const root = fixture({
      migrations: ["0001_a.sql", "0002_b.sql", "0002_c.sql"],
    });
    expect(() => assertMigrationSequences(root, ["migrations"], [], [], ORIGIN_1))
      .toThrow(/holds 2 migrations numbered 0002: 0002_b\.sql, 0002_c\.sql/);
  });

  it("a gap, naming the missing number and both neighbours", () => {
    const root = fixture({ migrations: ["0001_a.sql", "0003_c.sql"] });
    expect(() => assertMigrationSequences(root, ["migrations"], [], [], ORIGIN_1))
      .toThrow(/not contiguous: 0002 is missing between 0001_a\.sql and 0003_c\.sql/);
  });

  it("a .sql file outside NNNN_lower_snake_case.sql", () => {
    const root = fixture({ migrations: ["0001_a.sql", "not-a-migration.sql"] });
    expect(() => assertMigrationSequences(root, ["migrations"], [], [], ORIGIN_1))
      .toThrow(/is not a migration name/);
  });

  it("a backup left beside the migrations", () => {
    const root = fixture({ migrations: ["0001_a.sql", "0002_b.sql.bak"] });
    expect(() => assertMigrationSequences(root, ["migrations"], [], [], ORIGIN_1))
      .toThrow(/is not a migration name/);
  });

  it("a sequence that does not start at its recorded origin", () => {
    const root = fixture({ migrations: ["0002_a.sql", "0003_b.sql"] });
    expect(() => assertMigrationSequences(root, ["migrations"], [], [], ORIGIN_1))
      .toThrow(/starts at 0002, not 0001/);
  });

  it("a migrations dir with no recorded origin at all", () => {
    const root = fixture({ "migrations-new": ["0001_a.sql"] });
    expect(() =>
      assertMigrationSequences(root, ["migrations-new"], [], [], ORIGIN_1),
    ).toThrow(/has no recorded sequence origin/);
  });

  it("the same NAME in two directories applied to one database", () => {
    const root = fixture({
      migrations: ["0001_a.sql"],
      "migrations-contract": ["0001_a.sql"],
    });
    expect(() =>
      assertMigrationSequences(root, ["migrations", "migrations-contract"], [], [], {
        migrations: 1,
        "migrations-contract": 1,
      }),
    ).toThrow(/the same migration NAME in two directories/);
  });

  it("two directories whose number ranges overlap", () => {
    const root = fixture({
      migrations: ["0001_a.sql", "0002_b.sql"],
      "migrations-contract": ["0002_z.sql"],
    });
    expect(() =>
      assertMigrationSequences(root, ["migrations", "migrations-contract"], [], [], {
        migrations: 1,
        "migrations-contract": 2,
      }),
    ).toThrow(/0002 is used by both migrations and migrations-contract/);
  });
});

describe("a recorded duplicate", () => {
  const dupFiles = ["0002_b.sql", "0002_c.sql"];
  const dupTree = ["0001_a.sql", ...dupFiles];

  it("permits exactly the collision it names", () => {
    const root = fixture({ migrations: dupTree });
    expect(() =>
      assertMigrationSequences(
        root,
        ["migrations"],
        [],
        [duplicate(2, dupFiles)],
        ORIGIN_1,
      ),
    ).not.toThrow();
  });

  it("SELF-RETIRES: it fails the moment the collision is resolved", () => {
    const root = fixture({ migrations: ["0001_a.sql", "0002_b.sql", "0003_c.sql"] });
    expect(() =>
      assertMigrationSequences(
        root,
        ["migrations"],
        [],
        [duplicate(2, dupFiles)],
        ORIGIN_1,
      ),
    ).toThrow(/holds 1 migration\(s\) numbered 0002 \(0002_b\.sql\), but 0002 is still recorded as a duplicate/);
  });

  it("fails when a THIRD file joins the collision it pinned", () => {
    const root = fixture({ migrations: [...dupTree, "0002_d.sql"] });
    expect(() =>
      assertMigrationSequences(
        root,
        ["migrations"],
        [],
        [duplicate(2, dupFiles)],
        ORIGIN_1,
      ),
    ).toThrow(/but disk holds 0002_b\.sql, 0002_c\.sql, 0002_d\.sql/);
  });

  it("fails when a file inside the collision is renamed", () => {
    const root = fixture({ migrations: ["0001_a.sql", "0002_b.sql", "0002_e.sql"] });
    expect(() =>
      assertMigrationSequences(
        root,
        ["migrations"],
        [],
        [duplicate(2, dupFiles)],
        ORIGIN_1,
      ),
    ).toThrow(/but disk holds 0002_b\.sql, 0002_e\.sql/);
  });

  it("does not license the NEXT unrecorded collision", () => {
    const root = fixture({
      migrations: [...dupTree, "0003_d.sql", "0003_e.sql"],
    });
    expect(() =>
      assertMigrationSequences(
        root,
        ["migrations"],
        [],
        [duplicate(2, dupFiles)],
        ORIGIN_1,
      ),
    ).toThrow(/holds 2 migrations numbered 0003/);
  });

  it("cannot be recorded twice", () => {
    const root = fixture({ migrations: dupTree });
    expect(() =>
      assertMigrationSequences(
        root,
        ["migrations"],
        [],
        [duplicate(2, dupFiles), duplicate(2, dupFiles)],
        ORIGIN_1,
      ),
    ).toThrow(/migrations\/0002 is recorded as a duplicate twice/);
  });

  it("cannot name fewer than two files", () => {
    const root = fixture({ migrations: dupTree });
    expect(() =>
      assertMigrationSequences(
        root,
        ["migrations"],
        [],
        [duplicate(2, ["0002_b.sql"])],
        ORIGIN_1,
      ),
    ).toThrow(/names 1 file\(s\)/);
  });

  it("cannot name its files out of order or twice", () => {
    const root = fixture({ migrations: dupTree });
    expect(() =>
      assertMigrationSequences(
        root,
        ["migrations"],
        [],
        [duplicate(2, ["0002_c.sql", "0002_b.sql"])],
        ORIGIN_1,
      ),
    ).toThrow(/must name each colliding file once, in name order/);
    expect(() =>
      assertMigrationSequences(
        root,
        ["migrations"],
        [],
        [duplicate(2, ["0002_b.sql", "0002_b.sql"])],
        ORIGIN_1,
      ),
    ).toThrow(/must name each colliding file once, in name order/);
  });

  it("cannot also be recorded as a deliberate skip", () => {
    const root = fixture({ migrations: dupTree });
    expect(() =>
      assertMigrationSequences(
        root,
        ["migrations"],
        [skip(2)],
        [duplicate(2, dupFiles)],
        ORIGIN_1,
      ),
    ).toThrow(/recorded as BOTH a deliberate skip and a recorded duplicate/);
  });

  it("demands a reason of substance, and the minimum is a real boundary", () => {
    const root = fixture({ migrations: dupTree });
    const run = (reason: string) =>
      assertMigrationSequences(
        root,
        ["migrations"],
        [],
        [duplicate(2, dupFiles, { reason })],
        ORIGIN_1,
      );
    for (const bad of ["", "   ", "n/a", "duplicate", "x".repeat(MIGRATION_SKIP_REASON_MIN_CHARS - 1)]) {
      expect(() => run(bad), JSON.stringify(bad)).toThrow(/gives no reason worth reading/);
    }
    expect(() => run("x".repeat(MIGRATION_SKIP_REASON_MIN_CHARS))).not.toThrow();
  });
});

describe("a recorded skip", () => {
  it("permits exactly the gap it names", () => {
    const root = fixture({ migrations: ["0001_a.sql", "0003_c.sql"] });
    expect(() =>
      assertMigrationSequences(root, ["migrations"], [skip(2)], [], ORIGIN_1),
    ).not.toThrow();
  });

  it("SELF-RETIRES: it fails the moment the gap is closed", () => {
    const root = fixture({ migrations: ["0001_a.sql", "0002_b.sql", "0003_c.sql"] });
    expect(() =>
      assertMigrationSequences(root, ["migrations"], [skip(2)], [], ORIGIN_1),
    ).toThrow(/0002 is still recorded as a deliberate skip/);
  });

  it("may not pre-authorise a gap outside the observed range", () => {
    const root = fixture({ migrations: ["0001_a.sql", "0002_b.sql"] });
    expect(() =>
      assertMigrationSequences(root, ["migrations"], [skip(9)], [], ORIGIN_1),
    ).toThrow(/outside the sequence it holds \(0001-0002\)/);
  });

  it("cannot be recorded twice", () => {
    const root = fixture({ migrations: ["0001_a.sql", "0003_c.sql"] });
    expect(() =>
      assertMigrationSequences(root, ["migrations"], [skip(2), skip(2)], [], ORIGIN_1),
    ).toThrow(/migrations\/0002 is recorded as skipped twice/);
  });

  it("does not license the NEXT hole", () => {
    const root = fixture({ migrations: ["0001_a.sql", "0003_c.sql", "0005_e.sql"] });
    expect(() =>
      assertMigrationSequences(root, ["migrations"], [skip(2)], [], ORIGIN_1),
    ).toThrow(/0004 is missing between 0003_c\.sql and 0005_e\.sql/);
  });

  it("demands a reason of substance, and the minimum is a real boundary", () => {
    const root = fixture({ migrations: ["0001_a.sql", "0003_c.sql"] });
    const run = (reason: string) =>
      assertMigrationSequences(root, ["migrations"], [skip(2, { reason })], [], ORIGIN_1);
    for (const bad of ["", "  ", "n/a", "skipped", "x".repeat(MIGRATION_SKIP_REASON_MIN_CHARS - 1)]) {
      expect(() => run(bad), JSON.stringify(bad)).toThrow(/gives no reason worth reading/);
    }
    expect(() => run("x".repeat(MIGRATION_SKIP_REASON_MIN_CHARS))).not.toThrow();
  });
});

describe("it accepts", () => {
  it("a contiguous, unique sequence", () => {
    const root = fixture({ migrations: ["0001_a.sql", "0002_b.sql", "0003_c.sql"] });
    expect(() => assertMigrationSequences(root, ["migrations"], [], [], ORIGIN_1))
      .not.toThrow();
  });

  it("a single migration", () => {
    const root = fixture({ migrations: ["0001_a.sql"] });
    expect(() => assertMigrationSequences(root, ["migrations"], [], [], ORIGIN_1))
      .not.toThrow();
  });

  it("a README beside the migrations, and nothing else non-.sql", () => {
    const root = fixture({ migrations: ["0001_a.sql", "README.md"] });
    expect(() => assertMigrationSequences(root, ["migrations"], [], [], ORIGIN_1))
      .not.toThrow();
    const bad = fixture({ migrations: ["0001_a.sql", "NOTES.md"] });
    expect(() => assertMigrationSequences(bad, ["migrations"], [], [], ORIGIN_1))
      .toThrow(/is not a migration name/);
  });

  it("an EMPTY directory — losing a whole directory is a different failure", () => {
    const root = fixture({ migrations: [] });
    expect(() => assertMigrationSequences(root, ["migrations"], [], [], ORIGIN_1))
      .not.toThrow();
  });
});

// WIRING IS NOT ASSERTED HERE, DELIBERATELY.
//
// The obvious spec — read `vitest.config.ts` and require it to contain
// `assertProjectMigrationSequences(__dirname)` — was written and then deleted.
// It is a text assertion about a call it never makes, which is the shape D-285
// records: a checker satisfied by the presence of a string cannot tell a live
// call from a commented-out one. The wiring is proved BEHAVIOURALLY instead, in
// the lane's mutation table: with a fourth duplicate on disk, and again with a
// recorded exception removed — an
// ordinary worker-pool spec that touches none of this — exits 1 with the rule's
// own message. That is the whole worker pool refusing, and no string can fake it.
