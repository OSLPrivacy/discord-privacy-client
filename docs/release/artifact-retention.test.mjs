import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const policyPath = new URL("./artifact-retention.md", import.meta.url);

async function loadPolicy() {
  const document = await readFile(policyPath, "utf8");
  const match = document.match(/```json artifact-retention-policy\n([\s\S]*?)\n```/);
  assert.ok(match, "artifact retention policy must contain its JSON contract");
  return JSON.parse(match[1]);
}

test("retention policy preserves vulnerable releases while requiring an actionable advisory", async () => {
  const policy = await loadPolicy();

  assert.equal(policy.published_release_retention, "indefinite");
  assert.equal(policy.vulnerable_installer, "keep-downloadable-with-prominent-advisory");
  assert.deepEqual(policy.advisory_requirements, [
    "affected versions",
    "impact and practical mitigation",
    "first fixed version",
    "link to the current safe installer",
  ]);
});

test("retention policy permits removal only with a durable public record", async () => {
  const policy = await loadPolicy();

  assert.equal(policy.ordinary_unpublish, "forbidden");
  assert.deepEqual(policy.exceptional_removal.allowed_for, [
    "confirmed malicious or unauthorized release artifact",
    "binding legal requirement",
  ]);
  assert.deepEqual(policy.exceptional_removal.required_record, [
    "version and release tag",
    "artifact names and SHA-256 digests",
    "removal time and reason",
    "replacement or incident advisory",
  ]);
});
