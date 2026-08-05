/**
 * D2 release-source manifest DERIVATION.
 *
 * D-258. The pinned set used to be a hand-written 31-entry array. A list a
 * human maintains is omission-shaped: it is only ever wrong by being short, and
 * being short is silent. It cost this repo twice — ten source files and
 * migrations 0011-0017 were never inside it, so the `attachment-reserve.ts`
 * null-dereference fix changed the live fetch path without moving the digest by
 * a bit.
 *
 * The set is therefore no longer written down. It is derived from the file that
 * decides what actually deploys — `wrangler.toml`:
 *
 *   * `main` names the Worker entry point. Its top-level directory is a release
 *     root, walked whole.
 *   * every `migrations_dir` is a release root, walked whole. (Migrations are
 *     not bundled; they are applied to D1. They are release source anyway,
 *     because a Worker running against the wrong schema is the wrong release.)
 *   * `RELEASE_CONFIG_FILES` below adds the files that are release inputs
 *     without living under a root: the deploy config itself, the dependency
 *     manifest and its lockfile, and this deriver.
 *
 * A directory walk cannot omit a file that exists. What it CAN miss is a file
 * that ships from outside the roots, so `localModuleClosure` recomputes the
 * Worker's own relative-import graph from `main` and the contract test requires
 * that graph to be a subset of the derived set. Adding `lib/foo.ts` at the
 * project root and importing it from `src/index.ts` fails that check rather
 * than quietly shipping unpinned.
 *
 * This file is itself in `RELEASE_CONFIG_FILES`, so the derivation rule lives
 * inside the digest it derives: narrowing a root, dropping a config file, or
 * loosening the walk moves `D2_RELEASE_SOURCE_SHA256` and the contract test
 * goes red. The contract file that HOLDS the digest cannot be pinned by it —
 * a digest over its own declaration has no fixpoint — which is why the rule
 * lives here and only the value lives there.
 *
 * Residual, recorded rather than implied: this covers first-party source. It
 * does not pin `node_modules` bytes (the lockfile stands in for those), files
 * pulled in by a computed dynamic `import()`, or anything a deploy pipeline
 * injects outside `wrangler.toml`.
 */

import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, posix } from "node:path";
import ts from "typescript";

/**
 * Release inputs that do not live under a walked root. `main` and
 * `migrations_dir` come from wrangler.toml and are never listed here.
 */
export const RELEASE_CONFIG_FILES = [
  "package-lock.json",
  "package.json",
  "scripts/d2-release-source-manifest.ts",
  "wrangler.toml",
] as const;

/**
 * A floor, not a count: it only has to be low enough that every legitimate
 * addition leaves it true, and high enough that a walk which silently returned
 * nothing (wrong root, unreadable directory, a "tidied" exclude) cannot pass.
 * A gate that cannot fail is decoration — this one fails when starved.
 */
export const RELEASE_SOURCE_FILE_FLOOR = 45;

/**
 * The name shape wrangler applies. `wrangler d1 migrations apply` runs EVERY
 * file in `migrations_dir` in name order and records the NAME of each one it
 * ran in the database's own `d1_migrations` table. Both halves matter here: a
 * file whose name does not carry a number still gets applied, and a number
 * that is missing from the tree may still be sitting in a store's applied list.
 */
const MIGRATION_FILENAME = /^(\d{4})_[a-z0-9]+(?:_[a-z0-9]+)*\.sql$/;

/**
 * A recorded, deliberate hole in the sequence.
 *
 * D-280 asks whether a skip should be expressible at all, and the answer is
 * yes — but only as the most expensive edit in this project, and never as
 * silence. The reasoning, because the opposite choice is the obvious one:
 *
 *   * **The alternatives for a gap that already exists are worse.** Renumbering
 *     the migrations above it rewrites names a deployed store may already have
 *     recorded as applied, and no repository can tell you whether it did. A
 *     placeholder no-op migration is worse still — it is a name the store
 *     certainly has NOT applied, so it manufactures exactly the drift this
 *     check exists to detect. A gate whose only remedy is an unsafe act is a
 *     gate that gets deleted, and a deleted gate catches nothing.
 *   * **The hatch is not cheap.** This file is inside
 *     `D2_RELEASE_SOURCE_SHA256`, so adding an entry moves the release digest
 *     and must be accounted for byte by byte in the re-anchor ledger. Recording
 *     a skip therefore costs strictly more than closing the gap properly.
 *   * **It cannot rot into an allowlist.** A skip for a number that EXISTS on
 *     disk fails, a skip outside the observed range fails, and a skip without a
 *     substantive reason fails. Entries cannot accumulate as decoration, cannot
 *     pre-authorise a future gap, and cannot survive the gap being closed.
 */
