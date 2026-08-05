/**
 * D-287 / D-280 / D-288 — the migration sequence rule, at the keyserver.
 *
 * Ported unchanged in shape from `cipher-store-cf/scripts/d2-release-source-
 * manifest.ts` (`fix/d280-migration-0016-gap`), which wrote it to take
 * `(projectRoot, migrationDirs)` and said it ports without change. It does. The
 * one thing this copy ADDS is a second recorded-exception kind, because the
 * keyserver's holes are duplicates rather than gaps and the original refused
 * duplicates outright, with no way to record one. The argument for that hatch is
 * the original's own argument for skips, and it is set out at
 * `MIGRATION_NUMBER_DUPLICATES` below.
 *
 * WHAT WRANGLER ACTUALLY DOES, because every rule here follows from it:
 * `wrangler d1 migrations apply` reads EVERY file in `migrations_dir`, sorts by
 * NAME, applies the ones the database's own `d1_migrations` table does not
 * already list, and records the NAME of each one it ran. Three consequences:
 *
 *   * a file whose name carries no number still gets applied;
 *   * a number missing from the tree may still be sitting in a store's applied
 *     list, so a gap is never cosmetic;
 *   * two files sharing a number are ordered against each other by LEXICAL
 *     ACCIDENT, and which of them a given store calls "the 0025" is decided by
 *     what its name sorted to on the day it ran — not by intent.
 *
 * The same table is shared by every config that names the same database, which
 * is why `migrations-contract/` is gated here too: `wrangler.contract.toml`
 * applies it to `osl-keyserver-prod` and records names in the same
 * `d1_migrations`. Its README states the rule ("Numbers are reserved from 0100
 * upward … Names are globally unique in `d1_migrations`") and nothing executed
 * it; `assertMigrationSequences` now does.
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

/** The name shape wrangler applies: `NNNN_lower_snake_case.sql`. */
const MIGRATION_FILENAME = /^(\d{4})_[a-z0-9]+(?:_[a-z0-9]+)*\.sql$/;

/**
 * The ONLY non-migration filename a migrations directory may hold.
 *
 * D-280's copy refused every non-conforming filename on the stated grounds that
 * "wrangler applies EVERY file in migrations_dir". Checked against the wrangler
 * this project actually pins rather than taken on trust: `getMigrationsPath`
 * resolves `migrations_pattern`, and when it is unset — neither
 * `wrangler.toml` nor `wrangler.contract.toml` sets it —
 * `getDefaultMigrationsPattern` returns `${migrationsDir}/*.sql`
 * (`node_modules/wrangler/wrangler-dist/cli.js:240816`). So a `.md` is not
 * applied, and `migrations-contract/README.md` — which carries the reserved-
 * range rule this file executes — is legitimate.
 *
 * It is spelled as ONE name rather than "any non-.sql file" on purpose. A stray
 * `.sql.bak`, a `0044_thing.sql.orig`, or a `.sql` whose name carries no number
 * all still fail: `migrations_pattern` is overridable, and a file sitting in
 * this directory that nobody can classify is exactly what hides a gap.
 */
const MIGRATION_DIR_ALLOWED_NON_MIGRATION = "README.md";

/**
 * Where each migrations directory's sequence legitimately begins.
 *
 * Pinned, and a directory that is not listed FAILS rather than defaulting: this
 * is the one number in the rule a human chooses, so it may not be chosen
 * silently. `migrations` is 1 because it is applied to an empty database.
 * `migrations-contract` is 100 because it is deliberately held back from the
 * ordinary batch and its README reserves 0100 upward for exactly that; a
 * contracting half is applied AFTER the expanding half it carries, never to an
 * empty database.
 */
export const MIGRATION_SEQUENCE_ORIGINS: Readonly<Record<string, number>> = {
  migrations: 1,
  "migrations-contract": 100,
};

/** A reason has to carry the finding, not a shrug. `"n/a"` is the defect. */
export const MIGRATION_SKIP_REASON_MIN_CHARS = 80;

/** A recorded, deliberate hole in the sequence. */
export interface MigrationSequenceSkip {
  /** Migrations directory, relative to the project root, POSIX separators. */
  readonly dir: string;
  /** The number that is deliberately absent. */
  readonly number: number;
  /** Why it is absent, and what was established. The gap records nothing. */
  readonly reason: string;
}

