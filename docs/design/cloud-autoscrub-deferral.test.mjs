import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const recordPath = new URL("./cloud-autoscrub-deferral.md", import.meta.url);

function decisionRecord() {
  const document = readFileSync(recordPath, "utf8");
  const match = document.match(/```json\n([\s\S]*?)\n```/);
  assert.ok(match, "the decision record must contain machine-readable metadata");
  return JSON.parse(match[1]);
}

test("cloud AutoScrub remains deferred for v1 until every privacy control is reviewed", () => {
  const record = decisionRecord();

  assert.equal(record.decision, "DEFERRED");
  assert.equal(record.scope, "cloud-autoscrub");
  assert.equal(record.release, "v1");
  assert.deepEqual(record.reconsideration_requires, [
    "high-sensitivity warning",
    "separate explicit consent",
    "per-run isolation",
    "limited retention",
    "wiping",
    "deletion receipt",
  ]);
});

test("does not mislabel reviewed-and-launched AutoScrub as attended", () => {
  const document = readFileSync(recordPath, "utf8");
  assert.match(document, /may then continue without someone watching/);
  assert.doesNotMatch(document, /attended, local Pro AutoScrub/i);
});
