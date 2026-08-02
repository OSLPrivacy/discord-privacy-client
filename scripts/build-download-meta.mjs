#!/usr/bin/env node
/** T11-C3 / T11-T11: derive the download-page facts from release evidence. */
import { readFile, writeFile } from "node:fs/promises";

export function checksumForAsset(checksums, asset) {
  const line = checksums.split(/\r?\n/).find((entry) => entry.endsWith(`  ${asset}`) || entry.endsWith(` *${asset}`));
  const match = line && /^([a-fA-F0-9]{64})\s+[* ](.+)$/.exec(line);
  if (!match || match[2] !== asset) throw new Error(`SHA256SUMS.txt has no valid entry for ${asset}`);
  return match[1].toLowerCase();
}

export function downloadMeta(latest, checksums, asset) {
  if (!latest || typeof latest.version !== "string" || !/^\d+\.\d+\.\d+/.test(latest.version)) throw new Error("latest.json version must be semver");
  const published = latest.pub_date ?? latest.notes?.match(/\b\d{4}-\d{2}-\d{2}\b/)?.[0];
  if (typeof published !== "string" || !/^\d{4}-\d{2}-\d{2}/.test(published)) throw new Error("latest.json must provide an ISO publication date");
  return { asset, version: latest.version, published: published.slice(0, 10), sha256: checksumForAsset(checksums, asset) };
}

export function renderDownloadMeta(meta) {
  return `<dl class="download-meta" data-release-derived="true">\n  <dt>Version</dt><dd>${meta.version}</dd>\n  <dt>Released</dt><dd>${meta.published}</dd>\n  <dt>SHA-256</dt><dd><code>${meta.sha256}</code></dd>\n</dl>`;
}

function parseArgs(argv) {
  const values = {};
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    if (flag === "--help") return { help: true };
    const value = argv[++index];
    if (!["--latest", "--checksums", "--asset", "--output"].includes(flag) || !value) throw new Error("Usage: node scripts/build-download-meta.mjs --latest latest.json --checksums SHA256SUMS.txt --asset installer.exe --output download-meta.html");
    values[flag.slice(2)] = value;
  }
  for (const name of ["latest", "checksums", "asset", "output"]) if (!values[name]) throw new Error("all four arguments are required");
  return values;
}

if (import.meta.main) {
  try {
    const options = parseArgs(process.argv.slice(2));
    if (options.help) console.log("Usage: node scripts/build-download-meta.mjs --latest latest.json --checksums SHA256SUMS.txt --asset installer.exe --output download-meta.html");
    else await writeFile(options.output, `${renderDownloadMeta(downloadMeta(JSON.parse(await readFile(options.latest, "utf8")), await readFile(options.checksums, "utf8"), options.asset))}\n`);
  } catch (error) { console.error(`T11-T11 FAIL: ${error.message}`); process.exitCode = 1; }
}
