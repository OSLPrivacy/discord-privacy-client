// D-175 — step-order test for the expand/contract split of the 0038 pair.
//
// The deliverable this proves: **you can stop after any step and production
// still works.** So the test is not "does the final schema work" — it is, for
// every intermediate state, "is this schema serveable by the Worker generation
// that is running at that point, and by the one that is about to be?"
//
// Nothing here touches production. It rebuilds the live schema locally from a
// read-only dump and applies the migration files one step at a time.
//
// TWO WORKER GENERATIONS, BOTH TAKEN FROM ARTIFACTS, NOT FROM SOURCE:
//   deployed  — the multipart bundle downloaded read-only from
//               /workers/services/oslprivacy-keyserver/environments/production/content
//   candidate — the bundle produced by `wrangler deploy --dry-run --outdir`
// Every SQL statement executed on their behalf is a string literal lifted out
// of the bundle. This project's standing rule is that the source is not the
// system, and three verdicts on this exact question were wrong because a
// hand-written suite invented a write path the running artifact does not have.
//
// Usage:
//   node scripts/expand-contract-step-order.mjs \
//     --schema <live-schema.json> --deployed <index.js> --candidate <index.js> \
//     [--mutate <name>]
//
// Exit code is NOT the result. Read the verdict lines. (A pipeline's exit code
// has been wrong about this repository before.)

import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { planBackfill } from "./backfill-username-skeletons.mjs";
import { usernameSkeleton } from "./username-skeleton-node.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const KEYSERVER = join(HERE, "..");

// ---------------------------------------------------------------------------
// args
// ---------------------------------------------------------------------------
const args = {};
for (let i = 2; i < process.argv.length; i += 2) {
  args[process.argv[i].replace(/^--/, "")] = process.argv[i + 1];
}
const MUTATION = args.mutate ?? "none";
const SIMULATE_OLD_WORKER = "simulate-old-worker-after-expand" in args
  || MUTATION === "simulate-old-worker-after-expand";

// ---------------------------------------------------------------------------
// SQL literal extraction — identical method to D-174's, kept verbatim so the
// two lanes' numbers are comparable.
// ---------------------------------------------------------------------------
function extractSql(path) {
  const src = readFileSync(path, "utf8");
  const HEAD = /^\s*(SELECT|INSERT|UPDATE|DELETE|REPLACE|WITH|CREATE|DROP|PRAGMA|ALTER)\b/i;
  const out = new Set();
  const quotes = ['"', "'", "`"];
  for (let i = 0; i < src.length; i++) {
    const q = src[i];
    if (!quotes.includes(q)) continue;
    let j = i + 1, buf = "";
    while (j < src.length) {
      if (src[j] === "\\") { buf += src[j + 1] === "n" ? "\n" : src[j + 1]; j += 2; continue; }
      if (src[j] === q) break;
      buf += src[j]; j++;
    }
    if (j >= src.length) continue;
    if (HEAD.test(buf) && buf.length > 12) out.add(buf.trim());
    i = j;
  }
  return [...out].sort();
}

// ---------------------------------------------------------------------------
// live schema
// ---------------------------------------------------------------------------
const parsed = JSON.parse(readFileSync(args.schema, "utf8"));
const rows = (Array.isArray(parsed) ? parsed[0]?.results : parsed.results) ?? [];
const ORDER = { table: 0, index: 1, view: 1, trigger: 2 };
const SCHEMA_OBJECTS = rows
  .filter((r) => r.sql && !String(r.name).startsWith("sqlite_"))
  .sort((a, b) => (ORDER[a.type] ?? 3) - (ORDER[b.type] ?? 3));

const DEPLOYED_SQL = extractSql(args.deployed);
const CANDIDATE_SQL = extractSql(args.candidate);
const DEPLOYED_SRC = readFileSync(args.deployed, "utf8");

