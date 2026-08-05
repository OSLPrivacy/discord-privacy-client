/**
 * D-280 — the migration sequence must be provably contiguous.
 *
 * The defect was a gap nobody could see: `migrations/` ran 0015 then 0017, and
 * the only thing that ever noticed was a lane deriving a release-pin file list
 * for an unrelated reason, three days later. Migrations are applied IN ORDER
 * and the store records the NAMES it applied, so a gap means either a migration
 * was written and dropped without renumbering — a store that ran it now holds
 * a schema this tree does not describe — or a number was skipped and nothing
 * recorded it. From inside a repository you cannot tell which, which is exactly
 * why the sequence has to be asserted rather than assumed.
 *
 * Every spec below drives `assertMigrationSequenceContiguous` against a real
 * directory on disk. None of them reads the rule's source text: a test that
 * grepped the constant would pass against a function that returned early.
 */

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it } from "vitest";
import {
  assertMigrationSequenceContiguous,
  deriveReleaseSourceFiles,
  MIGRATION_SEQUENCE_SKIPS,
  MIGRATION_SKIP_REASON_MIN_CHARS,
  type MigrationSequenceSkip,
  readReleaseRoots,
  RELEASE_SOURCE_FILE_FLOOR,
} from "../scripts/d2-release-source-manifest.js";

const PROJECT_ROOT = fileURLToPath(new URL("../", import.meta.url));

const scratches: string[] = [];

/** A migrations directory built from bare numbers, on real disk. */
function fixture(dir: string, numbers: readonly number[]): string {
  const root = mkdtempSync(join(tmpdir(), "d2-migseq-"));
  scratches.push(root);
  mkdirSync(join(root, dir), { recursive: true });
  for (const number of numbers) {
    const name = `${String(number).padStart(4, "0")}_fixture_step.sql`;
    writeFileSync(join(root, dir, name), "SELECT 1;\n");
  }
  return root;
}

function skip(over: Partial<MigrationSequenceSkip>): MigrationSequenceSkip {
  return {
    dir: "db",
    number: 3,
    reason: "r".repeat(MIGRATION_SKIP_REASON_MIN_CHARS + 10),
    ...over,
  };
}

afterEach(() => {
  while (scratches.length > 0) {
    rmSync(scratches.pop()!, { recursive: true, force: true });
  }
});

describe("D2 migration sequence is contiguous", () => {
  it("passes on the tree it ships, with 0016 the one recorded gap", () => {
    // Not a tautology: the fixtures below prove the same call refuses. This
    // asserts the real directory is in the state the skip record claims.
    const roots = readReleaseRoots(PROJECT_ROOT);
    expect(roots.migrationDirs).toEqual(["migrations"]);
    expect(() => assertMigrationSequenceContiguous(
      PROJECT_ROOT,
      roots.migrationDirs,
    )).not.toThrow();

    // The recorded exception is exactly one entry, and it is D-280's.
    expect(MIGRATION_SEQUENCE_SKIPS.map((entry) => ({
      dir: entry.dir,
      number: entry.number,
    }))).toEqual([{ dir: "migrations", number: 16 }]);

    // And it is load-bearing: strip the record and the tree fails.
    expect(() => assertMigrationSequenceContiguous(
      PROJECT_ROOT,
      roots.migrationDirs,
      [],
    )).toThrow(/0016 is missing between 0015_.*and 0017_/s);
  });

  it("names the gap when a migration goes missing from the middle", () => {
    expect(() => assertMigrationSequenceContiguous(
      fixture("db", [1, 2, 4, 5]),
      ["db"],
    )).toThrow(/db is not contiguous: 0003 is missing between 0002_.*0004_/s);
  });

  it("refuses a migration numbered out of order", () => {
    // Adding 0009 to a sequence that ends at 0004 is the same defect from the
    // other direction: the numbers between it and the tail belong to nothing.
    expect(() => assertMigrationSequenceContiguous(
      fixture("db", [1, 2, 3, 4, 9]),
      ["db"],
    )).toThrow(/0005 is missing between 0004_.*0009_/s);
  });

  it("is green on a contiguous sequence, so it is not simply always red", () => {
    expect(() => assertMigrationSequenceContiguous(
      fixture("db", [1, 2, 3, 4, 5]),
      ["db"],
    )).not.toThrow();
    expect(() => assertMigrationSequenceContiguous(
      fixture("db", [1]),
      ["db"],
    )).not.toThrow();
  });

  it("refuses a sequence that does not start at 0001", () => {
    expect(() => assertMigrationSequenceContiguous(
      fixture("db", [2, 3, 4]),
      ["db"],
    )).toThrow(/starts at 0002, not 0001/);
  });

  it("refuses two migrations sharing a number, and unnumbered files", () => {
    const duplicate = fixture("db", [1, 2]);
    writeFileSync(join(duplicate, "db", "0002_other_name.sql"), "SELECT 1;\n");
    expect(() => assertMigrationSequenceContiguous(duplicate, ["db"]))
      .toThrow(/two migrations numbered 0002/);

    const stray = fixture("db", [1, 2]);
    writeFileSync(join(stray, "db", "README.md"), "notes\n");
    expect(() => assertMigrationSequenceContiguous(stray, ["db"]))
      .toThrow(/db\/README\.md is not a migration name/);
  });
});

