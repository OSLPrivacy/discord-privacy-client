#!/usr/bin/env node
import {
  cpSync, existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, symlinkSync, writeFileSync,
} from "node:fs";
import { spawnSync } from "node:child_process";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(import.meta.url);
const repository = resolve(import.meta.dirname, "../..");
const store = resolve(repository, "cipher-store-cf");
const prefix = ".task-5215b-";
const gateCommit = "ebcdd1dfc";
const testFile = "cipher-store-cf/test/task-5215-upload-capacity.test.ts";
const capFile = "cipher-store-cf/src/lib/upload-capacity.ts";
const blobFile = "cipher-store-cf/src/endpoints/blob.ts";
const routeFile = "cipher-store-cf/src/index.ts";

const replaceOnce = (source, before, after, id) => {
  const first = source.indexOf(before);
  if (first < 0) throw new Error(`${id}: mutation anchor absent`);
  if (source.indexOf(before, first + before.length) >= 0) throw new Error(`${id}: mutation anchor ambiguous`);
  return source.slice(0, first) + after + source.slice(first + before.length);
};

const edit = (candidate, relative, before, after, id) => {
  const path = resolve(candidate, relative);
  const source = readFileSync(path, "utf8");
  writeFileSync(path, replaceOnce(source, before, after, id));
};

