import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";
import { checksumForAsset, fetchChecked } from "./publish-installer.mjs";

const asset = "osl-hub-0.1.0-x64-nsis.exe";
const bytes = Buffer.from("current NSIS installer");
const digest = createHash("sha256").update(bytes).digest("hex");
const checksums = `${digest}  ${asset}\n`;
const response = (body, status = 200) => ({ ok: status === 200, status, text: async () => body, arrayBuffer: async () => Buffer.from(body) });

test("T11-T10 accepts a mirror only when its SHA-256 matches release SHA256SUMS", async () => {
  const result = await fetchChecked(async (url) => response(url.endsWith("SHA256SUMS.txt") ? checksums : bytes), "https://mirror/installer.exe", "https://release/SHA256SUMS.txt", asset);
  assert.equal(result.sha256, digest);
});

test("T11-T10 sabotage: stale MSI or any other bytes fail the release checksum", async () => {
  await assert.rejects(() => fetchChecked(async (url) => response(url.endsWith("SHA256SUMS.txt") ? checksums : Buffer.from("old MSI")), "https://mirror/old.msi", "https://release/SHA256SUMS.txt", asset), /SHA-256 mismatch/);
  assert.throws(() => checksumForAsset(checksums, "osl-privacy-0.0.1.msi"), /no entry/);
});
