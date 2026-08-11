import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { checkEvidence } from "../scripts/task-6092-proof.mjs";

const roots: string[] = [];

function fixture(): string {
  const root = mkdtempSync(join(tmpdir(), "task-6092-checker-"));
  roots.push(root);
  mkdirSync(join(root, "raw", "objects"), { recursive: true });
  mkdirSync(join(root, "raw", "absence"), { recursive: true });
  mkdirSync(join(root, "canaries"), { recursive: true });
  mkdirSync(join(root, "mutant"), { recursive: true });
  const lanes = [
    ["alice-to-bob", "text", false], ["alice-to-bob", "file", false],
    ["bob-to-alice", "text", false], ["bob-to-alice", "file", false],
    ["alice-to-bob", "text", true], ["alice-to-bob", "file", true],
  ] as const;
  const messages = lanes.map(([direction, kind, offline], index) => ({
    direction, kind, offline, message_id: (index + 1).toString(16).padStart(32, "0"),
  }));
  const envelope = Buffer.concat([Buffer.from("OSL6092\0"), Buffer.alloc(112, 0xa5)]);
  const rows = messages.map((message) => ({
    message_id: message.message_id,
    envelope_bytes: envelope.length,
    envelope_hex: envelope.toString("hex").toUpperCase(),
  }));
  const objects = messages.map((message) => ({ key: `messages/${message.message_id}`, size: envelope.length }));
  for (const message of messages) writeFileSync(join(root, "raw", "objects", `${message.message_id}.bin`), envelope);
  writeFileSync(join(root, "raw", "message-rows.json"), JSON.stringify(rows));
  writeFileSync(join(root, "raw", "leak-rows.json"), "[]");
  writeFileSync(join(root, "raw", "object-inventory.json"), JSON.stringify({ objects, truncated: false }));
  writeFileSync(join(root, "raw", "d1-backup.sql"), "BEGIN; COMMIT;");
  const scanArtifacts = ["raw/message-rows.json", "raw/leak-rows.json", "raw/object-inventory.json", "raw/d1-backup.sql"];
  const surfaces: Array<{ name: string; result: string; artifact: string }> = [
    { name: "primary_database", result: "raw", artifact: "raw/message-rows.json" },
    { name: "object_blobs", result: "raw", artifact: "raw/object-inventory.json" },
    { name: "backups", result: "raw", artifact: "raw/d1-backup.sql" },
  ];
  for (const name of ["queues", "caches", "replicas", "logs", "telemetry"]) {
    const artifact = `raw/absence/${name}.txt`;
    writeFileSync(join(root, artifact), `${name} absent verified`);
    scanArtifacts.push(artifact);
    surfaces.push({ name, result: "absent_verified", artifact });
  }
  scanArtifacts.push(...messages.map((message) => `raw/objects/${message.message_id}.bin`));
  const textFiles = ["one", "two", "three"].map((name) => {
    const path = `canaries/${name}.txt`;
    writeFileSync(join(root, path), `~private-${name}-${"x".repeat(64)}`);
    return path;
  });
  const fileFiles = ["one", "two", "three"].map((name, index) => {
    const path = `canaries/${name}.bin`;
    const body = Buffer.alloc(4096, 0x31 + index);
    body[0] = 0x7e;
    writeFileSync(join(root, path), body);
    writeFileSync(join(root, `${path}.sha256`), name.repeat(64).slice(0, 64));
    return path;
  });
  writeFileSync(join(root, "manifest.json"), JSON.stringify({
    format: "osl.task6092.deployed-ciphertext-proof.v1",
    service: { deployed: true, worker_name: "worker", deployment_id: "version", frozen_before_raw_read: true, frozen_at_ms: 1, first_decryption_at_ms: 2 },
    clients: [
      { client_id: "a", public_key_sha256: "1", key_bytes: 32, installed: true },
      { client_id: "b", public_key_sha256: "2", key_bytes: 32, installed: true },
    ],
    messages, surfaces,
    raw: { message_rows: "raw/message-rows.json", leak_rows: "raw/leak-rows.json", object_inventory: "raw/object-inventory.json", scan_artifacts: scanArtifacts },
    canaries: { text_files: textFiles, file_files: fileFiles },
    scan: { text_canary_matches: 0, file_fingerprint_matches: 0 },
    decryptions: { exact: 6, second_open_refused: 6 },
    tamper: { byte_flips: 1, refused: true, plaintext_output_bytes: 0, exact_surface: "object_blobs.messages/id" },
    mutation: { checker_exit: 1, exact_surface: "primary_database.surface_leaks.leak_bytes", ordinary_readback_preserved: true },
    restored_run: true,
  }));
  writeFileSync(join(root, "mutant", "checker.stderr.txt"), "TASK 6092 FAIL: primary_database.surface_leaks.leak_bytes");
  writeFileSync(join(root, "mutant", "leak-rows.json"), "[{}]");
  return root;
}

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

describe("TASK 6092 fail-closed deployed ciphertext checker", () => {
  it("accepts complete raw surface, exact decryption, once, and tamper evidence", () => {
    expect(checkEvidence(fixture())).toMatchObject({ messages: 6, surfaces: 8, decryptions: 6 });
  });

  it("names the exact production raw surface when one plaintext-canary byte is written", () => {
    const root = fixture();
    writeFileSync(join(root, "raw", "leak-rows.json"), JSON.stringify([{
      surface: "primary_database.surface_leaks.leak_bytes", message_id: "1", leak_hex: "7E",
    }]));
    expect(() => checkEvidence(root)).toThrow("plaintext canary byte found on exact raw surface primary_database.surface_leaks.leak_bytes");
  });

  it.each([
    ["client", (manifest: any) => { manifest.clients.pop(); }],
    ["offline queue", (manifest: any) => { manifest.messages = manifest.messages.filter((message: any) => !message.offline); }],
    ["raw inventory", (manifest: any) => { manifest.surfaces = manifest.surfaces.filter((surface: any) => surface.name !== "queues"); }],
    ["authenticated envelope", (_manifest: any, root: string) => { const path = join(root, "raw", "message-rows.json"); const rows = JSON.parse(readFileSync(path, "utf8")); rows[0].envelope_hex = "00"; writeFileSync(path, JSON.stringify(rows)); }],
    ["recipient decryption", (manifest: any) => { manifest.decryptions.exact = 5; }],
    ["tamper run", (manifest: any) => { manifest.tamper.byte_flips = 0; }],
  ])("fails when %s evidence is starved", (_label, mutate) => {
    const root = fixture();
    const path = join(root, "manifest.json");
    const manifest = JSON.parse(readFileSync(path, "utf8"));
    mutate(manifest, root);
    writeFileSync(path, JSON.stringify(manifest));
    expect(() => checkEvidence(root)).toThrow();
  });
});