export interface MigrationSequenceSkip {
  /** Migrations directory, relative to the project root, POSIX separators. */
  readonly dir: string;
  /** The number that is deliberately absent. */
  readonly number: number;
  /** Why it is absent, and what was established. The gap records nothing. */
  readonly reason: string;
}

/**
 * A reason has to carry the finding, not a shrug. `""`, `"n/a"` and `"skipped"`
 * are the failure D-280 is about, written down.
 */
export const MIGRATION_SKIP_REASON_MIN_CHARS = 80;

export const MIGRATION_SEQUENCE_SKIPS: readonly MigrationSequenceSkip[] = [
  {
    dir: "migrations",
    number: 16,
    reason:
      "D-280. 0016 WAS written and is NOT a numbering slip: commit d0f47c6c "
      + '"T2-40 reserve single-fetch blobs before serving", 2026-08-02, added '
      + "migrations/0016_blob_fetch_reservations.sql on the LOCAL branch ch3. "
      + "ch3 was never pushed to any remote and d0f47c6c is an ancestor of "
      + "neither this branch nor origin/main. Its child commit T2-41 numbered "
      + "itself 0017 on top of it and was cherry-picked into the integration "
      + "line ALONE (cba9fc41f, parent 230ae1bb, committed 90 minutes later), "
      + "so the number was consumed by a commit that never arrived. The 0016 "
      + "columns (single_fetch, reserved_until on blob_capability_index) are "
      + "named by no file in this tree and its endpoint blob-reserve.ts is "
      + "absent too, so the tree is internally consistent. WHAT A DEPLOYED "
      + "STORE APPLIED IS NOT DETERMINABLE FROM THIS REPOSITORY: it needs that "
      + "database's d1_migrations table. The number therefore stays burned. "
      + "Renumbering 0017 would rewrite a name a store may already hold as "
      + "applied; a placeholder 0016 would be a name it certainly does not.",
  },
];

export interface ReleaseRoots {
  /** Worker entry point, relative to the project root, POSIX separators. */
  main: string;
  /** Directories walked whole, relative to the project root. */
  roots: string[];
  /** The subset of `roots` that wrangler applies to D1, in declared order. */
  migrationDirs: string[];
}

function fail(message: string): never {
  throw new Error(`D2 release source manifest: ${message}`);
}

function pad(value: number): string {
  return String(value).padStart(4, "0");
}

function tomlStrings(toml: string, key: string): string[] {
  const values: string[] = [];
  for (const line of toml.split(/\r?\n/)) {
    const match = new RegExp(`^\\s*${key}\\s*=\\s*"([^"]+)"\\s*$`).exec(line);
    if (match) values.push(match[1]!);
  }
  return values;
}

