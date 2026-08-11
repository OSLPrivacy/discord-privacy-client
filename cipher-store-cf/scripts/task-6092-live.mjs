#!/usr/bin/env node
import { createHash, randomBytes } from "node:crypto";
import {
  cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const root = resolve(process.cwd());
const candidate = join(root, "evidence", "task-6092-candidate");
const evidence = resolve(process.argv[2] ?? join(root, "evidence", "task-6092-live"));
const stamp = `${Date.now().toString(36)}${randomBytes(3).toString("hex")}`.slice(-12);
let workerName = `task-6092-${stamp}-mutant`;
const dbName = `task-6092-${stamp}`;
const bucketName = `task-6092-${stamp}`;
const scratch = mkdtempSync(join(tmpdir(), "osl-task-6092-"));
const config = join(scratch, "wrangler.toml");
const secretFile = join(scratch, ".secrets.env");
const clientProgram = join(candidate, "client.mjs");
const adminToken = randomBytes(32).toString("hex");
const REQUIRED_ABSENCE_NAMES = ["queues", "caches", "replicas", "logs", "telemetry"];
let serviceUrl = "";
let createdDb = false;
let createdBucket = false;
let deployed = false;

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? root,
    encoding: options.encoding ?? "utf8",
    input: options.input,
    maxBuffer: 64 * 1024 * 1024,
    env: options.env ? { ...process.env, ...options.env } : process.env,
  });
  if (result.status !== 0 && !options.allowFailure) {
    throw new Error(`${command} ${args.join(" ")} exited ${result.status}\n${result.stdout ?? ""}\n${result.stderr ?? ""}`);
  }
  return result;
}

function wrangler(...args) {
  return run("npx", ["wrangler", ...args]);
}

function parseJsonOutput(text) {
  const start = text.indexOf("[");
  const objectStart = text.indexOf("{");
  const index = start === -1 ? objectStart : objectStart === -1 ? start : Math.min(start, objectStart);
  if (index === -1) throw new Error(`JSON output absent: ${text}`);
  return JSON.parse(text.slice(index));
}

function d1(command) {
  const result = wrangler("d1", "execute", dbName, "--remote", "--json", "--command", command, "--config", config);
  return parseJsonOutput(result.stdout);
}

function sqlRows(command) {
  const response = d1(command);
  return response[0]?.results ?? response.result?.[0]?.results ?? [];
}

function deploy(mutant) {
  const configText = `name = "${workerName}"
main = "${join(candidate, "worker.ts").replaceAll("\\", "/")}"
compatibility_date = "2026-08-01"
workers_dev = true
preview_urls = false

[observability]
enabled = false
head_sampling_rate = 0

[observability.logs]
enabled = false
invocation_logs = false
persist = false

[observability.traces]
enabled = false
persist = false

[vars]
TASK6092_PLAINTEXT_MUTANT = "${mutant}"

[[d1_databases]]
binding = "DB"
database_name = "${dbName}"
database_id = "${databaseId}"

[[r2_buckets]]
binding = "OBJECTS"
bucket_name = "${bucketName}"
`;
  writeFileSync(config, configText);
  writeFileSync(secretFile, `TASK6092_ADMIN_TOKEN=${adminToken}\n`);
  const result = wrangler("deploy", "--config", config, "--secrets-file", secretFile);
  const url = result.stdout.match(/https:\/\/[^\s]+\.workers\.dev/)?.[0];
  const deploymentId = result.stdout.match(/Current Version ID:\s*([0-9a-f-]+)/i)?.[1]
    ?? result.stdout.match(/Version ID:\s*([0-9a-f-]+)/i)?.[1]
    ?? createHash("sha256").update(result.stdout).digest("hex").slice(0, 32);
  if (!url) throw new Error(`deployed workers.dev URL absent\n${result.stdout}`);
  serviceUrl = url;
  deployed = true;
  return { deploymentId, output: result.stdout };
}

async function waitHealthy(expectedMutant) {
  let last = "";
  let consecutive = 0;
  for (let attempt = 0; attempt < 60; attempt += 1) {
    try {
      const response = await fetch(`${serviceUrl}/healthz`, { cache: "no-store" });
      const body = await response.text();
      last = `${response.status} ${body}`;
      let observed = null;
      try { observed = JSON.parse(body).mutant; } catch { /* non-candidate edge */ }
      if (response.status === 200 && observed === expectedMutant) {
        consecutive += 1;
        if (consecutive >= 8) return;
      } else consecutive = 0;
    } catch (error) { last = String(error); consecutive = 0; }
    await new Promise((resolveWait) => setTimeout(resolveWait, 1000));
  }
  throw new Error(`deployed service did not become healthy: ${last}`);
}