const MUTANTS = [
  { id: "charge_true_tiny_size", file: capFile, find: "UPLOAD_REQUEST_FLOOR_BYTES = 1_000_000", put: "UPLOAD_REQUEST_FLOOR_BYTES = 1", test: "charges max", diag: "pack=$5 operation=true_tiny_floor remaining_signed_bytes=2000000" },
  { id: "restore_deleted_capacity", file: blobFile, find: 'await env.DB.prepare("DELETE FROM blob_capability_index WHERE blob_id = ?").bind(id).run();', put: 'await env.DB.prepare("DELETE FROM blob_capability_index WHERE blob_id = ?").bind(id).run();\n      await env.DB.prepare(`UPDATE upload_capacity_reservations SET spent_bytes=spent_bytes-(SELECT debit_bytes FROM upload_capacity_claims WHERE object_id=?) WHERE grant_id=(SELECT grant_id FROM upload_capacity_claims WHERE object_id=?)`).bind(id,id).run();', test: "keeps deletion", diag: "pack=$5 operation=deletion_no_restoration remaining_signed_bytes=3000000" },
  { id: "omit_deleted_object", file: blobFile, find: 'await env.DB.prepare("DELETE FROM blob_capability_index WHERE blob_id = ?").bind(id).run();', put: "void id;", test: "keeps deletion", diag: "pack=$5 operation=deletion_wrong_object remaining_signed_bytes=3000000" },
  { id: "delete_wrong_object", file: blobFile, find: '.bind(id).run();\n    },\n  }, blobId', put: '.bind("0000000000000000000000000000044c").run();\n    },\n  }, blobId', test: "keeps deletion", diag: "pack=$5 operation=deletion_wrong_object remaining_signed_bytes=3000000" },
  { id: "remove_shipping_hook", file: routeFile, find: "handleUpload(request, env, grant.value)", put: "handleUpload(request, env)", test: "voucher 5", diag: "pack=$5 operation=16401_first_refused remaining_signed_bytes=0" },
  { id: "accept_local_credential", file: capFile, find: 'if (!header?.startsWith(`${scheme} `))', put: "if (!header || header.length < scheme.length + 1)", test: "accepts only exact", diag: "pack=$5 operation=local_credential_type remaining_signed_bytes=2000000" },
  { id: "accept_missing_reservation", file: capFile, find: "if (!matches) return refusal(401, \"upload_reservation_required\", \"exact upload reservation required\");", put: "if (false && !matches) return refusal(401, \"upload_reservation_required\", \"exact upload reservation required\");", test: "accepts only exact", diag: "pack=$5 operation=missing_reservation remaining_signed_bytes=2000000" },
  { id: "accept_swapped_reservation", file: capFile, find: "WHERE reservation_id=? AND grant_id=?", put: "WHERE reservation_id=? AND ? IS NOT NULL", test: "accepts only exact", diag: "pack=$5 operation=swapped_reservation remaining_signed_bytes=2000000" },
  { id: "reuse_reservation", file: capFile, find: "ON CONFLICT(reservation_id) DO NOTHING", put: "ON CONFLICT(reservation_id) DO UPDATE SET grant_id=excluded.grant_id,authority=excluded.authority,capacity_bytes=excluded.capacity_bytes,base_expiry=excluded.base_expiry,effective_expiry=excluded.effective_expiry,outage_extension_seconds=excluded.outage_extension_seconds,signed_reservation=excluded.signed_reservation", test: "accepts only exact", diag: "pack=$5 operation=reused_reservation remaining_signed_bytes=2000000" },
  { id: "accept_swapped_type", file: capFile, find: "if (!claims || schema !== expectedSchema)", put: "if (!claims)", test: "accepts only exact", diag: "pack=monthly operation=swapped_reservation_type remaining_signed_bytes=2000000" },
  { id: "add_store_identity", file: capFile, find: "if (!exactKeys(claims, expectedKeys)) return refusal", put: "if (false && !exactKeys(claims, expectedKeys)) return refusal", test: "accepts only exact", diag: "pack=$5 operation=identity_join remaining_signed_bytes=2000000" },
  { id: "reject_valid_monthly", file: capFile, find: "? env.MONTHLY_UPLOAD_AUTHORITY_PUBKEY_B64\n    : env.SOLD_UPLOAD_AUTHORITY_PUBKEY_B64", put: "? undefined\n    : env.SOLD_UPLOAD_AUTHORITY_PUBKEY_B64", test: "uses the separate", diag: "pack=monthly operation=normal_monthly_route remaining_signed_bytes=1000000" },
  { id: "sold_base_expiry_only", file: capFile, find: "r.effective_expiry>?", put: "r.base_expiry>?", test: "uses signed effective", diag: "pack=sold operation=post_base_expiry_real_upload remaining_signed_bytes=2000000" },
  { id: "shorten_signed_outage", file: capFile, find: "r.effective_expiry>?", put: "r.effective_expiry-r.outage_extension_seconds>?", test: "uses signed effective", diag: "pack=sold operation=post_base_expiry_real_upload remaining_signed_bytes=2000000" },
  { id: "display_only_remainder", file: capFile, find: "remainingBytes: signedBytes - spentBytes - heldBytes", put: "remainingBytes: signedBytes - spentBytes - heldBytes + 1", test: "charges max", diag: "pack=$5 operation=displayed_store_disagreement remaining_signed_bytes=1000000" },
  { id: "both_barrier_uploads_write", file: capFile, find: ">= ?\n    ON CONFLICT(object_id)", put: ">= 0*?\n    ON CONFLICT(object_id)", test: "lets exactly", diag: "pack=$5 operation=barrier_double_success remaining_signed_bytes=0" },
  { id: "serialise_barrier", file: testFile, find: "const firstParticipant = upload(value, 400, { body: participant() });\n    // TASK5215_BARRIER_PARTICIPANT_B\n    const secondParticipant = upload(value, 401, { body: participant() });\n    const responses = await Promise.all([firstParticipant, secondParticipant]);", put: "const firstParticipant = await upload(value, 400, { body: participant() });\n    // TASK5215_BARRIER_PARTICIPANT_B\n    const secondParticipant = await upload(value, 401, { body: participant() });\n    const responses = [firstParticipant, secondParticipant];", test: "lets exactly", diag: "pack=$5 operation=serial_starved_barrier remaining_signed_bytes=1000000" },
  { id: "starve_barrier_participant", file: testFile, find: "const secondParticipant = upload(value, 401, { body: participant() });", put: "const secondParticipant = Promise.resolve(new Response(null, { status: 402 }));", test: "lets exactly", diag: "pack=$5 operation=serial_starved_barrier remaining_signed_bytes=1000000" },
  { id: "debit_only_state", file: blobFile, edits: [
    ["WHERE object_id=? AND grant_id=? AND state='pending'`).bind(\n          headers.blobId", "WHERE 0 AND object_id=? AND grant_id=? AND state='pending'`).bind(\n          headers.blobId"],
    ["if ((results[0]?.meta?.changes ?? 0) !== 1", "if (false"],
  ], test: "commits neither", diag: "pack=$5 operation=debit_only_atomic_state remaining_signed_bytes=1000000" },
  { id: "object_only_state", file: blobFile, edits: [
    ["SET spent_bytes=spent_bytes+?\n          WHERE grant_id=?", "SET spent_bytes=spent_bytes+?\n          WHERE 0 AND grant_id=?"],
    ["|| (results[1]?.meta?.changes ?? 0) !== 1", "|| false"],
  ], test: "commits neither", diag: "pack=$5 operation=object_only_atomic_state remaining_signed_bytes=2000000" },
  { id: "retain_16400_ceiling_15", file: blobFile, find: "if (uploadGrant) {", put: 'if (uploadGrant?.capacityBytes === 51_600_000_000 && BigInt(`0x${headers.blobId}`) >= 16_400n) return error(402, "upload_capacity_exhausted", "mutant ceiling");\n  if (uploadGrant) {', test: "voucher 15", diag: "pack=$15 operation=51600_final_allowed remaining_signed_bytes=1000000" },
  { id: "retain_16400_ceiling_40", file: blobFile, find: "if (uploadGrant) {", put: 'if (uploadGrant?.capacityBytes === 139_600_000_000 && BigInt(`0x${headers.blobId}`) >= 16_400n) return error(402, "upload_capacity_exhausted", "mutant ceiling");\n  if (uploadGrant) {', test: "voucher 40", diag: "pack=$40 operation=139600_final_allowed remaining_signed_bytes=1000000" },
  { id: "accept_51601_after_exhaustion", file: capFile, find: ">= ?\n    ON CONFLICT(object_id)", put: ">= 0*?\n    ON CONFLICT(object_id)", test: "voucher 15", diag: "pack=$15 operation=51601_first_refused remaining_signed_bytes=0" },
  { id: "accept_139601_after_exhaustion", file: capFile, find: ">= ?\n    ON CONFLICT(object_id)", put: ">= 0*?\n    ON CONFLICT(object_id)", test: "voucher 40", diag: "pack=$40 operation=139601_first_refused remaining_signed_bytes=0" },
  { id: "borrow_5_pack_evidence", file: testFile, find: "equalIs(value.capacityBytes, row.bytes, `$${row.usd}`, \"independent_pack_evidence\", row.bytes);", put: "equalIs(PACK_ROWS[0].bytes, row.bytes, `$${row.usd}`, \"independent_pack_evidence\", row.bytes);", test: "voucher 15", diag: "pack=$15 operation=independent_pack_evidence remaining_signed_bytes=51600000000" },
  { id: "hidden_route_count_cutoff", file: blobFile, find: "if (uploadGrant) {", put: 'if (uploadGrant?.capacityBytes === 51_600_000_000 && BigInt(`0x${headers.blobId}`) > 51_599n) return error(402, "upload_capacity_exhausted", "hidden route cutoff");\n  if (uploadGrant) {', test: "voucher 15", diag: "pack=$15 operation=51600_final_allowed remaining_signed_bytes=1000000" },
  { id: "hidden_claim_count_cutoff", file: capFile, find: "AND r.capacity_bytes=? AND r.effective_expiry=?", put: "AND r.capacity_bytes=? AND r.effective_expiry=? AND (r.capacity_bytes!=139600000000 OR ?<139600)", bind: true, test: "voucher 40", diag: "pack=$40 operation=139600_final_allowed remaining_signed_bytes=1000000" },
];

