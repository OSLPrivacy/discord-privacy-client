// D-248 — plan the `username_skeleton` backfill, and REFUSE rather than guess.
//
// WHY THIS IS NOT A SQL MIGRATION. A UTS #39 skeleton cannot be computed in
// SQLite. Migration 0038's original backfill wrote `skeleton = username`, which
// is the defect in migration form: the comment justifying it claimed that "rows
// predating the identity-hardening writer used the ASCII-only grammar, for
// which the normalized spelling is also the skeleton", and that claim is FALSE
// even for pure ASCII — `michael` skeletons to `rnichael`, `paypa1` to
// `paypal`. So the backfill has to run outside the database, against the same
// pinned artifact the Worker uses.
//
// WHAT IT REFUSES TO DO. Two live rows can turn out to share a skeleton once it
// is computed correctly; on the shipping alphabet `michael`/`michae1`,
// `support`/`supp0rt` and `paypal`/`paypa1` are each one such pair. Both rows
// are somebody's identity. This tool will NOT pick one, and it will not emit
// SQL that a UNIQUE index would silently reject: it prints the whole collision
// set and exits 3, because which identity keeps the name is an owner decision
// and dropping one quietly is the impersonation this defect is about.
//
// USAGE
//   npx wrangler d1 execute osl-keyserver-prod --remote --json --command \
//     "SELECT username, username_skeleton, display_username FROM username_directory" \
//     > directory.json
//   npx wrangler d1 execute osl-keyserver-prod --remote --json --command \
//     "SELECT username, skeleton FROM username_tombstones" > tombstones.json
//   node scripts/backfill-username-skeletons.mjs \
//     --directory directory.json [--tombstones tombstones.json] [--out backfill.sql]
//
// EXIT CODES (they are the result; read them)
//   0  a plan was produced, or there was nothing to do
//   2  bad invocation / unreadable input
//   3  REFUSED — a collision, or a row whose handle the artifact will not
//      analyze. Nothing was emitted.

import { readFileSync, writeFileSync } from "node:fs";
import { usernameSkeleton, UsernameNotAnalyzable } from "./username-skeleton-node.mjs";

/// `wrangler d1 execute --json` emits `[{ results: [...] }]`; a hand-made file
/// may be either that, `{ results: [...] }`, or a bare array. Accept all three
/// and refuse anything else rather than silently backfilling zero rows.
export function readRows(text, label) {
  const parsed = JSON.parse(text);
  const rows = Array.isArray(parsed)
    ? (parsed.length > 0 && parsed[0] && typeof parsed[0] === "object" && "results" in parsed[0]
      ? parsed[0].results
      : parsed)
    : parsed?.results;
  if (!Array.isArray(rows)) throw new Error(`${label}: expected rows, got ${typeof rows}`);
  return rows;
}

function sqlString(value) {
  return `'${String(value).replace(/'/g, "''")}'`;
}