// ---------------------------------------------------------------------------
// A gate that cannot fail is decoration. These assertions starve the harness's
// own inputs: if the artifact stops containing what the suite depends on, the
// suite must stop, not quietly pass.
// ---------------------------------------------------------------------------
const preconditions = [];
function precondition(label, ok, detail) {
  preconditions.push({ label, ok, detail });
}
precondition("deployed bundle parsed as SQL", DEPLOYED_SQL.length > 100, `${DEPLOYED_SQL.length} statements`);
precondition("candidate bundle parsed as SQL", CANDIDATE_SQL.length > 100, `${CANDIDATE_SQL.length} statements`);
// The deployed Worker's username-claim catch. If this literal is not in the
// running artifact, every "409 vs 500" claim below is unfounded.
const DEPLOYED_CLAIM_CATCH = "/username_directory\\.username|UNIQUE|PRIMARY/i";
precondition(
  "deployed catch regex present in the running artifact",
  DEPLOYED_SRC.includes(DEPLOYED_CLAIM_CATCH),
  DEPLOYED_CLAIM_CATCH,
);
precondition(
  "deployed artifact does NOT map 'username is retired'",
  !DEPLOYED_SRC.includes("username is retired"),
  "an ABORT with that text would be rethrown -> 500",
);
precondition(
  "deployed artifact never names account_ownership_proof_bindings",
  !DEPLOYED_SRC.includes("account_ownership_proof_bindings"),
  "0 occurrences -> it cannot write that table",
);
// The contract's mandatory-column abort text is chosen, not accidental: it
// begins with `username_directory.username`, which is inside the deployed
// Worker's own catch. So even after the contract has landed, a Worker rolled
// back below the candidate answers 409 rather than 500 — a wrong answer, but
// not a 500 and not an exception trace. Assert both halves of that claim.
const CONTRACT_ABORT_TEXT = "username_directory.username_skeleton and display_username are required";
precondition(
  "contract abort text falls inside the deployed Worker's catch",
  /username_directory\.username|UNIQUE|PRIMARY/i.test(CONTRACT_ABORT_TEXT),
  "post-contract rollback degrades to 409, not 500",
);
precondition(
  "that text is what the contract migration actually raises",
  readFileSync(join(KEYSERVER, "migrations-contract/0100_username_identity_contract.sql"), "utf8")
    .includes(CONTRACT_ABORT_TEXT),
  "migrations-contract/0100_username_identity_contract.sql",
);

// ---------------------------------------------------------------------------
// statement picking — exactly one match, or the harness stops
// ---------------------------------------------------------------------------
function pick(list, label, ...needles) {
  const hits = list.filter((s) => needles.every((n) => s.includes(n)));
  if (hits.length !== 1) {
    console.log(`\nFATAL: '${label}' matched ${hits.length} statements, expected exactly 1.`);
    for (const h of hits) console.log("  ---\n" + h);
    process.exit(2);
  }
  return hits[0];
}

const D = {
  claim: pick(DEPLOYED_SQL, "deployed username claim", "INSERT INTO username_directory", "ON CONFLICT"),
  renameDelete: pick(DEPLOYED_SQL, "deployed rename delete", "DELETE FROM username_directory", "username <> ?2"),
  rotateDelete: pick(DEPLOYED_SQL, "deployed rotate delete", "DELETE FROM username_directory", "ik_ed25519_pub = ?2"),
  lookup: pick(DEPLOYED_SQL, "deployed username lookup", "SELECT user_id FROM username_directory"),
  licenseSelect: pick(DEPLOYED_SQL, "deployed license read", "SELECT * FROM licenses"),
  licenseInsert: pick(DEPLOYED_SQL, "deployed license insert", "INSERT OR IGNORE INTO licenses", "issued_at) VALUES (?, ?, ?)"),
  challengeInsert: pick(DEPLOYED_SQL, "deployed challenge insert", "INSERT INTO account_ownership_challenges"),
};
const C = {
  claim: pick(CANDIDATE_SQL, "candidate username claim", "INSERT INTO username_directory", "username_skeleton"),
  renameDelete: pick(CANDIDATE_SQL, "candidate rename delete", "DELETE FROM username_directory", "username <> ?2"),
  rotateTombstone: pick(CANDIDATE_SQL, "candidate rotate tombstone", "INSERT INTO username_tombstones", "?3 FROM username_directory"),
  rotateDelete: pick(CANDIDATE_SQL, "candidate rotate delete", "DELETE FROM username_directory", "ik_ed25519_pub = ?2"),
  bindingInsert: pick(CANDIDATE_SQL, "candidate binding insert", "INSERT INTO account_ownership_proof_bindings"),
  lookup: pick(CANDIDATE_SQL, "candidate username lookup", "SELECT user_id FROM username_directory"),
  // The candidate replaced the deployed Worker's `SELECT *` with explicit
  // column lists that name 0040's new columns. That asymmetry is why 0040 must
  // be checked against BOTH artifacts and not reasoned about from either.
  licenseSelect: pick(CANDIDATE_SQL, "candidate license read", "SELECT subscription_id, revoked_at, redeemed_at"),
  licenseRedeem: pick(CANDIDATE_SQL, "candidate license redeem", "SET redeemed_at = ?"),
};