const EXPECTED = MUTANTS.map(({ id }) => id);
const CONTROLS = [
  ["catalogue_5", "{ usd: 5, bytes: 16_400_000_000, objects: 16_400 }"],
  ["catalogue_15", "{ usd: 15, bytes: 51_600_000_000, objects: 51_600 }"],
  ["catalogue_40", "{ usd: 40, bytes: 139_600_000_000, objects: 139_600 }"],
  ["terminal_final", "`${expectedObjects}_final_allowed`"],
  ["terminal_refused", "`${expectedObjects + 1}_first_refused`"],
  ["object_16401", "object_16401_class_a"],
  ["floor_tiny", "true_tiny_floor"],
  ["floor_terminal", "floor_exhaustion"],
  ["monthly_journey", "normal_monthly_route"],
  ["sold_outage_journey", "post_base_expiry_real_upload"],
  ["barrier_a", "TASK5215_BARRIER_PARTICIPANT_A"],
  ["barrier_b", "TASK5215_BARRIER_PARTICIPANT_B"],
  ["deletion", "deletion_no_restoration"],
  ["atomic", "debit_only_atomic_state"],
];

function verifyInventory(mutants = MUTANTS) {
  for (const id of EXPECTED) {
    if (!mutants.some((entry) => entry.id === id)) throw new Error(`absent_mutation:${id}`);
  }
  if (mutants.length !== EXPECTED.length) throw new Error(`mutation_count expected=${EXPECTED.length} actual=${mutants.length}`);
}

