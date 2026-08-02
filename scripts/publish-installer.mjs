#!/usr/bin/env node
/**
 * T11-C2 / T11-T10: mirror a release installer without inventing a checksum.
 *
 * GitHub Releases remains canonical; this only publishes a byte-identical R2
 * mirror after checking the release's SHA256SUMS.txt entry.
 */
import { createHash } from "node:crypto";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";

export function checksumForAsset(checksums, asset) {
  const line = checksums.split(/\r?\n/).find((entry) => entry.endsWith(`  ${asset}`) || entry.endsWith(` *${asset}`));
  if (!line) throw new Error(`SHA256SUMS.txt has no entry for ${asset}`);
  const match = /^([a-fA-F0-9]{64})\s+[* ](.+)$/.exec(line);
  if (!match || match[2] !== asset) throw new Error(`invalid SHA256SUMS.txt entry for ${asset}`);
  return match[1].toLowerCase();
}

export async function fetchChecked(fetchImpl, installerUrl, checksumsUrl, asset) {
  const [installer, checksums] = await Promise.all([fetchImpl(installerUrl), fetchImpl(checksumsUrl)]);
  if (!installer.ok) throw new Error(`installer download failed: HTTP ${installer.status}`);
  if (!checksums.ok) throw new Error(`checksum download failed: HTTP ${checksums.status}`);
  const bytes = Buffer.from(await installer.arrayBuffer());
  const expected = checksumForAsset(await checksums.text(), asset);
  const actual = createHash("sha256").update(bytes).digest("hex");
  if (actual !== expected) throw new Error(`SHA-256 mismatch for ${asset}: expected ${expected}, received ${actual}`);
  return { bytes, sha256: actual };
}

function run(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { stdio: "inherit" });
    child.on("error", reject);
    child.on("exit", (code) => code === 0 ? resolve() : reject(new Error(`${command} exited ${code}`)));
  });
}

function usage() {
  return "Usage: node scripts/publish-installer.mjs --installer-url URL --checksums-url URL --asset NAME [--bucket BUCKET --object KEY] [--check-url URL]";
}

function args(argv) {
  const result = {};
  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i];
    if (flag === "--help") return { help: true };
    if (!flag.startsWith("--")) throw new Error(`unknown argument: ${flag}`);
    const value = argv[++i];
    if (!value) throw new Error(`missing value for ${flag}`);
    result[flag.slice(2).replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = value;
  }
  for (const required of ["installerUrl", "checksumsUrl", "asset"]) if (!result[required]) throw new Error(usage());
  return result;
}

export async function main(options, { fetchImpl = fetch, upload = run } = {}) {
  const source = await fetchChecked(fetchImpl, options.installerUrl, options.checksumsUrl, options.asset);
  if (options.checkUrl) {
    const mirror = await fetchChecked(fetchImpl, options.checkUrl, options.checksumsUrl, options.asset);
    if (!mirror.bytes.equals(source.bytes)) throw new Error("mirror bytes differ from the canonical release");
    console.log(`T11-T10 PASS: ${options.asset} SHA-256 ${source.sha256}`);
    return source;
  }
  if (!options.bucket || !options.object) throw new Error("--bucket and --object are required to publish (or use --check-url)");
  const directory = await mkdtemp(join(tmpdir(), "osl-installer-"));
  const file = join(directory, options.asset);
  try {
    await writeFile(file, source.bytes);
    await upload("wrangler", ["r2", "object", "put", `${options.bucket}/${options.object}`, "--file", file, "--content-type", "application/vnd.microsoft.portable-executable"]);
    console.log(`published mirror ${options.bucket}/${options.object} with SHA-256 ${source.sha256}`);
  } finally { await rm(directory, { recursive: true, force: true }); }
  return source;
}

if (import.meta.main) {
  try { await main(args(process.argv.slice(2))); } catch (error) { console.error(`T11-T10 FAIL: ${error.message}`); process.exitCode = 1; }
}