/**
 * A recorded, deliberate collision: one number, two or more files.
 *
 * D-280 chose to make GAPS expressible and refused duplicates outright. That is
 * the right default and it is kept — an UNRECORDED duplicate is still a hard
 * failure. But the keyserver's three duplicates already exist, and the routes to
 * green without a record are all worse than the record:
 *
 *   * **Renumbering.** For `0025` and `0026` this is not merely unverifiable, it
 *     is DISPROVED: `EXPAND-CONTRACT-0038.md` transcribes a read-only
 *     `wrangler d1 migrations list osl-keyserver-prod --remote` taken
 *     2026-08-04 whose pending set begins at `0038`, and states production runs
 *     "on the schema through `0037`". Every name at or below `0037` — both
 *     `0025`s and both `0026`s — is therefore already recorded as applied in
 *     that store's `d1_migrations`. Renaming one makes it unapplied again, and
 *     the next `migrations apply` would re-run its CREATE TABLE against a
 *     schema that already has it.
 *   * **Deleting one of the pair.** Forbidden outright, and it is the same act
 *     seen from the other side: the name stays in the store's applied list with
 *     nothing in the tree describing what it did.
 *   * **Not enabling the gate.** D-280's finding, and it is decisive: a gate
 *     whose only remedy is an unsafe act is a gate that gets deleted, and a
 *     deleted gate catches nothing.
 *
 * So the hatch exists — in exactly the original's shape, and no wider:
 *
 *   * it is not an allowlist for a NUMBER, it pins the exact FILE SET. A third
 *     file joining a recorded pair fails, naming it. A file renamed inside a
 *     recorded pair fails.
 *   * it SELF-RETIRES: the moment a duplicate is genuinely resolved, the record
 *     no longer matches disk and the gate stays red until the record is removed.
 *   * it demands a reason of substance, measured in characters, or it fails.
 *   * recording the same number twice fails; recording a number as both a skip
 *     and a duplicate fails.
 *
 * NOTE WHAT A RECORD DOES NOT CLAIM. It does not say the collision is harmless.
 * For each entry below the run order was proved irrelevant against real SQLite
 * (see `migration-sequence.test.ts`, which applies each pair in BOTH orders and
 * compares `sqlite_master`) — that is why these are recorded rather than
 * escalated. A pair whose order mattered would not belong here at all.
 */
export interface MigrationNumberDuplicate {
  /** Migrations directory, relative to the project root, POSIX separators. */
  readonly dir: string;
  /** The number two or more files share. */
  readonly number: number;
  /** EXACTLY the files sharing it, sorted. Not a prefix, not a minimum. */
  readonly files: readonly string[];
  /** How it arose and what was established about the deployed store. */
  readonly reason: string;
}

export const MIGRATION_SEQUENCE_SKIPS: readonly MigrationSequenceSkip[] = [
  {
    dir: "migrations",
    number: 42,
    reason:
      "D-287/D-288, the same mechanism as D-280 and still open. 0042 IS "
      + "written: commit f7df8684 \"D-147: a second, separately-scoped issuer "
      + "that mints the blob-store audience\", 2026-08-04, adds "
      + "migrations/0042_storage_grant_issuance.sql. `git branch -r --contains "
      + "f7df8684` returns NOTHING and `git branch --contains` returns exactly "
      + "one branch, the local fix/d147-cutover-issuer; it is an ancestor of "
      + "neither integrate/first-usable nor origin/main. 0043 was then "
      + "allocated on this line on 2026-08-05 (1dec9981), so the number is "
      + "consumed by a commit that has not arrived. Because f7df8684 exists on "
      + "no remote, applying 0042 to any store would have required running "
      + "wrangler from that one local checkout: every CI and origin/main path "
      + "is excluded by construction. This entry retires itself the moment "
      + "that lane merges and 0042 appears on disk. Do NOT close it with a "
      + "placeholder: that is a name no store has applied.",
  },
];

