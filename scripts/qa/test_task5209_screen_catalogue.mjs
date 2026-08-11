import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import test from "node:test";

test("5209 packaged screen catalogue and 5209b mutations", () => {
  const green = spawnSync("node", ["scripts/qa/task-5209-screen-catalogue.mjs"], { encoding: "utf8" });
  assert.equal(green.status, 0, `${green.stdout}\n${green.stderr}`);
  assert.match(green.stdout, /source=56 packaged=56 runtime=56 oracle=56/);
  assert.match(green.stdout, /literals=0 missing_keys=0 generic_collisions=0 swapped_controls=0/);
  assert.match(green.stdout, /changed_keys=3 changed_items=3 changed_surfaces=3/);
  const red = spawnSync("bash", ["scripts/qa/task-5209b-break-it.sh"], { encoding: "utf8" });
  assert.equal(red.status, 0, `${red.stdout}\n${red.stderr}`);
  for (const mutant of ["unregistered-surface", "hard-coded-item", "swap-confirm-cancel", "generic-collision", "self-derived-oracle"]) {
    assert.match(red.stdout, new RegExp(`mutant=${mutant} exit=1`));
  }
});