// ---------------------------------------------------------------------------
// migration steps
// ---------------------------------------------------------------------------
const EXPAND = [
  "migrations/0038_account_ownership_binding_account_unique.sql",
  "migrations/0038_username_identity_hardening.sql",
  "migrations/0039_device_roster.sql",
  "migrations/0040_license_redemption.sql",
  "migrations/0041_space_event_queue_reserved.sql",
];
let CONTRACT = [
  "migrations-contract/0100_username_identity_contract.sql",
  "migrations-contract/0101_account_ownership_binding_contract.sql",
];
// The backfill of step 2. D-248: the skeleton half of this used to be
// `COALESCE(username_skeleton, username)`, which writes the RAW name into the
// skeleton column and makes the unique index decorative. A UTS #39 skeleton
// cannot be computed in SQL, so the skeleton half is now produced by
// `scripts/backfill-username-skeletons.mjs` from the same pinned artifact the
// Worker uses, and it REFUSES if two live rows fold together.
const BACKFILL_DISPLAY = `UPDATE username_directory
   SET display_username = COALESCE(display_username, username)
 WHERE display_username IS NULL`;

function applyBackfill(d) {
  d.exec(BACKFILL_DISPLAY);
  const rows = d.prepare(
    "SELECT username, username_skeleton, display_username FROM username_directory",
  ).all();
  const tombstones = tableExists(d, "username_tombstones")
    ? d.prepare("SELECT username, skeleton FROM username_tombstones").all()
    : [];
  const plan = planBackfill({ directory: rows, tombstones });
  if (plan.refused) {
    // Not a silent skip: the operator backfill refuses here too, and a refusal
    // that the step-order test swallowed would be the decoration this file
    // exists to avoid.
    throw new Error(
      `backfill REFUSED — collisions ${JSON.stringify(plan.collisions)} problems ${JSON.stringify(plan.problems)}`,
    );
  }
  if (plan.sql) d.exec(plan.sql);
}

function loadMigration(rel) {
  return readFileSync(join(KEYSERVER, rel), "utf8");
}

// MUTANTS. Each is a named, deliberate corruption of the plan; the step-order
// test must go RED for it. `--mutate none` is the real plan.
function migrationsFor(rel) {
  let sql = loadMigration(rel);
  if (MUTATION === "collapse-expand-contract" && rel.endsWith("0038_username_identity_hardening.sql")) {
    // Put the contract back inside the expand file: one step again.
    sql += "\n" + loadMigration("migrations-contract/0100_username_identity_contract.sql");
  }
  if (MUTATION === "collapse-expand-contract" && rel.endsWith("0038_account_ownership_binding_account_unique.sql")) {
    sql += "\n" + loadMigration("migrations-contract/0101_account_ownership_binding_contract.sql");
  }
  if (MUTATION === "not-null-default-empty" && rel.endsWith("0038_username_identity_hardening.sql")) {
    sql = sql
      .replace("ADD COLUMN username_skeleton TEXT;", "ADD COLUMN username_skeleton TEXT NOT NULL DEFAULT '';")
      .replace("ADD COLUMN display_username TEXT;", "ADD COLUMN display_username TEXT NOT NULL DEFAULT '';");
  }
  if (MUTATION === "ungated-retired-abort" && rel.endsWith("0038_username_identity_hardening.sql")) {
    sql = sql
      .replace("WHEN NEW.username_skeleton IS NOT NULL\n AND EXISTS (", "WHEN EXISTS (")
      .replace(/CREATE TRIGGER username_directory_reject_retired_legacy_writer[\s\S]*?END;/, "");
  }
  // Starve the contract guard of its own input: delete the guard, keep the
  // simulated old-Worker row. If the plan is still reported GREEN, the guard was
  // decoration. Run as: --mutate remove-contract-guard --simulate-old-worker-after-expand 1
  if (MUTATION === "remove-contract-guard" && rel.endsWith("0100_username_identity_contract.sql")) {
    sql = sql.replace(/CREATE TABLE _contract_guard_0100_directory[\s\S]*?DROP TABLE _contract_guard_0100_directory;/, "");
  }
  if (MUTATION === "no-coalesce-retire" && rel.endsWith("0038_username_identity_hardening.sql")) {
    sql = sql
      .replace("skeleton TEXT UNIQUE,", "skeleton TEXT NOT NULL UNIQUE,")
      .replace("COALESCE(OLD.username_skeleton, OLD.username),", "OLD.username_skeleton,");
  }
  return sql;
}

