import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const policyPath = new URL("./transparency-log-policy.md", import.meta.url);

async function loadPolicy() {
  const document = await readFile(policyPath, "utf8");
  const match = document.match(/```json transparency-log-policy\n([\s\S]*?)\n```/);
  assert.ok(match, "transparency-log policy must contain its JSON contract");
  return JSON.parse(match[1]);
}

test("published hashes stay local and transparency logs stay out of runtime", async () => {
  const policy = await loadPolicy();

  assert.equal(policy.format, 1);
  assert.equal(policy.published_hash_list.delivery, "bundle the signed build-hashes.json list and signature into the client at build time");
  assert.equal(policy.published_hash_list.network_refresh, "forbidden");
  assert.equal(policy.transparency_log.provider, "Sigstore/Rekor via actions/attest-build-provenance");
  assert.equal(policy.transparency_log.runtime_queries, "forbidden");
  assert.equal(policy.enforcement.gate, "node scripts/check_no_runtime_transparency_queries.mjs");
  assert.deepEqual(policy.enforcement.runtime_roots, ["apps/osl-hub/src", "apps/osl-hub-ui/src"]);
});
