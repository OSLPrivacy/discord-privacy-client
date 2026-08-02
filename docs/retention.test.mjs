import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const policyPath = new URL("./retention.md", import.meta.url);

async function loadPolicy() {
  const document = await readFile(policyPath, "utf8");
  const match = document.match(/```json retention-claim-policy\n([\s\S]*?)\n```/);
  assert.ok(match, "retention policy must contain its JSON contract");
  return JSON.parse(match[1]);
}

test("relay deletion claim stays ineligible until production durability evidence exists", async () => {
  const policy = await loadPolicy();

  assert.equal(policy.current_relay_deletion_claim, "not-eligible-no-production-probe-receipt");
  assert.deepEqual(policy.required_evidence, [
    "payload bytes are stored in R2 rather than D1 or a SQLite Durable Object",
    "the exact deployed workers.dev Worker passes deletion-durability-probe",
    "the probe observes a 404 through the PAYLOADS binding after acknowledgement",
    "the serving bucket has no lock rules",
    "the evidence records that R2 versioning is unsupported",
  ]);
});

test("retention copy requires limits on copies, future logging, and relay metadata", async () => {
  const policy = await loadPolicy();

  assert.deepEqual(policy.required_limitations, [
    "does not remove copies already received by people or providers",
    "does not prevent a provider or network from logging future activity",
    "relay observes request timing and IP address unless OHTTP is in use",
  ]);
  assert.equal(
    policy.forward_looking_order,
    "deletion does not defeat it; unlinkability is the relevant defense",
  );
});