// The collapse mutant folds the contract back into the expand files, so there
// is no separate contract step left to run — that IS the mutation.
if (MUTATION === "collapse-expand-contract") CONTRACT = [];

const STEPS = [
  // `expectOther` is asserted, not merely reported: every step states what the
  // NON-serving generation must do, so no step can pass vacuously.
  //   step 0 — the candidate MUST be broken. That is D-175's measured
  //            deploy-first outage and it is this harness's control: if the
  //            candidate came up clean here, the harness would not be looking.
  //   steps 1-4 — BOTH generations must serve. That is the whole deliverable.
  //   step 5 — the deployed generation is EXPECTED to stop serving username
  //            claims. Contracting is one-way by definition; a contract that
  //            left the old generation working would not have contracted.
  { n: 0, name: "baseline — production as it is today", apply: [], serving: "deployed", expectOther: "BROKEN" },
  { n: 1, name: "EXPAND  — apply migrations/ (additive only)", apply: EXPAND, serving: "deployed", expectOther: "SERVES" },
  { n: 2, name: "BACKFILL — repeatable UPDATE, no schema change", apply: ["#backfill"], serving: "deployed", expectOther: "SERVES" },
  { n: 3, name: "DEPLOY  — consuming Worker takes traffic (no schema change)", apply: [], serving: "candidate", expectOther: "SERVES" },
  { n: 4, name: "SETTLE  — wait + verify no NULL skeleton (no schema change)", apply: [], serving: "candidate", expectOther: "SERVES" },
  { n: 5, name: "CONTRACT — apply migrations-contract/", apply: CONTRACT, serving: "candidate", expectOther: "BROKEN" },
];

// ---------------------------------------------------------------------------
// database construction
// ---------------------------------------------------------------------------
const HEX = (n) => String(n).padStart(2, "0").repeat(32).slice(0, 64);

function freshDb(uptoStep) {
  const d = new DatabaseSync(":memory:");
  for (const o of SCHEMA_OBJECTS) d.exec(o.sql);
  for (const id of ["u-alice", "u-bob", "u-carol", "u-dave"]) {
    d.exec(`INSERT INTO users (user_id, ik_x25519_pub, ik_ed25519_pub, ik_mlkem768_pub,
      ik_x25519_signature, registered_at, identity_lookup_enabled)
      VALUES ('${id}','x','ed-${id}','m','s','2026-08-04T00:00:00Z',1)`);
  }
  d.exec(`INSERT INTO subscriptions (subscription_id, customer_id, customer_email, status, created_at, updated_at)
          VALUES ('s1','c1','c@example.invalid','ACTIVE',1,1)`);
  const applied = [];
  const migrationErrors = [];
  for (const step of STEPS) {
    if (step.n > uptoStep) break;
    for (const rel of step.apply) {
      try {
        if (rel === "#backfill") { applyBackfill(d); applied.push("backfill"); continue; }
        d.exec(migrationsFor(rel));
        applied.push(rel);
      } catch (e) {
        migrationErrors.push(`${rel}: ${e.message}`);
      }
    }
    // `--simulate-old-worker-after-expand` reproduces the hazard the contract
    // guard exists for: a pre-expand Worker that is STILL LIVE and writes a
    // username claim after the expand migration has landed. Its row carries no
    // skeleton, which is the evidence the guard refuses on.
    if (SIMULATE_OLD_WORKER && step.n === 3) {
      try {
        const digest = "dg-legacy";
        d.exec(`INSERT OR REPLACE INTO username_claim_receipts (user_id, request_digest, expires_at)
                VALUES ('u-dave', '${digest}', ${1785880000 + 600})`);
        d.prepare(D.claim).run("legacy", "u-dave", "OSLFR1.code", "2026-08-04T00:00:00Z", digest);
      } catch (e) { migrationErrors.push(`simulated old-Worker claim: ${e.message}`); }
    }
  }
  return { d, applied, migrationErrors };
}

