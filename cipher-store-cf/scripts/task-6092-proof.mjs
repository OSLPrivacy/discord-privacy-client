#!/usr/bin/env node
import { existsSync, readFileSync, statSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REQUIRED_SURFACES = new Map([
  ["primary_database", "raw"],
  ["object_blobs", "raw"],
  ["queues", "absent_verified"],
  ["caches", "absent_verified"],
  ["replicas", "absent_verified"],
  ["backups", "raw"],
  ["logs", "absent_verified"],
  ["telemetry", "absent_verified"],
]);

export function checkEvidence(rootInput) {
  const root = resolve(rootInput);
  const manifestPath = join(root, "manifest.json");
  if (!existsSync(manifestPath)) throw new Error("missing evidence: manifest.json");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  const fail = (message) => { throw new Error(message); };

  if (manifest.format !== "osl.task6092.deployed-ciphertext-proof.v1") fail("manifest format missing");
  if (manifest.service?.deployed !== true || !manifest.service?.worker_name || !manifest.service?.deployment_id) {
    fail("deployed service evidence missing");
  }
  if (manifest.service?.frozen_before_raw_read !== true
      || !(Number(manifest.service?.frozen_at_ms) < Number(manifest.service?.first_decryption_at_ms))) {
    fail("freeze before recipient decryption evidence missing");
  }
  if (!Array.isArray(manifest.clients) || manifest.clients.length !== 2) fail("two installed clients required");
  const clientIds = new Set(manifest.clients.map((client) => client.client_id));
  const publicKeys = new Set(manifest.clients.map((client) => client.public_key_sha256));
  if (clientIds.size !== 2 || publicKeys.size !== 2
      || manifest.clients.some((client) => client.installed !== true || client.key_bytes < 32)) {
    fail("two independently keyed installed clients required");
  }

  const surfaces = new Map((manifest.surfaces ?? []).map((surface) => [surface.name, surface]));
  for (const [name, result] of REQUIRED_SURFACES) {
    const surface = surfaces.get(name);
    if (!surface) fail(`service surface missing: ${name}`);
    if (surface.result !== result) fail(`service surface ${name} expected ${result}, got ${surface.result ?? "absent"}`);
    const artifact = surface.artifact && join(root, surface.artifact);
    if (!artifact || !existsSync(artifact) || statSync(artifact).size === 0) {
      fail(`service surface ${name} has no independent raw/absence artifact`);
    }
  }

  const leakRowsPath = join(root, manifest.raw?.leak_rows ?? "");
  if (!existsSync(leakRowsPath)) fail("primary_database.surface_leaks raw read missing");
  const leakRows = JSON.parse(readFileSync(leakRowsPath, "utf8"));
  if (!Array.isArray(leakRows)) fail("primary_database.surface_leaks raw read malformed");
  if (leakRows.length !== 0) {
    const exact = leakRows[0]?.surface ?? "primary_database.surface_leaks.leak_bytes";
    fail(`plaintext canary byte found on exact raw surface ${exact}`);
  }

  const messages = manifest.messages ?? [];
  if (messages.length !== 6) fail(`expected 6 online/offline messages, got ${messages.length}`);
  const requiredLanes = new Set([
    "alice-to-bob:text:online", "alice-to-bob:file:online",
    "bob-to-alice:text:online", "bob-to-alice:file:online",
    "alice-to-bob:text:offline", "alice-to-bob:file:offline",
  ]);
  const observedLanes = new Set(messages.map((message) =>
    `${message.direction}:${message.kind}:${message.offline ? "offline" : "online"}`));
  for (const lane of requiredLanes) if (!observedLanes.has(lane)) fail(`message lane missing: ${lane}`);
  if (new Set(messages.map((message) => message.message_id)).size !== 6) fail("message ids are not distinct");

  const dbRowsPath = join(root, manifest.raw?.message_rows ?? "");
  const inventoryPath = join(root, manifest.raw?.object_inventory ?? "");
  if (!existsSync(dbRowsPath) || !existsSync(inventoryPath)) fail("raw database/object inventory missing");
  const dbRows = JSON.parse(readFileSync(dbRowsPath, "utf8"));
  const inventory = JSON.parse(readFileSync(inventoryPath, "utf8"));
  if (!Array.isArray(dbRows) || !Array.isArray(inventory.objects) || inventory.truncated !== false) {
    fail("raw service inventory is incomplete");
  }
  for (const message of messages) {
    const row = dbRows.find((candidate) => candidate.message_id === message.message_id);
    if (!row) fail(`primary_database row missing exact message id ${message.message_id}`);
    if (row.envelope_bytes < 120 || !row.envelope_hex?.startsWith("4F534C3630393200")) {
      fail(`authenticated encrypted envelope missing for exact message id ${message.message_id}`);
    }
    const key = `messages/${message.message_id}`;
    if (!inventory.objects.some((object) => object.key === key && object.size === row.envelope_bytes)) {
      fail(`object_blobs record missing exact message id ${message.message_id}`);
    }
    const objectPath = join(root, "raw", "objects", `${message.message_id}.bin`);
    if (!existsSync(objectPath)) fail(`object_blobs raw bytes missing exact message id ${message.message_id}`);
    const object = readFileSync(objectPath);
    if (object.length !== row.envelope_bytes || object.subarray(0, 8).toString("hex") !== "4f534c3630393200") {
      fail(`object_blobs unauthenticated/empty envelope for exact message id ${message.message_id}`);
    }
  }

  const rawArtifacts = (manifest.raw?.scan_artifacts ?? []).map((path) => readFileSync(join(root, path)));
  if (rawArtifacts.length < 8) fail("raw service scan artifacts starved");
  const textFiles = manifest.canaries?.text_files ?? [];
  const fileFiles = manifest.canaries?.file_files ?? [];
  if (textFiles.length !== 3 || fileFiles.length !== 3) fail("private text/file canary inventory starved");
  const distinctCanaries = new Set();
  for (const textPath of textFiles) {
    const canary = readFileSync(join(root, textPath));
    if (canary.length < 64 || canary[0] !== 0x7e) fail(`private text canary malformed: ${basename(textPath)}`);
    distinctCanaries.add(canary.toString("hex"));
    for (const raw of rawArtifacts) if (raw.includes(canary)) fail(`text canary found in raw service surface: ${basename(textPath)}`);
  }
  for (const filePath of fileFiles) {
    const canary = readFileSync(join(root, filePath));
    const fingerprint = readFileSync(join(root, `${filePath}.sha256`));
    if (canary.length < 4096 || canary[0] !== 0x7e || fingerprint.length !== 64) fail(`private file canary malformed: ${basename(filePath)}`);
    distinctCanaries.add(canary.toString("hex"));
    for (const raw of rawArtifacts) {
      if (raw.includes(canary) || raw.includes(fingerprint)) fail(`file canary/fingerprint found in raw service surface: ${basename(filePath)}`);
    }
  }
  if (distinctCanaries.size !== 6) fail("cryptographic canaries are not distinct");
  if (manifest.scan?.text_canary_matches !== 0 || manifest.scan?.file_fingerprint_matches !== 0) {
    fail("raw service scan did not report zero canary/fingerprint matches");
  }

  if (manifest.decryptions?.exact !== 6 || manifest.decryptions?.second_open_refused !== 6) {
    fail("both recipients must decrypt all 6 byte-exact canaries once");
  }
  if (manifest.tamper?.byte_flips !== 1 || manifest.tamper?.refused !== true
      || manifest.tamper?.plaintext_output_bytes !== 0 || !manifest.tamper?.exact_surface) {
    fail("one-byte ciphertext tamper refusal evidence missing");
  }
  const mutantStderr = join(root, "mutant", "checker.stderr.txt");
  const mutantLeaks = join(root, "mutant", "leak-rows.json");
  if (manifest.mutation?.checker_exit !== 1
      || manifest.mutation?.exact_surface !== "primary_database.surface_leaks.leak_bytes"
      || manifest.mutation?.ordinary_readback_preserved !== true
      || !existsSync(mutantStderr) || !readFileSync(mutantStderr, "utf8").includes(manifest.mutation.exact_surface)
      || !existsSync(mutantLeaks) || JSON.parse(readFileSync(mutantLeaks, "utf8")).length !== 1) {
    fail("production plaintext mutation/red proof missing");
  }
  if (manifest.restored_run !== true) fail("restored deployed-service run missing");
  return {
    messages: 6,
    surfaces: REQUIRED_SURFACES.size,
    textCanaryMatches: 0,
    fileFingerprintMatches: 0,
    decryptions: 6,
    secondOpenRefused: 6,
    tamperByteFlips: 1,
  };
}

const invoked = process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invoked) {
  const root = process.argv[2];
  if (!root) {
    process.stderr.write("TASK 6092 FAIL: evidence directory required\n");
    process.exit(1);
  }
  try {
    const result = checkEvidence(root);
    process.stdout.write(
      `TASK 6092 PASS: messages=${result.messages} surfaces=${result.surfaces} text_canary_matches=0 file_fingerprint_matches=0 decryptions=${result.decryptions} second_open_refused=${result.secondOpenRefused} tamper_byte_flips=1\n`,
    );
  } catch (error) {
    process.stderr.write(`TASK 6092 FAIL: ${error.message}\n`);
    process.exit(1);
  }
}
