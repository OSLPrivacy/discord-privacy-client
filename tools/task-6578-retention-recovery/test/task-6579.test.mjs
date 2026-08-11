import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { FAILURE_EDGES, MUTANTS } from "../src/model.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const run = (args, env = {}) => spawnSync(process.execPath, ["src/cli.mjs", ...args], {
  cwd: ROOT, encoding: "utf8", env: { ...process.env, ...env },
});

test("6579 proves unattended recovery, exact report, crawls, and every mutant", () => {
  const result = run(["6579"]);
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, /due_deleted=8/);
  assert.match(result.stdout, /not_due_preserved=4/);
  assert.match(result.stdout, /failure_edges=8/);
  assert.match(result.stdout, /telegram_reports=1/);
  assert.match(result.stdout, /mutants_red=8/);
});

for (const mutant of MUTANTS) test(`6579 rejects mutant ${mutant}`, () => {
  const result = run(["6579", "--mutant", mutant]);
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stderr, /TASK6579 FAIL/);
});

for (const edge of FAILURE_EDGES) test(`6579 starvation names absent failure ${edge}`, () => {
  const result = run(["6579"], { OSL_6579_STARVE: `failure:${edge}` });
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stderr, new RegExp(`absent starvation failure=${edge}`));
});

for (const mutant of MUTANTS) test(`6579 starvation names absent mutant ${mutant}`, () => {
  const result = run(["6579"], { OSL_6579_STARVE: `mutant:${mutant}` });
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stderr, new RegExp(`absent starvation mutant=${mutant}`));
});