export const MIGRATION_NUMBER_DUPLICATES: readonly MigrationNumberDuplicate[] = [
  {
    dir: "migrations",
    number: 25,
    files: ["0025_payment_alert_outbox.sql", "0025_username_directory.sql"],
    reason:
      "D-287/D-288. Two branches each allocated 0025 on top of 0024. "
      + "4b7a1573 \"Finish OSL Chats profiles and username friends\" "
      + "(2026-07-21) added 0025_username_directory; 12357c0b \"Harden crypto "
      + "wallet proof and payment alerts\" (2026-07-22) added "
      + "0025_payment_alert_outbox; neither commit is an ancestor of the "
      + "other. The collision LANDED at b767a604 — the same payment-alert "
      + "change re-applied (identical author date 2026-07-22 10:04:33, commit "
      + "date 21s later, different parent, a cherry-pick/rebase signature) "
      + "onto a tree that already held 0025_username_directory. Its parent "
      + "74318ff1 holds only the username file; b767a604 holds both, "
      + "renumbered neither. NOT RENUMBERABLE: production's applied list, "
      + "transcribed read-only 2026-08-04 in EXPAND-CONTRACT-0038.md, is "
      + "pending from 0038 and \"on the schema through 0037\", so both names "
      + "are already recorded as applied there. Run order proved irrelevant "
      + "against real SQLite: disjoint objects (payment_alert_outbox vs "
      + "username_directory/username_claim_receipts), both orders apply "
      + "cleanly to an identical sqlite_master.",
  },
  {
    dir: "migrations",
    number: 26,
    files: ["0026_osl_mail.sql", "0026_rn_capability_advertisement.sql"],
    reason:
      "D-287/D-288, the SAME two branches colliding a second time on the next "
      + "number. On the payment line, 08552e5d \"WIP snapshot\" (2026-07-26, "
      + "parent 12357c0b) added 0026_rn_capability_advertisement above "
      + "0025_payment_alert_outbox. On the username line, 5b598b5a "
      + "\"osl-mail-backend\" (2026-07-29, ancestor of origin/main) added "
      + "0026_osl_mail above 0025_username_directory. Each line was internally "
      + "contiguous and correct; the duplicate is created by joining them. It "
      + "landed by MERGE, not cherry-pick: 0480eb74 \"Merge branch "
      + "'batch-verify' into osl-mail-backend\", 2026-07-30 00:37:38, whose "
      + "two parents hold one 0026 each. NOT RENUMBERABLE for the same reason "
      + "as 0025 — both names sit below 0037 and are already applied in "
      + "production. Run order proved irrelevant against real SQLite: "
      + "0026_osl_mail creates mail_* tables, 0026_rn_capability_advertisement "
      + "adds users.rn_capabilities; ADD COLUMN on an FK parent commutes with "
      + "creating its children. Both orders yield an identical sqlite_master.",
  },
  {
    dir: "migrations",
    number: 38,
    files: [
      "0038_account_ownership_binding_account_unique.sql",
      "0038_username_identity_hardening.sql",
    ],
    reason:
      "D-287, and NOT the two-branch mechanism the other two are — this one is "
      + "a same-line misallocation, which is worse because nothing was hidden. "
      + "dbff8a6d \"t10-o2 enforce first binding wins across owners\" "
      + "(2026-08-01) correctly took 0038: its parent 0cf9edbd held 0037 then "
      + "0040, so 0038 and 0039 were both free. 9ef9101e \"T5-K3 retire "
      + "username identities with tombstones\" (2026-08-02) then took 0038 "
      + "AGAIN, and dbff8a6d is an ANCESTOR of it: the colliding file was "
      + "already on disk in that author's own checkout (parent f56f420e lists "
      + "0038_account_ownership_binding_account_unique and 0040, with 0039 "
      + "free). No merge, no cherry-pick, nothing to reconstruct. NOT "
      + "RENUMBERED, and here the reason is different from 0025/0026: the "
      + "2026-08-04 read-only list shows production has applied NEITHER name, "
      + "so a rename is not disproved — but it is not established either. That "
      + "transcript is a point-in-time record made by another lane, not a live "
      + "read; both names ARE applied in every store fed by the vitest worker "
      + "pool and by `db:migrate:local`; and the pair is the subject of a "
      + "half-executed expand/contract cutover whose contract halves "
      + "(migrations-contract/0100, 0101) name these files. See the tasklog "
      + "for the exact command a human must run to clear this. Run order "
      + "proved irrelevant against real SQLite: disjoint objects, and the one "
      + "table both write (worker_schema_capabilities) is created IF NOT "
      + "EXISTS back at 0031 and receives INSERT OR REPLACE on distinct "
      + "capability keys, which commutes.",
  },
];

function fail(message: string): never {
  throw new Error(`keyserver migration sequence: ${message}`);
}

function pad(value: number): string {
  return String(value).padStart(4, "0");
}

/**
 * Every `migrations_dir` any wrangler config in this project declares.
 *
 * Derived, never listed: the contract directory exists precisely because a
 * SECOND config was added to hold files back from the ordinary batch, so the
 * next such config must be covered the day it lands rather than the day someone
 * remembers. A config that names a directory with no recorded origin fails.
 */
