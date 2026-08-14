#!/usr/bin/env node
// TASK 6804 acceptance runner. It deliberately writes its own kit encoder so
// the loader is tested against bytes it did not produce.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const uiRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(uiRoot, "..", "..");
const hubRoot = join(repoRoot, "apps", "osl-hub");
const fixtureRoot = mkdtempSync(join(tmpdir(), "osl-6804-"));
const receipt = join(fixtureRoot, "receipt.json");
const cargoTarget = process.env.CARGO_TARGET_DIR ?? "/mnt/d/osl-lane-targets/c";

function fail(message) { throw new Error(`TASK6804 ${message}`); }
function sha256(bytes) { return createHash("sha256").update(bytes).digest("hex"); }

function encodeKit({ userId, identityPhrase, passwordPhrase, version = 1 }) {
  const body = `OSL-RECOVERY-KIT\nversion: ${version}\nuser-id: ${userId}\nidentity-phrase: ${identityPhrase}\npassword-phrase: ${passwordPhrase}\n`;
  return Buffer.from(`${body}digest: ${sha256(Buffer.from(body))}\n`, "utf8");
}

function runPhase(name, extraEnv = {}) {
  const result = spawnSync(
    "cargo",
    ["test", "--no-default-features", "--features", "core", "--test", "task_6804_recovery_kit_upload", name, "--", "--test-threads=1"],
    {
      cwd: hubRoot,
      encoding: "utf8",
      env: { ...process.env, CARGO_TARGET_DIR: cargoTarget, OSL6804_FIXTURES: fixtureRoot, OSL6804_RECEIPT: receipt, ...extraEnv },
      timeout: 300_000,
    },
  );
  if (result.status !== 0) fail(`${name} did not complete`);
}

function writeIndependentFixtures(material) {
  const source = encodeKit(material);
  writeFileSync(join(fixtureRoot, "kit-source.oslkit"), source);
  const corrupt = Buffer.from(source);
  corrupt[corrupt.length - 2] ^= 1;
  writeFileSync(join(fixtureRoot, "kit-corrupt.oslkit"), corrupt);
  writeFileSync(join(fixtureRoot, "kit-version2.oslkit"), encodeKit({ ...material, version: 2 }));
  writeFileSync(join(fixtureRoot, "kit-foreign.oslkit"), encodeKit({
    // A real kit-shaped file with another identity under the source account
    // id: it passes format/version/digest and only identity binding can refuse
    // it on the clean Restore Account profile.
    userId: material.userId,
    identityPhrase: material.foreignIdentityPhrase,
    passwordPhrase: material.foreignPasswordPhrase,
  }));
  writeFileSync(join(fixtureRoot, "not-a-kit.png"), Buffer.from([0x89, 0x50, 0x4e, 0x47]));
  return sha256(source);
}

function outcome(receiptValue, page, tag) {
  const found = receiptValue.outcomes.find((item) => item.page === page && item.tag === tag);
  assert.ok(found, `${page}/${tag} outcome is present`);
  return found;
}

