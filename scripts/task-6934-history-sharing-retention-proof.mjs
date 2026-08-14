#!/usr/bin/env node
/**
 * TASK 6934 — hostile copies of the 6933 forward-only history implementation.
 *
 * Each case gets its own directory.  The 6933 test file is copied verbatim;
 * only the production implementation is changed.  This deliberately tests
 * the joiner's view, rather than accepting a producer-side claim that a key
 * was not retained or that a relay did not re-share it.
 */
import assert from "node:assert/strict";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const ui = join(root, "apps/osl-hub-ui");
const history = "src/enclave-history-settings.ts";
const disclosures = "src/enclave-disclosures.ts";
const vitest = join(ui, "node_modules/vitest/vitest.mjs");
const gate = "src/task-6933-forward-history-reshare.test.ts";

const requiredSourceWitnesses = [
  "firstApplicant", "secondApplicant", "thirdApplicant", "before-signed-epoch",
  "Alex Rivera", "ENCLAVE_HISTORY_RESHARE_SINK_INVENTORY", "Already shared items cannot be recalled.",
];
const requiredOutputWitnesses = [
  "first applicant", "third applicant", "member-alex", "signed_epoch=50", "inventory=3",
];

function replaceOnce(source, needle, replacement, caseName) {
  const at = source.indexOf(needle);
  assert.notEqual(at, -1, `TASK6934 absent case=${caseName} anchor=${needle}`);
  return `${source.slice(0, at)}${replacement}${source.slice(at + needle.length)}`;
}

function replaceAllRequired(source, needle, replacement, caseName) {
  assert.ok(source.includes(needle), `TASK6934 absent case=${caseName} anchor=${needle}`);
  return source.replaceAll(needle, replacement);
}

function copyUi(caseName) {
  const dir = mkdtempSync(join(tmpdir(), `osl-task-6934-${caseName}-`));
  cpSync(join(ui, "src"), join(dir, "src"), { recursive: true });
  cpSync(join(ui, "vitest.config.cjs"), join(dir, "vitest.config.cjs"));
  symlinkSync(join(ui, "node_modules"), join(dir, "node_modules"), "dir");
  return dir;
}

function run6933(dir) {
  return spawnSync(process.execPath, [vitest, "run", gate, "--config", "vitest.config.cjs", "--maxWorkers=1", "--minWorkers=1"], {
    cwd: dir,
    encoding: "utf8",
    env: { ...process.env, CI: "1" },
  });
}