export function readMigrationDirs(projectRoot: string): string[] {
  const configs = readdirSync(projectRoot, { withFileTypes: true })
    .filter((entry) => entry.isFile() && /^wrangler.*\.toml$/.test(entry.name))
    .map((entry) => entry.name)
    .sort();
  if (configs.length === 0) fail("no wrangler*.toml config found");
  const dirs: string[] = [];
  for (const config of configs) {
    const toml = readFileSync(join(projectRoot, config), "utf8");
    for (const line of toml.split(/\r?\n/)) {
      const match = /^\s*migrations_dir\s*=\s*"([^"]+)"\s*$/.exec(line);
      if (match) {
        dirs.push(match[1]!.replace(/\\/g, "/").replace(/^\.\//, ""));
      }
    }
  }
  if (dirs.length === 0) fail("no wrangler config declares a `migrations_dir`");
  const unique = [...new Set(dirs)].sort();
  for (const dir of unique) {
    if (!statSync(join(projectRoot, dir)).isDirectory()) {
      fail(`migrations_dir ${dir} is not a directory`);
    }
  }
  return unique;
}

/**
 * D-280/D-287/D-288. Refuse a migrations directory whose numbering is not
 * contiguous, not unique, not well-named, or does not begin where it must.
 *
 * NOT covered here, deliberately: an EMPTY migrations directory has no numbers
 * and therefore no gaps and no collisions. Losing a whole directory is a
 * different failure and must not be caught by this rule's evidence — a check
 * satisfied by two different absences cannot tell you which one you have.
 */