function makeCandidate() {
  const root = mkdtempSync(join(repository, prefix));
  const candidateStore = join(root, "cipher-store-cf");
  const archive = join(root, "task-5215-gate.tar");
  const archived = spawnSync("git", ["archive", "--format=tar", `--output=${archive}`, gateCommit, "cipher-store-cf", "keyserver-cf"], {
    cwd: repository, encoding: "utf8",
  });
  if (archived.status !== 0) throw new Error(`gate_archive_failed:${archived.stderr ?? archived.stdout}`);
  const extracted = spawnSync("tar", ["-xf", archive, "-C", root], { cwd: repository, encoding: "utf8" });
  rmSync(archive, { force: true });
  if (extracted.status !== 0) throw new Error(`gate_extract_failed:${extracted.stderr ?? extracted.stdout}`);

  // Exercise the task-owned production files from this lane on the clean 5215
  // shipping tree. The surrounding checkout contains unrelated merge debris,
  // so copying the whole live store would prevent Vitest from reaching 5215.
  for (const relative of [
    "cipher-store-cf/migrations/0022_upload_capacity_authorities.sql",
    capFile,
    blobFile,
    testFile,
  ]) cpSync(resolve(repository, relative), resolve(root, relative));
  symlinkSync(join(store, "node_modules"), join(candidateStore, "node_modules"), "dir");
  return { root, candidateStore };
}

function applyMutant(candidateRoot, mutant) {
  const file = resolve(candidateRoot, mutant.file);
  if (mutant.edits) {
    for (const [before, after] of mutant.edits) edit(candidateRoot, mutant.file, before, after, mutant.id);
  } else {
    edit(candidateRoot, mutant.file, mutant.find, mutant.put, mutant.id);
  }
  if (mutant.bind) {
    const source = readFileSync(file, "utf8");
    writeFileSync(file, replaceOnce(source,
      "grant.authority, grant.capacityBytes, grant.effectiveExpiry, now, debitBytes,",
      "grant.authority, grant.capacityBytes, grant.effectiveExpiry, objectId === '00000000000000000000000000022150' ? 139600 : 0, now, debitBytes,",
      `${mutant.id}_binding`));
  }
}

function runVitest(candidateStore, selector) {
  // Vitest treats -t as a regular expression. Escape diagnostic selectors so
  // voucher names such as "$5" select the journey instead of matching zero
  // tests (a zero-test selector otherwise exits green).
  const exactSelector = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return spawnSync(process.execPath, ["./node_modules/vitest/vitest.mjs", "run", "test/task-5215-upload-capacity.test.ts", "-t", exactSelector, "--reporter=verbose"], {
    cwd: candidateStore,
    encoding: "utf8",
    env: { ...process.env, CI: "1" },
    timeout: 180_000,
    maxBuffer: 16 * 1024 * 1024,
  });
}

function copiesRemaining() {
  return readdirSync(repository).filter((name) => name.startsWith(prefix)).length;
}