/** The right side is the witness emitted by this proof after the child is red. */
const cases = [
  ["key-retention", "epoch=50 message=before-signed-epoch-1", (s) => replaceOnce(s, "ciphertext: encryptToCurrentJoinerKey(message.plaintextOnSharerDevice, joiner.currentKeyId),", "ciphertext: encryptToCurrentJoinerKey(message.plaintextOnSharerDevice, joiner.currentKeyId),\n      archivedEpochMessageKey: \"prejoin-key\",", "key-retention")],
  ["debug-cache", "epoch=50 message=post-1 sink=winning-epoch-debug-cache", (s) => replaceOnce(s, "ciphertext: encryptToCurrentJoinerKey(message.plaintextOnSharerDevice, joiner.currentKeyId),", "ciphertext: encryptToCurrentJoinerKey(message.plaintextOnSharerDevice, joiner.currentKeyId),\n      winningEpochDebugMessageKey: \"cached-key\",", "debug-cache")],
  ["opaque-blob", "epoch=50 message=post-1 sink=opaque-encrypted-blob", (s) => replaceOnce(s, "ciphertext: encryptToCurrentJoinerKey(message.plaintextOnSharerDevice, joiner.currentKeyId),", "ciphertext: encryptToCurrentJoinerKey(message.plaintextOnSharerDevice, joiner.currentKeyId),\n      opaqueBlobChainKey: \"sealed-key\",", "opaque-blob")],
  ["operator-backup", "epoch=50 message=post-1 sink=operator-backup", (s) => replaceOnce(s, "ciphertext: encryptToCurrentJoinerKey(message.plaintextOnSharerDevice, joiner.currentKeyId),", "ciphertext: encryptToCurrentJoinerKey(message.plaintextOnSharerDevice, joiner.currentKeyId),\n      operatorBackupRootKey: \"backup-key\",", "operator-backup")],
  ["pre-epoch-inclusion", "epoch=50 message=before-signed-epoch-1", (s) => replaceOnce(s, "if (message.committedSendEpoch < record.sharingStartedAtEpoch)", "if (false && message.committedSendEpoch < record.sharingStartedAtEpoch)", "pre-epoch-inclusion")],
  ["requested-pre-epoch", "epoch=30 message=before-signed-epoch-1", (s) => replaceOnce(s, "return new Uint8Array(0);", "return new Uint8Array([message.committedSendEpoch]);", "requested-pre-epoch")],
  ["back-dated-epoch", "epoch=50 message=before-signed-epoch-1", (s) => replaceOnce(s, "sharingStartedAtEpoch: enabled ? instruction.signedEpoch : current.sharingStartedAtEpoch,", "sharingStartedAtEpoch: enabled ? instruction.signedEpoch - 30 : current.sharingStartedAtEpoch,", "back-dated-epoch")],
  ["relay-re-share", "epoch=50 message=post-1 sharer=relay", (s) => replaceOnce(s, "sharerMemberId: sharer.memberId,", "sharerMemberId: \"relay\",", "relay-re-share")],
  ["relay-helper", "epoch=50 message=post-1 sharer=relay-side-helper", (s) => replaceOnce(s, "sharerDeviceId: sharer.deviceId,", "sharerDeviceId: \"relay-side-helper\",", "relay-helper")],
  ["stripped-sharer", "epoch=50 message=post-1 sharer=stripped", (s) => replaceOnce(s, "sharerMemberId: sharer.memberId,", "sharerMemberId: \"\",", "stripped-sharer")],
  ["generic-sharer", "epoch=50 message=post-1 sharer=generic-enclave", (s) => replaceOnce(s, "sharerMemberId: sharer.memberId,", "sharerMemberId: \"generic-enclave\",", "generic-sharer")],
  ["retroactive", "epoch=50 message=before-signed-epoch-1", (s) => replaceOnce(s, "if (message.committedSendEpoch < record.sharingStartedAtEpoch)", "if (false && message.committedSendEpoch < record.sharingStartedAtEpoch)", "retroactive")],
  ["hidden-default-leak", "epoch=20 message=first-prejoin-1", (s) => replaceOnce(s, 'mode: "hidden",', 'mode: "shared_from_when_turned_on",', "hidden-default-leak")],
  ["shared-by-default", "epoch=20 message=first-prejoin-1", (s) => replaceOnce(s, 'mode: "hidden",', 'mode: "shared_from_when_turned_on",', "shared-by-default")],
  ["unsigned", "epoch=50 sharer=member-alex", (s) => replaceOnce(s, "if (!verify(instruction))", "if (false && !verify(instruction))", "unsigned")],
  ["cross-enclave", "epoch=50 sharer=member-alex", (s) => replaceOnce(s, "if (instruction.enclaveId !== current.enclaveId)", "if (false && instruction.enclaveId !== current.enclaveId)", "cross-enclave")],
  ["unpermitted", "epoch=50 sharer=member-not-permitted", (s) => replaceOnce(s, "if (!resolvePermission(instruction.actorMemberId, current.enclaveId, \"configure_history_for_new_members\"))", "if (false && !resolvePermission(instruction.actorMemberId, current.enclaveId, \"configure_history_for_new_members\"))", "unpermitted")],
  ["sharing-after-off", "epoch=70 message=after-off-1 sharer=member-alex", (s) => replaceOnce(s, "if (record.remainder === undefined) return \"history-hidden\";", "if (record.remainder === undefined) return \"history-hidden\";", "sharing-after-off")],
  ["missing-sentence", "surface=invite-and-off-control", (s, d) => [replaceAllRequired(s, "Already shared items cannot be recalled.", "History wording removed.", "missing-sentence"), replaceOnce(d, "Enclave ownership cannot be transferred in this release; the last owner must remain.", "", "missing-sentence-disclosure")]],
  ["recall-promise", "surface=off-control", (s) => replaceAllRequired(s, "Already shared items cannot be recalled.", "Already shared items will be recalled.", "recall-promise")],
  ["empty-inventory", "sink=member-device-outbound-reshare-ciphertext", (s) => replaceOnce(s, '  "member-device-outbound-reshare-ciphertext",\n  "relay-ciphertext-courier",\n  "joiner-current-key-inbox",', "", "empty-inventory")],
];

// One case needs two coherent state mutations: retain the enabled mode after
// an off instruction and ignore its stopping epoch.  Keeping this separate
// from the declarative matrix makes the attacked epoch visible in the log.
cases[17][2] = (s) => replaceOnce(
  replaceOnce(
    s,
    "mode: instruction.mode,",
    "mode: enabled ? instruction.mode : current.mode,",
    "sharing-after-off-mode",
  ),
  "if (record.sharingStoppedAtEpoch !== null && message.committedSendEpoch >= record.sharingStoppedAtEpoch)",
  "if (false && record.sharingStoppedAtEpoch !== null && message.committedSendEpoch >= record.sharingStoppedAtEpoch)",
  "sharing-after-off-epoch",
);

