import assert from "node:assert/strict";
import test from "node:test";
import { downloadMeta, renderDownloadMeta } from "./build-download-meta.mjs";

const asset = "osl-hub-0.1.0-x64-nsis.exe";
const digest = "a".repeat(64);
const latest = { version: "0.1.0", pub_date: "2026-08-02T12:00:00Z" };

test("T11-T11 renders version, date and SHA exclusively from latest.json plus SHA256SUMS", () => {
  const meta = downloadMeta(latest, `${digest}  ${asset}\n`, asset);
  assert.deepEqual(meta, { asset, version: "0.1.0", published: "2026-08-02", sha256: digest });
  assert.match(renderDownloadMeta(meta), /data-release-derived="true"/);
});

test("T11-T11 sabotage: stale hardcoded 0.0.1 cannot satisfy release-derived metadata", () => {
  const meta = downloadMeta(latest, `${digest}  ${asset}\n`, asset);
  assert.notEqual(meta.version, "0.0.1");
  assert.throws(() => downloadMeta(latest, `${digest}  osl-privacy-0.0.1.msi\n`, asset), /no valid entry/);
});