const mode = process.argv[2];
if (mode === "--candidate") {
  const id = process.argv[3];
  const mutant = MUTANTS.find((entry) => entry.id === id);
  if (!mutant) throw new Error(`unknown mutant ${id}`);
  verifyInventory();
  let candidate;
  try {
    candidate = makeCandidate();
    applyMutant(candidate.root, mutant);
    const result = runVitest(candidate.candidateStore, mutant.test);
    const output = `${result.stdout ?? ""}\n${result.stderr ?? ""}`;
    if (result.status === 0) {
      console.error(output.split("\n").slice(-80).join("\n"));
      console.error(`TASK5215B_MUTANT id=${id} unexpectedly_green=1`);
      process.exitCode = 2;
    } else if (!output.includes(`TASK5215B_DIAGNOSTIC ${mutant.diag}`)) {
      console.error(output.split("\n").slice(-80).join("\n"));
      console.error(`TASK5215B_MUTANT id=${id} wrong_failure=1 expected=${JSON.stringify(mutant.diag)}`);
      process.exitCode = 2;
    } else {
      console.log(`TASK5215B_MUTANT id=${id} exit=1 ${mutant.diag} candidate_discarded=1`);
      process.exitCode = 1;
    }
  } finally {
    if (candidate) rmSync(candidate.root, { recursive: true, force: true });
  }
} else if (mode === "--starve") {
  const kind = process.argv[3];
  const id = process.argv[4];
  try {
    if (kind === "mutation") {
      verifyInventory(MUTANTS.filter((entry) => entry.id !== id));
    } else if (kind === "control") {
      const control = CONTROLS.find(([name]) => name === id);
      if (!control) throw new Error(`unknown_control:${id}`);
      let candidate;
      try {
        candidate = makeCandidate();
        const path = resolve(candidate.root, testFile);
        const source = readFileSync(path, "utf8");
        if (!source.includes(control[1])) throw new Error(`starve_${id}: anchor absent`);
        writeFileSync(path, source.split(control[1]).join(`STARVED_${id}`));
        const audit = spawnSync(process.execPath, [resolve(repository, "scripts/qa/task-5215b.mjs")], {
          cwd: repository, encoding: "utf8", env: { ...process.env, TASK5215_ROOT: candidate.root },
        });
        const output = `${audit.stdout ?? ""}${audit.stderr ?? ""}`;
        if (audit.status !== 1 || !output.includes("TASK5215B_EXIT=1")) throw new Error(`control_not_red:${id}`);
      } finally {
        if (candidate) rmSync(candidate.root, { recursive: true, force: true });
      }
      throw new Error(`absent_control:${id}`);
    } else throw new Error(`unknown_starvation:${kind}:${id}`);
    process.exitCode = 2;
  } catch (error) {
    const message = String(error);
    if (!message.includes(`absent_${kind}:${id}`)) {
      console.error(`TASK5215B_STARVATION kind=${kind} id=${id} wrong_failure=${JSON.stringify(message)}`);
      process.exitCode = 2;
    } else {
      console.log(`TASK5215B_STARVATION kind=${kind} id=${id} exit=1 named=${JSON.stringify(message)} discarded=1`);
      process.exitCode = 1;
    }
  }
} else if (mode === "--restored") {
  verifyInventory();
  const audit = spawnSync(process.execPath, [resolve(repository, "scripts/qa/task-5215b.mjs")], { cwd: repository, encoding: "utf8", env: process.env });
  let candidate;
  try {
    candidate = makeCandidate();
    const green = runVitest(candidate.candidateStore, "");
    if (audit.status !== 0 || green.status !== 0) {
      process.stderr.write(`${audit.stdout ?? ""}${audit.stderr ?? ""}${green.stdout ?? ""}${green.stderr ?? ""}`);
      process.exitCode = 2;
    } else {
      for (const line of (green.stdout ?? "").split("\n").filter((line) => line.includes("TASK5215_"))) console.log(line.trim());
      console.log("TASK5215B_RESTORED exit=0 tests=12 packs=3 authorities=2 candidate_discarded=1");
    }
  } finally {
    if (candidate) rmSync(candidate.root, { recursive: true, force: true });
    if (process.exitCode !== 2) console.log(`TASK5215B_RESTORED_CLEAN copies_remaining=${copiesRemaining()}`);
  }
} else {
  verifyInventory();
  let starvationRed = 0;
  for (const [kind, ids] of [["mutation", EXPECTED], ["control", CONTROLS.map(([id]) => id)]]) {
    for (const id of ids) {
      const result = spawnSync(process.execPath, [script, "--starve", kind, id], { cwd: repository, encoding: "utf8", env: process.env });
      process.stdout.write(result.stdout ?? "");
      process.stderr.write(result.stderr ?? "");
      if (result.status !== 1) throw new Error(`starvation failed kind=${kind} id=${id} status=${result.status}`);
      starvationRed++;
    }
  }
  let red = 0;
  for (const id of EXPECTED) {
    const result = spawnSync(process.execPath, [script, "--candidate", id], { cwd: repository, encoding: "utf8", env: process.env, timeout: 240_000, maxBuffer: 16 * 1024 * 1024 });
    process.stdout.write(result.stdout ?? "");
    process.stderr.write(result.stderr ?? "");
    if (result.status !== 1) throw new Error(`mutant failed id=${id} status=${result.status}`);
    red++;
  }
  const restored = spawnSync(process.execPath, [script, "--restored"], { cwd: repository, encoding: "utf8", env: process.env, timeout: 240_000, maxBuffer: 16 * 1024 * 1024 });
  process.stdout.write(restored.stdout ?? "");
  process.stderr.write(restored.stderr ?? "");
  if (restored.status !== 0) throw new Error(`restored status=${restored.status}`);
  const remaining = copiesRemaining();
  if (remaining !== 0) throw new Error(`copies_remaining=${remaining}`);
  console.log(`TASK5215B_RED_PROOF exit=0 mutants_red=${red} starvation_red=${starvationRed} restored_green=1 candidates_discarded=${red + CONTROLS.length} copies_remaining=0 discovered_count_guards=2 cutoff_mutants=4`);
}