function mutate(caseName, edit) {
  const dir = copyUi(caseName);
  try {
    const original = readFileSync(join(dir, history), "utf8");
    const originalDisclosures = readFileSync(join(dir, disclosures), "utf8");
    const changed = edit(original, originalDisclosures);
    const [nextHistory, nextDisclosures] = Array.isArray(changed) ? changed : [changed, originalDisclosures];
    writeFileSync(join(dir, history), nextHistory);
    writeFileSync(join(dir, disclosures), nextDisclosures);
    const result = run6933(dir);
    const output = `${result.stdout}${result.stderr}`;
    assert.notEqual(result.status, 0, `TASK6934 mutation=${caseName} unexpectedly green\n${output}`);
    assert.match(output, /FAIL|Error|AssertionError/u, `TASK6934 mutation=${caseName} did not reach the unchanged 6933 gate\n${output}`);
    return result.status;
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function baseline() {
  const result = run6933(ui);
  const output = `${result.stdout}${result.stderr}`;
  assert.equal(result.status, 0, output);
  const source = `${readFileSync(join(ui, history), "utf8")}\n${readFileSync(join(ui, gate), "utf8")}`;
  for (const witness of requiredSourceWitnesses) assert.ok(source.includes(witness), `TASK6934 absent case=baseline-source witness=${witness}`);
  for (const witness of requiredOutputWitnesses) assert.match(output, new RegExp(witness.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "u"), `TASK6934 absent case=baseline-output witness=${witness}`);
}

function runA12EscrowMutations() {
  const scanner = join(root, "scripts/check-no-future-device-key-escrow.mjs");
  for (const [caseName, field] of [
    ["key-retention", "archived_epoch_message_key"],
    ["debug-cache", "winning_epoch_debug_message_key"],
    ["opaque-blob", "opaque_blob_chain_key"],
    ["operator-backup", "operator_backup_root_key"],
  ]) {
    const dir = mkdtempSync(join(tmpdir(), `osl-task-6934-a12-${caseName}-`));
    try {
      const target = join(dir, "apps/osl-hub-ui/src");
      mkdirSync(target, { recursive: true });
      writeFileSync(join(target, "main.ts"), `const ${field} = \"hostile retained key\";\n`);
      const result = spawnSync(process.execPath, [scanner, dir], { encoding: "utf8" });
      const output = `${result.stdout}${result.stderr}`;
      assert.equal(result.status, 1, `TASK6934 A12 mutation=${caseName} did not make 4806 red\n${output}`);
      assert.match(output, new RegExp(`field ${field}`, "u"), output);
      console.log(`TASK6934 A12 mutation=${caseName} exit=1 ruling=A12 forward_secrecy=with_an_off_switch field=${field}`);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }
  console.log("TASK6934 A12 PASS throwaway_copies=4 discarded=4");
}

function run4884MissingSentenceMutation() {
  const dir = copyUi("missing-sentence-4884");
  try {
    const source = readFileSync(join(dir, disclosures), "utf8");
    writeFileSync(join(dir, disclosures), replaceOnce(
      source,
      "Enclave ownership cannot be transferred in this release; the last owner must remain.",
      "",
      "missing-sentence-4884",
    ));
    const result = spawnSync(process.execPath, [vitest, "run", "src/task-4884-enclave-disclosures.test.ts", "--config", "vitest.config.cjs", "--maxWorkers=1", "--minWorkers=1"], { cwd: dir, encoding: "utf8" });
    const output = `${result.stdout}${result.stderr}`;
    assert.equal(result.status, 1, `TASK6934 missing-sentence did not make 4884 red\n${output}`);
    assert.match(output, /Enclave ownership cannot be transferred/u, output);
    console.log("TASK6934 4884 mutation=missing-sentence exit=1 surface=invite-and-off-control");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function main() {
  assert.equal(cases.length, 21, "TASK6934 mutation matrix changed without updating its proof");
  assert.ok(existsSync(vitest), "TASK6934 vitest runner is absent");
  const requested = process.env.TASK6934_CASES?.split(",").filter(Boolean);
  const selected = requested === undefined ? cases : cases.filter(([caseName]) => requested.includes(caseName));
  if (requested !== undefined) {
    assert.equal(selected.length, requested.length, `TASK6934 absent case=${requested.find((caseName) => !selected.some(([selectedName]) => selectedName === caseName))}`);
  }
  if (process.env.TASK6934_A12 === "1") {
    runA12EscrowMutations();
    return;
  }
  if (process.env.TASK6934_4884 === "1") {
    run4884MissingSentenceMutation();
    return;
  }
  baseline();
  const red = [];
  for (const [caseName, witness, edit] of selected) {
    const status = mutate(caseName, edit);
    red.push(caseName);
    console.log(`TASK6934 RED mutation=${caseName} exit=${status} ${witness}`);
  }
  baseline();
  console.log(`TASK6934 PASS mutations=${red.length}/${cases.length} applicants=3 sealed_corpora=2 epoch_boundary=50 willing_member=member-alex relay_observer=0 sink_inventory=3 restoration=green copies_discarded=${red.length}`);
}

main();
