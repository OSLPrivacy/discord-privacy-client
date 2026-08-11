import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import test from "node:test";

const MUTANTS = [
  "routed-hard-coded-sentence",
  "modal-hard-coded-button",
  "hard-coded-hover-title-reason",
  "unregistered-reachable-screen",
  "swap-confirm-cancel",
  "generic-destructive-harmless-label",
  "cross-wired-security-reason",
];

test("5209 packaged screen catalogue and every 5209b isolated mutation", () => {
  const green = spawnSync("node", ["scripts/qa/task-5209-screen-catalogue.mjs"], { encoding: "utf8" });
  assert.equal(green.status, 0, `${green.stdout}\n${green.stderr}`);
  assert.match(green.stdout, /source=56 packaged=56 runtime=56 oracle=56/);
  assert.match(green.stdout, /items=1626 invoked_actions=1626 literals=0 missing_keys=0 generic_collisions=0 swapped_controls=0 cross_wired_mappings=0/);
  assert.match(green.stdout, /changed_keys=3 changed_items=3 changed_surfaces=3/);
  const red = spawnSync("bash", ["scripts/qa/task-5209b-break-it.sh"], { encoding: "utf8" });
  assert.equal(red.status, 0, `${red.stdout}\n${red.stderr}`);
  for (const mutant of MUTANTS) {
    assert.match(red.stdout, new RegExp(`mutant=${mutant} exit=1`));
  }
  assert.match(red.stdout, /disposals=7 builds=7 discarded_builds=7 temp_remaining=0/);
});

test("5209b starvation names every removed attack", () => {
  for (const mutant of MUTANTS) {
    const starved = spawnSync("bash", ["scripts/qa/task-5209b-break-it.sh"], {
      encoding: "utf8",
      env: { ...process.env, OSL_5209B_SKIP_MUTANT: mutant },
    });
    assert.equal(starved.status, 1, `${mutant}\n${starved.stdout}\n${starved.stderr}`);
    assert.match(`${starved.stdout}\n${starved.stderr}`, new RegExp(`absent mutant=${mutant}`));
  }
});