// ---------------------------------------------------------------------------
// prepare sweep — every statement the generation can issue
// ---------------------------------------------------------------------------
function prepareFailures(d, list) {
  const failures = new Map();
  for (const raw of list) {
    // Two deployed statements interpolate a table or predicate. Substitute a
    // neutral predicate so they can be prepared; skip if still unresolvable.
    const sql = raw
      .replace(/\$\{ownsCurrentKey\}/g, "EXISTS (SELECT 1 FROM users WHERE user_id = ? AND ik_ed25519_pub = ?)")
      .replace(/\$\{sets\.join\(", "\)\}/g, "status = ?");
    if (sql.includes("${")) continue;
    try { d.prepare(sql).columns; } catch (e) {
      try { d.prepare(sql); } catch (e2) { failures.set(sql, e2.message); continue; }
    }
  }
  return failures;
}

// ---------------------------------------------------------------------------
// behaviour suites — statements lifted verbatim from each bundle
// ---------------------------------------------------------------------------
const NOW_ISO = "2026-08-04T00:00:00Z";
const NOW_S = 1785880000;

function tableExists(d, name) {
  return !!d.prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?").get(name);
}

function seedReceipt(d, userId, digest) {
  d.exec(`INSERT OR REPLACE INTO username_claim_receipts (user_id, request_digest, expires_at)
          VALUES ('${userId}', '${digest}', ${NOW_S + 600})`);
}

// A refusal is only acceptable if the generation's own catch turns it into the
// right status. `map` says what the generation does with the message.
function classify(generation, message) {
  if (generation === "deployed") {
    return /username_directory\.username|UNIQUE|PRIMARY/i.test(message) ? "409" : "500";
  }
  return /username_directory\.username|username is retired|UNIQUE|PRIMARY/i.test(message) ? "409" : "500";
}

