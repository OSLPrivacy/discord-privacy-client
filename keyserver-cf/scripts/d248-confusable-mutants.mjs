#!/usr/bin/env node
/**
 * D-248 mutation proof — every writer of `username_skeleton`, plus the
 * comparison that makes the column mean anything.
 *
 * Each mutant reintroduces exactly ONE defect and requires a NON-ZERO exit code
 * from the suite that is supposed to catch it. A mutant that survives means the
 * gate is decoration, and the headline mutant here (`M2`) is the one that
 * distinguishes "a skeleton function runs" from "a skeleton is compared":
 * `usernameSkeleton()` is still called, its result is still computed, and only
 * the value bound into the column is wrong.
 *
 * Run:  node scripts/d248-confusable-mutants.mjs
 */
import { readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const D248 = "test/integration/d248-confusable-username.test.ts";

const MUTANTS = [
  {
    id: "M1-skeleton-column-holds-the-raw-name",
    why: "D-248 itself: bind ?1 into username_skeleton, so the skeleton equals the raw name and the unique index can never fire",
    edits: [{
      file: "src/endpoints/usernames.ts",
      from: "         SELECT ?1, ?6, ?1, ?2, ?3, ?4, ?4",
      to: "         SELECT ?1, ?1, ?1, ?2, ?3, ?4, ?4",
    }],
    suites: [D248],
  },
  {
    id: "M2-skeleton-computed-but-never-compared",
    why: "keep computing the skeleton -- the function runs, its refusals still fire -- but store the raw name instead. Proves the COMPARISON does the work, not the call.",
    edits: [{
      file: "src/endpoints/usernames.ts",
      from: ").bind(username, userId, body.friend_code, now, digest, skeleton),",
      to: ").bind(username, userId, body.friend_code, now, digest, username),",
    }],
    suites: [D248],
  },
  {
    id: "M3-rotate-writer-tombstones-the-raw-name",
    why: "src/lib/db.ts rotateUserKeys copies the raw username into username_tombstones.skeleton, so a rotated-away name's confusables become reclaimable",
    edits: [{
      file: "src/lib/db.ts",
      from: "       SELECT username, username_skeleton, ?3 FROM username_directory",
      to: "       SELECT username, username, ?3 FROM username_directory",
    }],
    suites: [D248],
  },
  {
    id: "M4-unregister-writer-tombstones-the-raw-name",
    why: "src/lib/db.ts unregisterUserIfCurrent does the same, so a deleted account's name is confusable-reclaimable",
    edits: [{
      file: "src/lib/db.ts",
      from: "           SELECT username, username_skeleton, ? FROM username_directory",
      to: "           SELECT username, username, ? FROM username_directory",
    }],
    suites: [D248],
  },
  {
    id: "M5-retire-trigger-tombstones-the-raw-name",
    why: "migration 0038's BEFORE DELETE trigger -- the rename path's writer -- stores the raw name as the retired skeleton",
    edits: [{
      file: "migrations/0038_username_identity_hardening.sql",
      from: "          COALESCE(OLD.username_skeleton, OLD.username),",
      to: "          OLD.username,",
    }],
    suites: [D248],
  },
  {
    id: "M6-no-case-fold-on-the-skeleton",
    why: "drop the second pass through the artifact's normalizer, so UTS #39's UPPERCASE prototypes leave zero-for-o (supp0rt vs support) in different classes",
    edits: [{
      file: "src/lib/username.ts",
      from: "  const folded = analyzeIdentifier(analysis.skeleton).normalized;",
      to: "  const folded = analysis.skeleton;",
    }],
    suites: [D248],
  },
  {
    id: "M7-upsert-keeps-a-stale-skeleton",
    why: "stop the refresh path rewriting the derived columns, so every identity that already exists keeps its raw skeleton forever",
    edits: [{
      file: "src/endpoints/usernames.ts",
      from: "           username = excluded.username, username_skeleton = excluded.username_skeleton,\n           display_username = excluded.display_username,\n           friend_code",
      to: "           username = excluded.username,\n           friend_code",
    }],
    suites: [D248],
  },
  {
    id: "M8-backfill-fabricates-a-skeleton",
    why: "let the operator backfill write the raw name (migration 0038's original COALESCE) instead of the computed skeleton",
    edits: [{
      file: "scripts/backfill-username-skeletons.mjs",
      from: "      return usernameSkeleton(name);",
      to: "      return name;",
    }],
    suites: ["scripts/backfill-username-skeletons.test.ts"],
    node: true,
  },
];

function run(suites, node) {
  const args = node
    ? ["vitest", "run", "--config", "vitest.node.config.ts", ...suites]
    : ["vitest", "run", ...suites];
  return spawnSync("npx", args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
}

let allDead = true;
for (const mutant of MUTANTS) {
  const originals = new Map();
  let applied = true;
  for (const edit of mutant.edits) {
    const before = readFileSync(edit.file, "utf8");
    if (!originals.has(edit.file)) originals.set(edit.file, before);
    if (!before.includes(edit.from)) {
      console.log(`[HARNESS STOP] ${mutant.id}: anchor not found in ${edit.file}`);
      console.log(`               ${JSON.stringify(edit.from)}`);
      applied = false;
      allDead = false;
      break;
    }
    writeFileSync(edit.file, before.replace(edit.from, edit.to));
  }
  if (applied) {
    const result = run(mutant.suites, mutant.node === true);
    const dead = result.status !== 0;
    if (!dead) allDead = false;
    const tail = `${result.stdout}${result.stderr}`.split("\n")
      .filter((l) => /Tests? +\d|Test Files/.test(l)).join(" | ");
    console.log(`[${dead ? "DEAD    " : "SURVIVED"}] ${mutant.id}`);
    console.log(`           exit ${result.status}  ${tail}`);
    console.log(`           ${mutant.why}`);
  }
  for (const [file, text] of originals) writeFileSync(file, text);
}

console.log(allDead
  ? "\nALL MUTANTS DEAD — every skeleton writer and the comparison are load-bearing"
  : "\nAT LEAST ONE MUTANT SURVIVED — that gate is decoration, do not trust it");
process.exit(allDead ? 0 : 1);