function assertReceipt(receiptValue, material, sourceHash) {
  assert.deepEqual(receiptValue.pages, ["forgot-password", "restore-account"]);
  assert.equal(receiptValue.sourceUserId, receiptValue.restoredUserId);
  assert.equal(receiptValue.passwordResetVerified, true);
  assert.equal(receiptValue.restoreBytesUnchanged, true);
  assert.equal(receiptValue.forgotBytesUnchanged, true);
  for (const page of receiptValue.pages) {
    const cancelled = outcome(receiptValue, page, "cancel");
    assert.equal(cancelled.wordCount, 0);
    assert.equal(cancelled.refusal, null);
    assert.equal(cancelled.path, "");
    for (const [tag, refusal] of [["corrupt-byte", "damaged"], ["wrong-version", "unsupported-version"], ["wrong-identity", "wrong-identity"], ["non-kit-file", "not-a-kit-file"]]) {
      const rejected = outcome(receiptValue, page, tag);
      assert.equal(rejected.wordCount, 0);
      assert.equal(rejected.refusal, refusal);
      assert.match(rejected.refusalMessage, /Nothing was changed\.$/u);
    }
    const valid = outcome(receiptValue, page, "valid-kit");
    assert.equal(valid.wordCount, 12);
    assert.equal(valid.selectedSha256, sourceHash);
    assert.equal(valid.validatedSha256, sourceHash);
    assert.equal(valid.selectedSha256, valid.validatedSha256);
    assert.deepEqual(valid.words, (page === "forgot-password" ? material.passwordPhrase : material.identityPhrase).split(" "));
  }
  const prohibited = [material.identityPhrase, material.passwordPhrase, ...material.identityPhrase.split(" "), ...material.passwordPhrase.split(" ")];
  for (const rendering of receiptValue.debugRenderings) {
    for (const secret of prohibited) assert.equal(rendering.includes(secret), false, "no recovery word in debug rendering");
  }
}

function sourceWiringCheck() {
  const picker = readFileSync(join(hubRoot, "src", "recovery_kit_picker.rs"), "utf8");
  const native = readFileSync(join(hubRoot, "src", "main.rs"), "utf8");
  const validator = readFileSync(join(hubRoot, "src", "recovery_kit_file.rs"), "utf8");
  const ui = readFileSync(join(uiRoot, "src", "main.ts"), "utf8");
  const forgot = readFileSync(join(uiRoot, "src", "account-recovery.ts"), "utf8");
  assert.match(picker, /blocking_pick_file\(\)/u);
  assert.match(picker, /add_filter\("OSL recovery kit", &\[RECOVERY_KIT_EXTENSION\]\)/u);
  assert.match(native, /async fn load_hub_recovery_kit_file\([\s\S]*?page: String/u);
  assert.match(native, /load_recovery_kit_through_picker\(page, &picker, expected_account_id\.as_deref\(\)\)/u);
  assert.equal(/load_hub_recovery_kit_file[\s\S]{0,300}path:/u.test(native), false, "command accepts no renderer path");
  assert.match(validator, /let bytes = std::fs::read\(path\)/u);
  assert.equal((validator.match(/std::fs::read\(/gu) ?? []).length, 1, "only the frozen selection reads kit bytes");
  for (const source of [ui, forgot]) {
    assert.match(source, /Upload recovery kit/u);
    assert.match(source, /data-recovery-kit-word=/u);
    assert.match(source, /Array\.from\(\{ length: 12 \}/u);
  }
  const uploadHandler = ui.slice(ui.indexOf("function bindRecoveryKitUploads"), ui.indexOf("function bindOnboardingPasswordRole"));
  assert.match(uploadHandler, /loadHubRecoveryKitFile\(page\)/u);
  assert.equal(/importHubOslIdentityPhrase|setHubMainPasswordAfterRecovery/u.test(uploadHandler), false, "loading only fills the ordinary forms");
  assert.match(ui, /bindImportForm\(\)/u);
  assert.match(ui, /runAccountRecoveryPhrase/u);
}

try {
  sourceWiringCheck();
  runPhase("phase1_export_source_material");
  const material = JSON.parse(readFileSync(join(fixtureRoot, "source-material.json"), "utf8"));
  const sourceHash = writeIndependentFixtures(material);
  runPhase("phase2_journeys_and_refusals", process.env.OSL6804_STARVE ? { OSL6804_STARVE: process.env.OSL6804_STARVE } : {});
  assertReceipt(JSON.parse(readFileSync(receipt, "utf8")), material, sourceHash);
  console.log("TASK6804_CHECK=PASS pages=2 boxes=12 refusals=4 frozen_hashes=2");
} finally {
  rmSync(fixtureRoot, { recursive: true, force: true });
}
