import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(new URL("./check-task1369-two-way-delivery.mjs", import.meta.url));
const completeFixture = fileURLToPath(
  new URL("./fixtures/task1369-two-way-command-output.txt", import.meta.url),
);
const missingFixture = fileURLToPath(
  new URL("./fixtures/task1369-missing-bob-to-alice-command-output.txt", import.meta.url),
);

function runChecker(fixture) {
  return spawnSync(process.execPath, [script, fixture], {
    encoding: "utf8",
  });
}

test("accepts the Task 1369 command output only when both delivery directions land once", () => {
  const result = runChecker(completeFixture);

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /two-way delivery proof passed directions=2/u);
});

test("exits 1 and names the delivery direction removed from the fixture", () => {
  const result = runChecker(missingFixture);

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /missing delivery direction: task1369-bob-copy -> task1369-alice-copy/u,
  );
});