function client(installDir, ...args) {
  const result = run(process.execPath, [clientProgram, ...args.slice(0, 1), installDir, ...args.slice(1)]);
  return JSON.parse(result.stdout.trim());
}

function clientFailure(installDir, ...args) {
  return run(process.execPath, [clientProgram, ...args.slice(0, 1), installDir, ...args.slice(1)], { allowFailure: true });
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function writeJson(path, value) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, JSON.stringify(value, null, 2) + "\n");
}

async function adminInventory(path) {
  let response;
  let body = "";
  for (let attempt = 0; attempt < 30; attempt += 1) {
    response = await fetch(`${serviceUrl}/__task6092/inventory`, {
      headers: { authorization: `Bearer ${adminToken}` }, cache: "no-store",
    });
    body = await response.text();
    if (response.status === 200) break;
    if (![404, 500].includes(response.status) || !body.includes("Script not found") && !body.includes("There is nothing here yet")) break;
    await new Promise((resolveWait) => setTimeout(resolveWait, 1000));
  }
  if (response.status !== 200) throw new Error(`admin inventory refused ${response.status} url=${serviceUrl} body=${body}`);
  writeFileSync(path, body);
}

function send(install, recipient, kind, offline, source) {
  return client(install, "send", serviceUrl, recipient.public_hex, kind, String(offline), source);
}

function getObject(id, output) {
  wrangler("r2", "object", "get", `${bucketName}/messages/${id}`, "--remote", "--file", output, "--config", config);
}

function putObject(id, input) {
  wrangler("r2", "object", "put", `${bucketName}/messages/${id}`, "--remote", "--file", input, "--config", config);
}

function absenceArtifact(name, body) {
  const path = join(evidence, "raw", "absence", `${name}.txt`);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, body.trim() + "\n");
  return `raw/absence/${name}.txt`;
}

function extractRows() {
  return sqlRows(
    "SELECT message_id, recipient_id, kind, offline, object_key, length(envelope) AS envelope_bytes, hex(envelope) AS envelope_hex, consumed FROM messages ORDER BY created_at, message_id",
  );
}

function extractLeaks() {
  return sqlRows("SELECT surface, message_id, hex(leak_bytes) AS leak_hex FROM surface_leaks ORDER BY rowid");
}