/** Read the deploy inputs that decide which trees are release source. */
export function readReleaseRoots(projectRoot: string): ReleaseRoots {
  const toml = readFileSync(join(projectRoot, "wrangler.toml"), "utf8");
  const mains = tomlStrings(toml, "main");
  if (mains.length !== 1) {
    fail("wrangler.toml must declare exactly one `main` entry point");
  }
  const main = mains[0]!.replace(/\\/g, "/").replace(/^\.\//, "");
  const migrationDirs = tomlStrings(toml, "migrations_dir")
    .map((value) => value.replace(/\\/g, "/").replace(/^\.\//, ""));
  if (migrationDirs.length === 0) {
    fail("wrangler.toml declares no `migrations_dir`");
  }
  const mainRoot = main.split("/")[0]!;
  if (mainRoot === main) fail(`\`main\` (${main}) is not inside a directory`);
  const roots = [...new Set([mainRoot, ...migrationDirs])].sort();
  for (const root of roots) {
    if (!statSync(join(projectRoot, root)).isDirectory()) {
      fail(`release root ${root} is not a directory`);
    }
  }
  return { main, roots, migrationDirs };
}

/**
 * D-280. Refuse a migrations directory whose numbering is not contiguous.
 *
 * Migrations are applied IN ORDER and the store records the names it ran, so a
 * gap is never cosmetic. It means one of exactly two things, and from inside a
 * repository you cannot tell which:
 *
 *   * a migration was written and later dropped WITHOUT renumbering, in which
 *     case a store that already ran it holds columns this tree does not
 *     describe — schema drift with no record of itself; or
 *   * the number was simply skipped, in which case nothing anywhere says so and
 *     the next person to notice re-investigates from scratch.
 *
 * Both happened here at once and neither was caught, because nothing asserted
 * contiguity: 0016 was written on a branch that never merged, and 0017, which
 * had numbered itself on top of it, was cherry-picked in alone. It surfaced
 * three days later as a side observation by a lane deriving a release-pin file
 * list for an unrelated reason. That is the failure mode this closes — a gap
 * now fails at the moment it is introduced, naming the number.
 *
 * NOT covered here, deliberately: an EMPTY migrations directory has no numbers
 * and therefore no gaps. Losing the whole directory is what
 * `RELEASE_SOURCE_FILE_FLOOR` is for, and the two are kept separate so neither
 * can be satisfied by the other's evidence.
 */
export function assertMigrationSequenceContiguous(
  projectRoot: string,
  migrationDirs: readonly string[],
  // Defaulted, not injected at the call site: production always gets the pinned
  // table, and a test can still starve the skip rules with a table of its own
  // to prove they refuse rather than merely exist.
  recordedSkips: readonly MigrationSequenceSkip[] = MIGRATION_SEQUENCE_SKIPS,
): void {
  const seenSkips = new Set<string>();
  for (const skip of recordedSkips) {
    const key = `${skip.dir}/${pad(skip.number)}`;
    if (seenSkips.has(key)) fail(`${key} is recorded as skipped twice`);
    seenSkips.add(key);
    if (!Number.isInteger(skip.number) || skip.number < 1) {
      fail(`recorded skip ${key} is not a migration number`);
    }
    if (skip.reason.trim().length < MIGRATION_SKIP_REASON_MIN_CHARS) {
      fail(
        `recorded skip ${key} gives no reason worth reading `
        + `(${skip.reason.trim().length} characters, minimum `
        + `${MIGRATION_SKIP_REASON_MIN_CHARS}). A gap that is admitted without `
        + "the finding behind it is the defect, written down",
      );
    }
  }

  for (const dir of migrationDirs) {
    const byNumber = new Map<number, string>();
    const names = readdirSync(join(projectRoot, dir), { withFileTypes: true })
      .filter((entry) => entry.isFile())
      .map((entry) => entry.name)
      .sort();
    for (const name of names) {
      const match = MIGRATION_FILENAME.exec(name);
      if (match === null) {
        fail(
          `${dir}/${name} is not a migration name. wrangler applies EVERY file `
          + "in `migrations_dir` in name order, so anything outside "
          + "NNNN_lower_snake_case.sql either ships as an unnumbered migration "
          + "or hides a gap from this check",
        );
      }
      const number = Number(match[1]);
      const existing = byNumber.get(number);
      if (existing !== undefined) {
        fail(
          `${dir} holds two migrations numbered ${match[1]}: ${existing} and `
          + `${name}. Which one a store recorded as applied is then decided by `
          + "name order rather than by intent, and applying both to a store "
          + "that ran only one is not the same operation twice",
        );
      }
      byNumber.set(number, name);
    }
    if (byNumber.size === 0) continue;

    const numbers = [...byNumber.keys()].sort((a, b) => a - b);
    const first = numbers[0]!;
    const last = numbers[numbers.length - 1]!;
    if (first !== 1) {
      fail(
        `${dir} starts at ${pad(first)}, not 0001. The first migration builds `
        + "the schema every later one alters; a sequence that starts above 1 "
        + "cannot be applied to an empty database",
      );
    }

    const skips = new Map(
      recordedSkips
        .filter((skip) => skip.dir === dir)
        .map((skip) => [skip.number, skip] as const),
    );
    for (const [number] of skips) {
      const present = byNumber.get(number);
      if (present !== undefined) {
        fail(
          `${dir}/${present} exists, but ${pad(number)} is still recorded as a `
          + "deliberate skip. A skip that outlives its gap is an exception "
          + "nobody is reading — delete the entry",
        );
      }
      if (number < first || number > last) {
        fail(
          `${dir} records a skip for ${pad(number)}, which is outside the `
          + `sequence it holds (${pad(first)}-${pad(last)}). A skip may record `
          + "a gap that exists; it may not pre-authorise one",
        );
      }
    }

    for (let number = first; number <= last; number += 1) {
      if (byNumber.has(number) || skips.has(number)) continue;
      const below = byNumber.get(Math.max(...numbers.filter((n) => n < number)));
      const above = byNumber.get(Math.min(...numbers.filter((n) => n > number)));
      fail(
        `${dir} is not contiguous: ${pad(number)} is missing between ${below} `
        + `and ${above}. Migrations are applied in order and a store records `
        + "the NAMES it applied, so this is either a migration that was "
        + "written and dropped without renumbering — which a deployed store "
        + "may already have run, leaving a schema this tree does not describe "
        + "— or a number skipped with nothing recording it. Do NOT close it "
        + "with a placeholder migration: that is a name no store has applied, "
        + "and it manufactures the drift this check exists to catch. Establish "
        + "from history which happened, then either restore the file or record "
        + "it in MIGRATION_SEQUENCE_SKIPS with the evidence",
      );
    }
  }
}

function walk(projectRoot: string, relative: string, out: string[]): string[] {
  const entries = readdirSync(join(projectRoot, relative), {
    withFileTypes: true,
  }).sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
  for (const entry of entries) {
    const child = posix.join(relative, entry.name);
    if (entry.isDirectory()) walk(projectRoot, child, out);
    else if (entry.isFile()) out.push(child);
  }
  return out;
}

/**
 * Every release-source path, sorted, with POSIX separators. No exclusions: a
 * derivation with an exclude list is a hand-written list again, one negation
 * further away from being read.
 */
export function deriveReleaseSourceFiles(projectRoot: string): string[] {
  const { roots, migrationDirs } = readReleaseRoots(projectRoot);
  const walked: string[] = [];
  for (const root of roots) walk(projectRoot, root, walked);
  const files = [...new Set([...walked, ...RELEASE_CONFIG_FILES])].sort();
  if (files.length < RELEASE_SOURCE_FILE_FLOOR) {
    fail(
      `derived ${files.length} release files, below the floor of `
      + `${RELEASE_SOURCE_FILE_FLOOR}; the walk found less source than exists`,
    );
  }
  // D-280. Deliberately here rather than at a call site: the release digest
  // cannot be computed over a gapped sequence at all, so every consumer of the
  // derived set enforces it and there is no single call to delete. Both the
  // rule and this invocation live in a file that is inside the digest, so
  // removing either moves D2_RELEASE_SOURCE_SHA256 and the contract goes red.
  assertMigrationSequenceContiguous(projectRoot, migrationDirs);
  return files;
}

/**
 * Digest over (path, byte length, bytes) for each file in order. Unchanged from
 * the hand-listed era on purpose: only the membership rule moved, so a digest
 * recomputed over the old list still reproduces the old value, and the ledger
 * for the re-anchor can attribute the move to set expansion alone.
 */
export function releaseSourceManifestSha256(
  projectRoot: string,
  files: readonly string[],
): string {
  const manifest = createHash("sha256");
  for (const relative of files) {
    const bytes = readFileSync(join(projectRoot, relative));
    manifest.update(relative);
    manifest.update("\0");
    manifest.update(String(bytes.byteLength));
    manifest.update("\0");
    manifest.update(bytes);
  }
  return manifest.digest("hex");
}

function resolveLocal(projectRoot: string, relative: string): string | null {
  const candidates = [relative];
  if (relative.endsWith(".js")) candidates.push(`${relative.slice(0, -3)}.ts`);
  if (relative.endsWith(".mjs")) candidates.push(`${relative.slice(0, -4)}.mts`);
  if (!/\.[a-z]+$/.test(relative)) {
    candidates.push(`${relative}.ts`, `${relative}.js`, `${relative}/index.ts`);
  }
  for (const candidate of candidates) {
    try {
      if (statSync(join(projectRoot, candidate)).isFile()) return candidate;
    } catch {
      // not this candidate
    }
  }
  return null;
}

function relativeSpecifiers(projectRoot: string, relative: string): string[] {
  const source = ts.createSourceFile(
    relative,
    readFileSync(join(projectRoot, relative), "utf8"),
    ts.ScriptTarget.Latest,
    true,
    relative.endsWith(".ts") ? ts.ScriptKind.TS : ts.ScriptKind.JS,
  );
  const specifiers: string[] = [];
  const collect = (value: string): void => {
    if (value.startsWith(".")) specifiers.push(value);
  };
  const visit = (node: ts.Node): void => {
    if (
      (ts.isImportDeclaration(node) || ts.isExportDeclaration(node))
      && node.moduleSpecifier
      && ts.isStringLiteral(node.moduleSpecifier)
    ) {
      collect(node.moduleSpecifier.text);
    }
    if (
      ts.isCallExpression(node)
      && node.expression.kind === ts.SyntaxKind.ImportKeyword
      && node.arguments[0]
      && ts.isStringLiteral(node.arguments[0])
    ) {
      collect((node.arguments[0] as ts.StringLiteral).text);
    }
    node.forEachChild(visit);
  };
  visit(source);
  return specifiers;
}

/**
 * Every first-party module reachable from `entry` by relative import, sorted.
 * Bare specifiers stop the walk: those are npm dependencies, and the lockfile
 * is what pins them.
 */
export function localModuleClosure(
  projectRoot: string,
  entry: string,
): string[] {
  const seen = new Set<string>();
  const queue = [entry];
  while (queue.length > 0) {
    const current = queue.pop()!;
    if (seen.has(current)) continue;
    seen.add(current);
    const directory = posix.dirname(current);
    for (const specifier of relativeSpecifiers(projectRoot, current)) {
      const target = resolveLocal(projectRoot, posix.join(directory, specifier));
      if (target === null) {
        fail(`${current} imports ${specifier}, which resolves to no file`);
      }
      if (!seen.has(target)) queue.push(target);
    }
  }
  return [...seen].sort();
}
