import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const evidenceDir = dirname(fileURLToPath(import.meta.url));
const report = JSON.parse(readFileSync(join(evidenceDir, "audit.json"), "utf8"));
const requiredArtifacts = ["build-identity.json", "screenshots/status-tags.png", "run.log"];

function validateAudit(audit, artifactExists) {
  assert.equal(audit.task, "t7-01");
  assert.equal(audit.platform, "linux");
  assert.match(audit.sourceCommit, /^[0-9a-f]{40}$/);

  if (audit.auditState === "complete") {
    for (const artifact of requiredArtifacts) {
      assert.ok(artifactExists(artifact), `completed audit is missing ${artifact}`);
    }
    for (const verdict of Object.values(audit.verdicts)) {
      assert.notEqual(verdict, "not-assessed", "completed audit cannot leave a verdict unassessed");
    }
    return;
  }

  assert.equal(audit.auditState, "blocked");
  assert.match(audit.blocker, /prohibits Cargo/i);
  for (const verdict of Object.values(audit.verdicts)) {
    assert.equal(verdict, "not-assessed");
  }
}

test("T7-01 evidence does not claim a completed fresh-binary audit without its artifacts", () => {
  validateAudit(report, (artifact) => existsSync(join(evidenceDir, artifact)));
});

test("a completed claim with no captured binary evidence is rejected", () => {
  const fabricatedCompletion = { ...report, auditState: "complete" };
  assert.throws(
    () => validateAudit(fabricatedCompletion, () => false),
    /completed audit is missing build-identity\.json/,
  );
});