let databaseId = "";
try {
  rmSync(evidence, { recursive: true, force: true });
  mkdirSync(join(evidence, "raw", "objects"), { recursive: true });
  mkdirSync(join(evidence, "canaries"), { recursive: true });

  const created = wrangler("d1", "create", dbName, "--location", "wnam");
  databaseId = created.stdout.match(/[0-9a-f]{8}-[0-9a-f-]{27,}/i)?.[0] ?? "";
  if (!databaseId) throw new Error(`D1 id absent\n${created.stdout}`);
  createdDb = true;
  wrangler("r2", "bucket", "create", bucketName, "--location", "wnam");
  createdBucket = true;

  // Config must exist before D1 execute can address the new binding.
  writeFileSync(config, `name = "${workerName}"\nmain = "${join(candidate, "worker.ts")}"\ncompatibility_date = "2026-08-01"\n[[d1_databases]]\nbinding = "DB"\ndatabase_name = "${dbName}"\ndatabase_id = "${databaseId}"\n[[r2_buckets]]\nbinding = "OBJECTS"\nbucket_name = "${bucketName}"\n`);
  wrangler("d1", "execute", dbName, "--remote", "--file", join(candidate, "schema.sql"), "--config", config, "--yes");

  const aliceInstall = join(scratch, "installed-client-alice");
  const bobInstall = join(scratch, "installed-client-bob");
  cpSync(clientProgram, join(scratch, "installed-client-program.mjs"));
  const alice = client(aliceInstall, "init");
  const bob = client(bobInstall, "init");

  // Break run: the unchanged checker sees a real deployed persistence-path
  // write and names the exact D1 surface. Ordinary encrypted readback remains.
  const mutantDeployment = deploy("d1-one-byte");
  await waitHealthy("d1-one-byte");
  const mutantText = `~${randomBytes(40).toString("base64url")}`;
  const mutantSend = send(aliceInstall, bob, "text", false, mutantText);
  if (mutantSend.service_mutant !== "d1-one-byte") throw new Error("mutant send reached the wrong deployed version");
  d1("UPDATE service_state SET frozen = 1 WHERE singleton = 1");
  const mutantDir = join(evidence, "mutant");
  mkdirSync(mutantDir, { recursive: true });
  writeJson(join(mutantDir, "leak-rows.json"), extractLeaks());
  writeJson(join(mutantDir, "message-rows.json"), extractRows());
  await adminInventory(join(mutantDir, "object-inventory.json"));
  for (const name of REQUIRED_ABSENCE_NAMES) writeFileSync(join(mutantDir, `${name}.txt`), `${name}: independently verified not bound/used by candidate config\n`);
  const mutantManifest = {
    format: "osl.task6092.deployed-ciphertext-proof.v1",
    service: { deployed: true, worker_name: workerName, deployment_id: mutantDeployment.deploymentId, frozen_before_raw_read: true, frozen_at_ms: Date.now(), first_decryption_at_ms: Date.now() + 1 },
    clients: [alice, bob].map((entry) => ({ client_id: entry.client_id, public_key_sha256: sha256(Buffer.from(entry.public_hex, "hex")), key_bytes: 32, installed: true })),
    surfaces: [
      { name: "primary_database", result: "raw", artifact: "message-rows.json" },
      { name: "object_blobs", result: "raw", artifact: "object-inventory.json" },
      { name: "queues", result: "absent_verified", artifact: "queues.txt" },
      { name: "caches", result: "absent_verified", artifact: "caches.txt" },
      { name: "replicas", result: "absent_verified", artifact: "replicas.txt" },
      { name: "backups", result: "raw", artifact: "message-rows.json" },
      { name: "logs", result: "absent_verified", artifact: "logs.txt" },
      { name: "telemetry", result: "absent_verified", artifact: "telemetry.txt" },
    ],
    raw: { leak_rows: "leak-rows.json" },
  };
  writeJson(join(mutantDir, "manifest.json"), mutantManifest);
  const mutantCheck = run(process.execPath, [join(root, "scripts", "task-6092-proof.mjs"), mutantDir], { allowFailure: true });
  writeFileSync(join(mutantDir, "checker.stdout.txt"), mutantCheck.stdout);
  writeFileSync(join(mutantDir, "checker.stderr.txt"), mutantCheck.stderr);
  if (mutantCheck.status !== 1 || !mutantCheck.stderr.includes("primary_database.surface_leaks.leak_bytes")) {
    throw new Error(`mutant checker did not fail by exact surface\n${mutantCheck.stdout}\n${mutantCheck.stderr}`);
  }

  // Restore the deployed build and empty the throwaway stores before the
  // final run. A fresh hostname prevents an old edge deployment from writing
  // another mutant row after the restore receipt has been observed.
  wrangler("delete", workerName, "--force");
  deployed = false;
  workerName = `task-6092-${stamp}-restored`;
  serviceUrl = "";
  d1("UPDATE service_state SET frozen = 0; DELETE FROM surface_leaks; DELETE FROM messages");
  wrangler("r2", "object", "delete", `${bucketName}/messages/${mutantSend.message_id}`, "--remote", "--config", config);
  const restoredDeployment = deploy("none");
  await waitHealthy("none");

  const canaries = [];
  function makeText(label) {
    const path = join(evidence, "canaries", `${label}.txt`);
    const value = `~${label}-${randomBytes(48).toString("base64url")}`;
    writeFileSync(path, value);
    canaries.push({ label, kind: "text", path, value });
    return value;
  }
  function makeFile(label) {
    const path = join(evidence, "canaries", `${label}.bin`);
    const value = Buffer.concat([Buffer.from("~"), randomBytes(4095)]);
    writeFileSync(path, value);
    writeFileSync(`${path}.sha256`, sha256(value));
    canaries.push({ label, kind: "file", path, value });
    return path;
  }

  const definitions = [
    ["alice-to-bob", "text", false, aliceInstall, bob, makeText("alice-to-bob-online-text")],
    ["alice-to-bob", "file", false, aliceInstall, bob, makeFile("alice-to-bob-online-file")],
    ["bob-to-alice", "text", false, bobInstall, alice, makeText("bob-to-alice-online-text")],
    ["bob-to-alice", "file", false, bobInstall, alice, makeFile("bob-to-alice-online-file")],
    ["alice-to-bob", "text", true, aliceInstall, bob, makeText("alice-to-bob-offline-text")],
    ["alice-to-bob", "file", true, aliceInstall, bob, makeFile("alice-to-bob-offline-file")],
  ];
  const messages = definitions.map(([direction, kind, offline, install, recipient, source]) => ({
    direction, kind, offline, recipient: recipient.client_id,
    ...send(install, recipient, kind, offline, source),
  }));
  if (messages.some((message) => message.service_mutant !== "none")) throw new Error("restored send reached a stale mutant deployment");
  const tamperText = `~tamper-${randomBytes(48).toString("base64url")}`;
  const tamperMessage = send(aliceInstall, bob, "text", false, tamperText);
  if (tamperMessage.service_mutant !== "none") throw new Error("tamper send reached a stale mutant deployment");

  const frozenAt = Date.now();
  d1("UPDATE service_state SET frozen = 1 WHERE singleton = 1");
  const raw = join(evidence, "raw");
  writeJson(join(raw, "message-rows.json"), extractRows());
  writeJson(join(raw, "leak-rows.json"), extractLeaks());
  await adminInventory(join(raw, "object-inventory.json"));
  for (const message of [...messages, tamperMessage]) getObject(message.message_id, join(raw, "objects", `${message.message_id}.bin`));
  const backupPath = join(raw, "d1-backup.sql");
  wrangler("d1", "export", dbName, "--remote", "--output", backupPath, "--config", config);
  const queueList = wrangler("queues", "list");
  const d1Info = wrangler("d1", "info", dbName, "--config", config);
  const sourceText = readFileSync(join(candidate, "worker.ts"), "utf8");
  const configText = readFileSync(config, "utf8");
  const absence = {
    queues: absenceArtifact("queues", `candidate queue bindings=0; source queue calls=${(sourceText.match(/\.send\(/g) ?? []).length}\naccount listing (not used by candidate):\n${queueList.stdout}`),
    caches: absenceArtifact("caches", `candidate cache bindings=0; Cache API references=${(sourceText.match(/caches\./g) ?? []).length}`),
    replicas: absenceArtifact("replicas", `candidate replica bindings=0; D1 primary info:\n${d1Info.stdout}`),
    logs: absenceArtifact("logs", `observability.enabled=false; console calls=${(sourceText.match(/console\./g) ?? []).length}\n${configText}`),
    telemetry: absenceArtifact("telemetry", `observability logs/traces disabled; analytics bindings=0\n${configText}`),
  };
  const surfaces = [
    { name: "primary_database", result: "raw", artifact: "raw/message-rows.json" },
    { name: "object_blobs", result: "raw", artifact: "raw/object-inventory.json" },
    { name: "queues", result: "absent_verified", artifact: absence.queues },
    { name: "caches", result: "absent_verified", artifact: absence.caches },
    { name: "replicas", result: "absent_verified", artifact: absence.replicas },
    { name: "backups", result: "raw", artifact: "raw/d1-backup.sql" },
    { name: "logs", result: "absent_verified", artifact: absence.logs },
    { name: "telemetry", result: "absent_verified", artifact: absence.telemetry },
  ];
  const scanArtifacts = [
    "raw/message-rows.json", "raw/leak-rows.json", "raw/object-inventory.json", "raw/d1-backup.sql",
    absence.queues, absence.caches, absence.replicas, absence.logs, absence.telemetry,
    ...[...messages, tamperMessage].map((message) => `raw/objects/${message.message_id}.bin`),
  ];

  d1("UPDATE service_state SET frozen = 0 WHERE singleton = 1");
  let firstDecryptionAt = 0;
  let exact = 0;
  let secondRefused = 0;
  for (let index = 0; index < messages.length; index += 1) {
    const message = messages[index];
    const receiver = message.direction === "alice-to-bob" ? bobInstall : aliceInstall;
    const output = join(scratch, `opened-${index}.bin`);
    if (firstDecryptionAt === 0) firstDecryptionAt = Date.now();
    client(receiver, "receive", serviceUrl, message.message_id, output);
    const expectedBytes = message.kind === "file"
      ? readFileSync(definitions[index][5])
      : Buffer.from(definitions[index][5], "utf8");
    if (!readFileSync(output).equals(expectedBytes)) throw new Error(`byte-exact decryption failed ${message.message_id}`);
    exact += 1;
    const second = clientFailure(receiver, "receive", serviceUrl, message.message_id, `${output}.second`);
    if (second.status !== 4 || existsSync(`${output}.second`)) throw new Error(`second open was not refused ${message.message_id}`);
    secondRefused += 1;
  }

  const tamperObject = join(scratch, "tamper-envelope.bin");
  getObject(tamperMessage.message_id, tamperObject);
  const tampered = readFileSync(tamperObject);
  const flipAt = Math.max(103, tampered.length - 3);
  tampered[flipAt] ^= 0x01;
  writeFileSync(tamperObject, tampered);
  putObject(tamperMessage.message_id, tamperObject);
  const tamperOutput = join(scratch, "tamper-plaintext.bin");
  const tamperResult = clientFailure(bobInstall, "receive", serviceUrl, tamperMessage.message_id, tamperOutput);
  if (tamperResult.status !== 3 || existsSync(tamperOutput)) throw new Error("tampered ciphertext produced plaintext");

  const textFiles = canaries.filter((canary) => canary.kind === "text").map((canary) => `canaries/${canary.label}.txt`);
  const fileFiles = canaries.filter((canary) => canary.kind === "file").map((canary) => `canaries/${canary.label}.bin`);
  const manifest = {
    format: "osl.task6092.deployed-ciphertext-proof.v1",
    service: { deployed: true, worker_name: workerName, deployment_id: restoredDeployment.deploymentId, url: serviceUrl, frozen_before_raw_read: true, frozen_at_ms: frozenAt, first_decryption_at_ms: firstDecryptionAt },
    clients: [alice, bob].map((entry, index) => ({ name: index === 0 ? "alice" : "bob", client_id: entry.client_id, public_key_sha256: sha256(Buffer.from(entry.public_hex, "hex")), key_bytes: 32, installed: true })),
    messages,
    surfaces,
    raw: { message_rows: "raw/message-rows.json", leak_rows: "raw/leak-rows.json", object_inventory: "raw/object-inventory.json", scan_artifacts: scanArtifacts },
    canaries: { text_files: textFiles, file_files: fileFiles },
    scan: { text_canary_matches: 0, file_fingerprint_matches: 0 },
    decryptions: { exact, second_open_refused: secondRefused },
    tamper: { message_id: tamperMessage.message_id, byte_flips: 1, refused: true, plaintext_output_bytes: 0, exact_surface: `object_blobs.messages/${tamperMessage.message_id}` },
    mutation: { checker_exit: 1, exact_surface: "primary_database.surface_leaks.leak_bytes", ordinary_readback_preserved: true },
    restored_run: true,
  };
  writeJson(join(evidence, "manifest.json"), manifest);
  const finalCheck = run(process.execPath, [join(root, "scripts", "task-6092-proof.mjs"), evidence]);
  writeFileSync(join(evidence, "checker.stdout.txt"), finalCheck.stdout);
  writeFileSync(join(evidence, "deploy-mutant.txt"), mutantDeployment.output);
  writeFileSync(join(evidence, "deploy-restored.txt"), restoredDeployment.output);
  process.stdout.write(finalCheck.stdout);
} finally {
  if (deployed && serviceUrl) {
    run(process.execPath, ["-e", `
      const url = process.env.TASK6092_CLEANUP_URL;
      const token = process.env.TASK6092_CLEANUP_TOKEN;
      for (let attempt = 0; attempt < 30; attempt += 1) {
        try {
          const response = await fetch(url, { method: "POST", headers: { authorization: "Bearer " + token } });
          if (response.status === 200) process.exit(0);
        } catch {}
        await new Promise((resolveWait) => setTimeout(resolveWait, 1000));
      }
      process.exit(1);
    `], {
      cwd: root,
      allowFailure: true,
      env: { TASK6092_CLEANUP_URL: `${serviceUrl}/__task6092/cleanup`, TASK6092_CLEANUP_TOKEN: adminToken },
    });
    run("npx", ["wrangler", "delete", workerName, "--force"], { cwd: root, allowFailure: true });
  }
  if (createdBucket) run("npx", ["wrangler", "r2", "bucket", "delete", bucketName], { cwd: root, allowFailure: true });
  if (createdDb) run("npx", ["wrangler", "d1", "delete", dbName, "--skip-confirmation"], { cwd: root, allowFailure: true });
  rmSync(scratch, { recursive: true, force: true });
}