/// Build the plan. Pure: no I/O, so the tests drive it directly.
export function planBackfill({ directory = [], tombstones = [] }) {
  const problems = [];
  const collisions = [];
  const directoryUpdates = [];
  const tombstoneUpdates = [];

  const skeletonOf = (name, table) => {
    try {
      return usernameSkeleton(name);
    } catch (error) {
      if (error instanceof UsernameNotAnalyzable) {
        problems.push(`${table}: ${JSON.stringify(name)} — ${error.message}`);
        return null;
      }
      throw error;
    }
  };

  const bySkeleton = new Map();
  for (const row of directory) {
    if (typeof row?.username !== "string") {
      problems.push(`username_directory: row without a username: ${JSON.stringify(row)}`);
      continue;
    }
    const skeleton = skeletonOf(row.username, "username_directory");
    if (skeleton === null) continue;
    if (!bySkeleton.has(skeleton)) bySkeleton.set(skeleton, []);
    bySkeleton.get(skeleton).push(row.username);
    // display_username is the spelling the identity holds; under
    // validate-don't-transform that IS `username`. Only fill it when absent —
    // never overwrite one an operator or a later grammar has set.
    const needsDisplay = row.display_username === null || row.display_username === undefined;
    if (row.username_skeleton !== skeleton || needsDisplay) {
      directoryUpdates.push(
        `UPDATE username_directory SET username_skeleton = ${sqlString(skeleton)}` +
        (needsDisplay ? `, display_username = ${sqlString(row.username)}` : "") +
        ` WHERE username = ${sqlString(row.username)};`,
      );
    }
  }

  for (const [skeleton, names] of bySkeleton) {
    if (names.length > 1) collisions.push({ skeleton, usernames: [...names].sort() });
  }

  // Tombstones carry the retired identity's skeleton. A tombstone whose
  // skeleton is raw lets a retired name's confusables be re-claimed, so it is
  // part of the same backfill. Two tombstones CAN share a skeleton legitimately
  // in a corrupt-but-real database, and `username_tombstones.skeleton` is
  // UNIQUE, so that is a refusal too.
  const tombstoneBySkeleton = new Map();
  for (const row of tombstones) {
    if (typeof row?.username !== "string") {
      problems.push(`username_tombstones: row without a username: ${JSON.stringify(row)}`);
      continue;
    }
    const skeleton = skeletonOf(row.username, "username_tombstones");
    if (skeleton === null) continue;
    if (!tombstoneBySkeleton.has(skeleton)) tombstoneBySkeleton.set(skeleton, []);
    tombstoneBySkeleton.get(skeleton).push(row.username);
    if (row.skeleton !== skeleton) {
      tombstoneUpdates.push(
        `UPDATE username_tombstones SET skeleton = ${sqlString(skeleton)}` +
        ` WHERE username = ${sqlString(row.username)};`,
      );
    }
  }
  for (const [skeleton, names] of tombstoneBySkeleton) {
    if (names.length > 1) {
      collisions.push({ skeleton, usernames: [...names].sort(), table: "username_tombstones" });
    }
  }

  const refused = collisions.length > 0 || problems.length > 0;
  return {
    refused,
    collisions,
    problems,
    directoryRows: directory.length,
    tombstoneRows: tombstones.length,
    sql: refused ? "" : [
      "-- D-248 · generated by scripts/backfill-username-skeletons.mjs.",
      "-- Every value below came from the pinned UTS #39 artifact, not from SQL.",
      "-- Re-running this file is a no-op: each statement is an absolute SET.",
      ...directoryUpdates,
      ...tombstoneUpdates,
    ].join("\n") + "\n",
    // Statements EMITTED, not statements imagined: a refusal emits none, and a
    // caller that reads this number must not be told otherwise.
    updateCount: refused ? 0 : directoryUpdates.length + tombstoneUpdates.length,
  };
}

function main(argv) {
  const args = {};
  for (let i = 2; i < argv.length; i += 2) args[argv[i].replace(/^--/, "")] = argv[i + 1];
  if (!args.directory) {
    console.error("usage: backfill-username-skeletons.mjs --directory <rows.json> [--tombstones <rows.json>] [--out <file.sql>]");
    return 2;
  }
  let directory, tombstones = [];
  try {
    directory = readRows(readFileSync(args.directory, "utf8"), args.directory);
    if (args.tombstones) tombstones = readRows(readFileSync(args.tombstones, "utf8"), args.tombstones);
  } catch (error) {
    console.error(`cannot read input: ${error.message}`);
    return 2;
  }

  const plan = planBackfill({ directory, tombstones });
  console.log(`username_directory rows : ${plan.directoryRows}`);
  console.log(`username_tombstones rows: ${plan.tombstoneRows}`);

  if (plan.problems.length > 0) {
    console.log("\nHANDLES THIS ARTIFACT WILL NOT ANALYZE — an owner decision, not a default:");
    for (const p of plan.problems) console.log(`  ${p}`);
  }
  if (plan.collisions.length > 0) {
    console.log("\nCOLLISION SET — these identities fold together once the skeleton is computed");
    console.log("correctly. Each line is two or more live identities that a UNIQUE skeleton index");
    console.log("cannot both hold. REFUSING: which one keeps the name is not this tool's call.");
    for (const c of plan.collisions) {
      console.log(`  ${c.table ?? "username_directory"}  skeleton ${JSON.stringify(c.skeleton)} <- ${c.usernames.map((u) => JSON.stringify(u)).join(", ")}`);
    }
  }
  if (plan.refused) {
    console.log("\nREFUSED. No SQL was emitted and nothing was changed.");
    return 3;
  }

  console.log(`statements to apply     : ${plan.updateCount}`);
  if (args.out) {
    writeFileSync(args.out, plan.sql);
    console.log(`written to              : ${args.out}`);
  } else if (plan.updateCount > 0) {
    console.log("");
    process.stdout.write(plan.sql);
  }
  return 0;
}

if (import.meta.url === `file://${process.argv[1]}`) process.exit(main(process.argv));