/**
 * The skip table is an escape hatch, so it gets the harder half of the test
 * budget: each rule below is starved, not asserted. An exception mechanism that
 * cannot refuse an entry is an allowlist.
 */
describe("D2 migration skips are recorded, narrow and self-retiring", () => {
  it("closes its own gap and nothing else", () => {
    const root = fixture("db", [1, 2, 4]);
    expect(() => assertMigrationSequenceContiguous(root, ["db"], [
      skip({ number: 3 }),
    ])).not.toThrow();

    // One skip does not license the next hole.
    const wider = fixture("db", [1, 2, 5]);
    expect(() => assertMigrationSequenceContiguous(wider, ["db"], [
      skip({ number: 3 }),
    ])).toThrow(/0004 is missing between 0002_.*0005_/s);
  });

  it("cannot outlive the gap it records", () => {
    const root = fixture("db", [1, 2, 3]);
    expect(() => assertMigrationSequenceContiguous(root, ["db"], [
      skip({ number: 3 }),
    ])).toThrow(/0003_fixture_step\.sql exists, but 0003 is still recorded/);
  });

  it("cannot pre-authorise a gap outside the sequence", () => {
    const root = fixture("db", [1, 2, 3]);
    expect(() => assertMigrationSequenceContiguous(root, ["db"], [
      skip({ number: 9 }),
    ])).toThrow(/outside the sequence it holds \(0001-0003\)/);
  });

  it("cannot be admitted without a reason worth reading", () => {
    const root = fixture("db", [1, 2, 4]);
    for (const reason of ["", "   ", "n/a", "skipped", "x".repeat(
      MIGRATION_SKIP_REASON_MIN_CHARS - 1,
    )]) {
      expect(() => assertMigrationSequenceContiguous(root, ["db"], [
        skip({ number: 3, reason }),
      ])).toThrow(/gives no reason worth reading/);
    }
    // And the boundary is a boundary, not a wall: one more character passes.
    expect(() => assertMigrationSequenceContiguous(root, ["db"], [
      skip({ number: 3, reason: "x".repeat(MIGRATION_SKIP_REASON_MIN_CHARS) }),
    ])).not.toThrow();
  });

  it("refuses the same number recorded twice", () => {
    const root = fixture("db", [1, 2, 4]);
    expect(() => assertMigrationSequenceContiguous(root, ["db"], [
      skip({ number: 3 }),
      skip({ number: 3, reason: "a".repeat(200) }),
    ])).toThrow(/db\/0003 is recorded as skipped twice/);
  });

  it("only applies to the directory it names", () => {
    const root = fixture("db", [1, 2, 4]);
    expect(() => assertMigrationSequenceContiguous(root, ["db"], [
      skip({ dir: "other", number: 3 }),
    ])).toThrow(/db is not contiguous: 0003 is missing/);
  });
});

/**
 * Wiring. The rule is worthless if the release path does not run it, and the
 * release path is `deriveReleaseSourceFiles` — every consumer of the derived
 * set, including `scripts/d2-contract-gate.ts` and the digest itself, goes
 * through it. The scratch tree is padded past `RELEASE_SOURCE_FILE_FLOOR` on
 * purpose, so the failure that arrives is the gap and not the floor.
 */
describe("the release-source derivation refuses a gapped sequence", () => {
  function padded(numbers: readonly number[]): string {
    const root = fixture("db", numbers);
    mkdirSync(join(root, "src"));
    writeFileSync(join(root, "src", "index.ts"), "export default {};\n");
    for (let index = 0; index < RELEASE_SOURCE_FILE_FLOOR; index += 1) {
      writeFileSync(join(root, "src", `filler-${index}.ts`), "export {};\n");
    }
    writeFileSync(join(root, "package.json"), "{}\n");
    writeFileSync(join(root, "package-lock.json"), "{}\n");
    mkdirSync(join(root, "scripts"));
    writeFileSync(
      join(root, "scripts", "d2-release-source-manifest.ts"),
      "export {};\n",
    );
    writeFileSync(
      join(root, "wrangler.toml"),
      'main = "src/index.ts"\nmigrations_dir = "db"\n',
    );
    return root;
  }

  it("derives happily over a contiguous sequence", () => {
    const files = deriveReleaseSourceFiles(padded([1, 2, 3]));
    expect(files.length).toBeGreaterThanOrEqual(RELEASE_SOURCE_FILE_FLOOR);
    expect(files).toContain("db/0002_fixture_step.sql");
  });

  it("cannot produce a manifest for a sequence with a hole in it", () => {
    expect(() => deriveReleaseSourceFiles(padded([1, 2, 4])))
      .toThrow(/db is not contiguous: 0003 is missing/);
    // Specifically NOT the floor: the two gates are separate evidence.
    expect(() => deriveReleaseSourceFiles(padded([1, 2, 4])))
      .not.toThrow(/below the floor/);
  });
});