export function assertMigrationSequences(
  projectRoot: string,
  migrationDirs: readonly string[],
  // Defaulted, not injected at the call site: production always gets the pinned
  // records, and a test can still starve the rules with records of its own to
  // prove they refuse rather than merely exist.
  recordedSkips: readonly MigrationSequenceSkip[] = MIGRATION_SEQUENCE_SKIPS,
  recordedDuplicates: readonly MigrationNumberDuplicate[] =
    MIGRATION_NUMBER_DUPLICATES,
  origins: Readonly<Record<string, number>> = MIGRATION_SEQUENCE_ORIGINS,
): void {
  const skipKeys = new Set<string>();
  for (const skip of recordedSkips) {
    const key = `${skip.dir}/${pad(skip.number)}`;
    if (skipKeys.has(key)) fail(`${key} is recorded as skipped twice`);
    skipKeys.add(key);
    if (!Number.isInteger(skip.number) || skip.number < 1) {
      fail(`recorded skip ${key} is not a migration number`);
    }
    assertReason(`recorded skip ${key}`, skip.reason);
  }

  const duplicateKeys = new Set<string>();
  for (const duplicate of recordedDuplicates) {
    const key = `${duplicate.dir}/${pad(duplicate.number)}`;
    if (duplicateKeys.has(key)) {
      fail(`${key} is recorded as a duplicate twice`);
    }
    duplicateKeys.add(key);
    if (skipKeys.has(key)) {
      fail(
        `${key} is recorded as BOTH a deliberate skip and a recorded `
        + "duplicate. It is either absent or shared; it cannot be both, and a "
        + "number that is two things at once is a record nobody is reading",
      );
    }
    if (!Number.isInteger(duplicate.number) || duplicate.number < 1) {
      fail(`recorded duplicate ${key} is not a migration number`);
    }
    if (duplicate.files.length < 2) {
      fail(
        `recorded duplicate ${key} names ${duplicate.files.length} file(s). A `
        + "duplicate record that does not name at least two colliding files "
        + "records nothing and cannot self-retire",
      );
    }
    const sorted = [...duplicate.files].sort();
    if (
      new Set(duplicate.files).size !== duplicate.files.length
      || sorted.join(" ") !== duplicate.files.join(" ")
    ) {
      fail(
        `recorded duplicate ${key} must name each colliding file once, in `
        + "name order — the order wrangler itself applies them in",
      );
    }
    assertReason(`recorded duplicate ${key}`, duplicate.reason);
  }

  const numberOwner = new Map<number, string>();
  const nameOwner = new Map<string, string>();

  for (const dir of migrationDirs) {
    const origin = origins[dir];
    if (origin === undefined) {
      fail(
        `${dir} is applied by a wrangler config but has no recorded sequence `
        + "origin. The number a sequence starts at is the one number in this "
        + "rule a human chooses, so a new migrations directory has to say "
        + "where it begins before it can be gated — add it to "
        + "MIGRATION_SEQUENCE_ORIGINS",
      );
    }

    const byNumber = new Map<number, string[]>();
    const names = readdirSync(join(projectRoot, dir), { withFileTypes: true })
      .filter(
        (entry) =>
          entry.isFile()
          && entry.name !== MIGRATION_DIR_ALLOWED_NON_MIGRATION,
      )
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
      const owner = nameOwner.get(name);
      if (owner !== undefined) {
        fail(
          `${owner}/${name} and ${dir}/${name} are the same migration NAME in `
          + "two directories applied to the same database. `d1_migrations` "
          + "records the bare name, so the second one to run is recorded as "
          + "already applied and silently never runs",
        );
      }
      nameOwner.set(name, dir);
      byNumber.set(number, [...(byNumber.get(number) ?? []), name]);
    }
    if (byNumber.size === 0) continue;

    for (const [number, files] of byNumber) {
      const owner = numberOwner.get(number);
      if (owner !== undefined && owner !== dir) {
        fail(
          `${pad(number)} is used by both ${owner} and ${dir}. Both are `
          + "applied to the same database and share one `d1_migrations` "
          + "table, so the ranges must not overlap — move the held-back range "
          + "up, never the ordinary sequence",
        );
      }
      numberOwner.set(number, dir);
      if (files.length < 2) continue;
      const recorded = recordedDuplicates.find(
        (entry) => entry.dir === dir && entry.number === number,
      );
      if (recorded === undefined) {
        fail(
          `${dir} holds ${files.length} migrations numbered ${pad(number)}: `
          + `${files.join(", ")}. Which one a store recorded as applied is `
          + "then decided by name order rather than by intent, and applying "
          + "both to a store that ran only one is not the same operation "
          + "twice. Establish from history how it arose and whether the run "
          + "order matters, then either renumber — only if you can show no "
          + "store holds the name — or record it in MIGRATION_NUMBER_DUPLICATES "
          + "with the evidence",
        );
      }
      if (recorded.files.join(" ") !== files.join(" ")) {
        fail(
          `${dir}/${pad(number)} is recorded as a duplicate of `
          + `${recorded.files.join(", ")}, but disk holds ${files.join(", ")}. `
          + "A duplicate record pins the exact file set: a file joining or "
          + "leaving a recorded collision is a NEW collision that nobody has "
          + "established anything about",
        );
      }
    }

    for (const duplicate of recordedDuplicates) {
      if (duplicate.dir !== dir) continue;
      const files = byNumber.get(duplicate.number) ?? [];
      if (files.length >= 2) continue;
      fail(
        `${dir} holds ${files.length} migration(s) numbered `
        + `${pad(duplicate.number)}${files.length === 1 ? ` (${files[0]})` : ""}`
        + `, but ${pad(duplicate.number)} is still recorded as a duplicate of `
        + `${duplicate.files.join(", ")}. A record that outlives its collision `
        + "is an exception nobody is reading — delete the entry",
      );
    }

    const numbers = [...byNumber.keys()].sort((a, b) => a - b);
    const first = numbers[0]!;
    const last = numbers[numbers.length - 1]!;
    if (first !== origin) {
      fail(
        `${dir} starts at ${pad(first)}, not ${pad(origin)}. The recorded `
        + "origin is where this directory's sequence is applied from; a "
        + "sequence that starts anywhere else either cannot be applied to the "
        + "database it targets, or has silently lost its first file",
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
          `${dir}/${present.join(", ")} exists, but ${pad(number)} is still `
          + "recorded as a deliberate skip. A skip that outlives its gap is an "
          + "exception nobody is reading — delete the entry",
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
      const below = byNumber.get(
        Math.max(...numbers.filter((n) => n < number)),
      );
      const above = byNumber.get(
        Math.min(...numbers.filter((n) => n > number)),
      );
      fail(
        `${dir} is not contiguous: ${pad(number)} is missing between `
        + `${below?.join(", ")} and ${above?.join(", ")}. Migrations are `
        + "applied in order and a store records the NAMES it applied, so this "
        + "is either a migration that was written and dropped without "
        + "renumbering — which a deployed store may already have run, leaving "
        + "a schema this tree does not describe — or a number skipped with "
        + "nothing recording it. Do NOT close it with a placeholder migration: "
        + "that is a name no store has applied, and it manufactures the drift "
        + "this check exists to catch. Establish from history which happened, "
        + "then either restore the file or record it in "
        + "MIGRATION_SEQUENCE_SKIPS with the evidence",
      );
    }
  }
}

function assertReason(subject: string, reason: string): void {
  if (reason.trim().length < MIGRATION_SKIP_REASON_MIN_CHARS) {
    fail(
      `${subject} gives no reason worth reading (${reason.trim().length} `
      + `characters, minimum ${MIGRATION_SKIP_REASON_MIN_CHARS}). A hole that `
      + "is admitted without the finding behind it is the defect, written down",
    );
  }
}

/** The whole rule over the real project, for callers that have a root. */
export function assertProjectMigrationSequences(projectRoot: string): void {
  assertMigrationSequences(projectRoot, readMigrationDirs(projectRoot));
}