function runSuite(d, generation) {
  const S = generation === "deployed" ? D : C;
  const problems = [];
  const note = [];
  // D-248 made the consuming Worker's claim take a sixth parameter: the UTS #39
  // skeleton, which cannot be computed in SQL. Bind by the statement's OWN
  // arity rather than a hardcoded count, so this harness keeps working against
  // a downloaded bundle from either side of that change instead of reporting
  // the newer generation BROKEN for a binding mistake of its own making.
  const claimArity = (sql) =>
    Math.max(0, ...[...sql.matchAll(/\?(\d+)/g)].map((m) => Number(m[1])));
  const claim = (name, user) => {
    const digest = `dg-${name}-${user}`;
    seedReceipt(d, user, digest);
    try {
      const args = [name, user, "OSLFR1.code", NOW_ISO, digest];
      if (claimArity(S.claim) >= 6) args.push(usernameSkeleton(name));
      const r = d.prepare(S.claim).run(...args);
      return { changes: r.changes, status: r.changes === 1 ? "200" : "409" };
    } catch (e) {
      return { changes: 0, status: classify(generation, e.message), error: e.message };
    }
  };
  const expect = (label, cond, detail) => {
    if (!cond) problems.push(`FAILED: ${label}${detail ? ` — ${detail}` : ""}`);
    else note.push(`${label}: ok`);
  };

  // 1. the first two claims ever. This is the exact pair D-167 measured.
  const c1 = claim("alice", "u-alice");
  expect("claim #1 (alice) succeeds", c1.status === "200", JSON.stringify(c1));
  const c2 = claim("bob", "u-bob");
  expect("claim #2 (bob) succeeds", c2.status === "200", JSON.stringify(c2));

  // 2. a rename retires a name (the tombstone path)
  let renamed = true;
  try {
    d.prepare(S.renameDelete).run("u-alice", "alice2", "dg-alice-u-alice");
  } catch (e) { renamed = false; problems.push(`rename delete aborted — ${e.message}`); }
  if (renamed) {
    // the rename delete only fires when the user already holds a different name;
    // drive the retirement path directly, as rotate/unregister do.
    try {
      d.exec("DELETE FROM username_directory WHERE username = 'alice'");
      note.push("retire (rotate/unregister delete): ok");
    } catch (e) { problems.push(`retire delete aborted — ${e.message}`); }
  }

  // 3. a fresh claim AFTER a retirement — the case that turned every claim into
  //    a 409 under the original migration.
  const c3 = claim("carol", "u-carol");
  expect("claim #3 (carol) after a retirement succeeds", c3.status === "200", JSON.stringify(c3));

  // 4. re-claiming the retired name. The retirement rule only exists once the
  //    expand migration has created username_tombstones; before that, names are
  //    reusable and that IS today's production behaviour. Asserting the future
  //    spec against the pre-migration schema would be testing the wrong thing.
  const retirementLive = tableExists(d, "username_tombstones");
  const c4 = claim("alice", "u-dave");
  if (retirementLive) {
    expect("retired name is refused", c4.status !== "200", JSON.stringify(c4));
    expect("retired name refusal is a 409, not a 500", c4.status === "409", JSON.stringify(c4));
  } else {
    note.push("retired name reusable (pre-expand schema; no tombstone table yet)");
  }

  // 5. lookup still resolves
  try {
    const got = d.prepare(S.lookup).get("bob");
    expect("username lookup resolves", got && got.user_id === "u-bob", JSON.stringify(got));
  } catch (e) { problems.push(`username lookup failed — ${e.message}`); }

  // 6. licenses: the SELECT * the deployed Worker really issues
  try {
    d.prepare(S.licenseInsert ?? D.licenseInsert).run("h1", "s1", NOW_S);
  } catch (e) { problems.push(`license insert failed — ${e.message}`); }
  try {
    const lic = d.prepare(S.licenseSelect).get("h1");
    expect("licenses read still yields the fields the Worker consumes",
      lic && lic.revoked_at === null && lic.subscription_id === "s1", JSON.stringify(lic));
    if (generation === "deployed") {
      // 0040 adds four nullable columns and the deployed Worker reads the row
      // with `SELECT *`. Its two callers destructure `license.revoked_at` and
      // `license.subscription_id` only (handleLicenseValidate, handleBillingPortal
      // in the downloaded bundle); nothing enumerates keys. Record what the row
      // actually carries so "inert" is measured, not asserted.
      note.push(`deployed SELECT * row keys: ${Object.keys(lic).join(",")}`);
    }
  } catch (e) { problems.push(`license select failed — ${e.message}`); }
  if (generation === "candidate") {
    try {
      d.prepare(C.licenseRedeem).run(NOW_S, NOW_S, "h1");
      note.push("license redemption update: ok");
    } catch (e) { problems.push(`license redeem failed — ${e.message}`); }
  }

  // 7. account ownership
  if (generation === "deployed") {
    try {
      d.prepare(D.challengeInsert).run(HEX(11), HEX(12), NOW_S, NOW_S + 600);
      note.push("account-ownership challenge insert: ok");
    } catch (e) { problems.push(`challenge insert failed — ${e.message}`); }
  } else {
    d.exec(`INSERT INTO account_ownership_challenges
       (nonce_sha256, binding_sha256, service, issued_at_unix_seconds, expires_at_unix_seconds, spent_at_unix_seconds)
       VALUES ('${HEX(21)}','${HEX(22)}','discord',${NOW_S},${NOW_S + 600},${NOW_S})`);
    try {
      d.prepare(C.bindingInsert).run(HEX(22), HEX(21), "u-bob", HEX(41), "ed25519_identity_challenge_v1", NOW_S);
      note.push("account-ownership proof binding insert: ok");
    } catch (e) { problems.push(`binding insert failed — ${e.message}`); }
  }

  return { problems, note };
}

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------
console.log("D-175 expand/contract step-order test");
console.log(`  mutation      : ${MUTATION}`);
console.log(`  live schema   : ${args.schema} (${SCHEMA_OBJECTS.length} objects)`);
console.log(`  deployed sql  : ${DEPLOYED_SQL.length} statements from ${args.deployed}`);
console.log(`  candidate sql : ${CANDIDATE_SQL.length} statements from ${args.candidate}`);
console.log("\n--- harness preconditions (a suite that cannot fail is decoration) ---");
let precondOk = true;
for (const p of preconditions) {
  console.log(`  [${p.ok ? "ok " : "FAIL"}] ${p.label} — ${p.detail}`);
  if (!p.ok) precondOk = false;
}
if (!precondOk) {
  console.log("\nFATAL: the artifacts no longer support this suite's assumptions. Stopping.");
  process.exit(2);
}

// IRREDUCIBLE = statements that cannot be prepared against the D1 schema even
// when EVERY step has been applied. They are not D1 statements at all: the OSL
// Mail Durable Object carries its own SQLite (messages, request_receipts,
// send_buckets, archive_entries, mailbox_state), and two literals are dynamic
// SQL fragments. Subtracting them is not an allowlist — it is the difference
// between "this migration broke it" and "this was never a D1 statement". The
// set is printed in full so the subtraction is auditable.
const IRREDUCIBLE = {};
{
  const { d } = freshDb(99);
  IRREDUCIBLE.deployed = prepareFailures(d, DEPLOYED_SQL);
  IRREDUCIBLE.candidate = prepareFailures(d, CANDIDATE_SQL);
}
console.log(`\nirreducible non-D1 statements (fail even on the fully migrated schema):`);
console.log(`  deployed  : ${IRREDUCIBLE.deployed.size}`);
console.log(`  candidate : ${IRREDUCIBLE.candidate.size}`);
const irrTables = new Set();
for (const m of [...IRREDUCIBLE.deployed.values(), ...IRREDUCIBLE.candidate.values()]) irrTables.add(m);
for (const m of [...irrTables].sort()) console.log(`      ${m}`);

{
  const { d } = freshDb(-1);
  const pfD = prepareFailures(d, DEPLOYED_SQL);
  const pfC = prepareFailures(d, CANDIDATE_SQL);
  const newD = [...pfD.keys()].filter((k) => !IRREDUCIBLE.deployed.has(k));
  const newC = [...pfC.keys()].filter((k) => !IRREDUCIBLE.candidate.has(k));
  console.log(`\ncontrol — the two generations against TODAY'S live schema, nothing applied:`);
  console.log(`  deployed  : ${newD.length} unpreparable   <- it is what production runs, so this must be 0`);
  console.log(`  candidate : ${newC.length} unpreparable   <- D-175's "deploy first is also an outage"`);
  for (const k of newC) console.log(`      ${pfC.get(k)}  ::  ${k.split("\n")[0].slice(0, 84)}`);
}

const verdicts = [];
for (const step of STEPS) {
  console.log(`\n=== STEP ${step.n} · ${step.name} ===`);
  const { d, applied, migrationErrors } = freshDb(step.n);
  console.log(`  schema state: ${applied.length ? applied.join(", ") : "(live, nothing applied)"}`);
  for (const me of migrationErrors) console.log(`  MIGRATION REFUSED/FAILED: ${me}`);
  const stepResult = { step: step.n, name: step.name, serving: step.serving, generations: {} };
  for (const generation of ["deployed", "candidate"]) {
    const { d: db2, migrationErrors: me2 } = freshDb(step.n);
    const pf = prepareFailures(db2, generation === "deployed" ? DEPLOYED_SQL : CANDIDATE_SQL);
    const { problems, note } = runSuite(db2, generation);
    const unresolved = [...pf.keys()].filter((k) => !IRREDUCIBLE[generation].has(k));
    stepResult.generations[generation] = {
      unpreparable: unresolved.length,
      problems,
      migrationErrors: me2,
      clean: unresolved.length === 0 && problems.length === 0 && me2.length === 0,
    };
    const g = stepResult.generations[generation];
    console.log(`  ${generation.padEnd(9)} : ${g.clean ? "SERVES" : "BROKEN"}  (unpreparable ${unresolved.length}, behaviour problems ${problems.length})`);
    for (const u of unresolved.slice(0, 14)) {
      console.log(`      cannot prepare: ${pf.get(u)}  ::  ${u.split("\n")[0].slice(0, 74)}`);
    }
    for (const p of problems) console.log(`      behaviour: ${p}`);
  }
  verdicts.push(stepResult);
  d.close();
}

// ---------------------------------------------------------------------------
// the property under test
// ---------------------------------------------------------------------------
console.log("\n=== THE PROPERTY: stop after any step and production still works ===");
let allOk = true;
for (const v of verdicts) {
  const serving = v.generations[v.serving].clean;
  const other = v.generations[v.serving === "deployed" ? "candidate" : "deployed"].clean;
  const step = STEPS.find((s) => s.n === v.step);
  const wantOther = step.expectOther === "SERVES";
  const ok = serving && other === wantOther;
  if (!ok) allOk = false;
  console.log(
    `  step ${v.step}: serving=${v.serving} ${serving ? "SERVES" : "BROKEN"}` +
    `  | other generation ${other ? "SERVES" : "BROKEN"} (required ${step.expectOther})` +
    `  -> ${ok ? "PASS" : "FAIL"}`,
  );
}
console.log(`\nVERDICT (mutation=${MUTATION}): ${allOk ? "GREEN — every intermediate state is serveable" : "RED — at least one intermediate state is an outage"}`);
