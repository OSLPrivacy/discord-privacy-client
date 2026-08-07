import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import test from "node:test";

test("Donate page says donations do not unlock Pro and has no Pro-code claim", () => {
  const output = execFileSync("node", ["scripts/check-donate-page-words.mjs"], { encoding: "utf8" });
  assert.match(output, /required sentence: "Donations do not unlock Pro\."/);
  assert.match(output, /donation Pro-code claims: 0/);
  assert.match(output, /donation unlocks Pro claims: 0/);
});

test("Donate page check rejects a fixture saying a donation unlocks Pro", () => {
  const red = spawnSync("node", [
    "scripts/check-donate-page-words.mjs",
    "docs/fixtures/donate-unlocks-pro.html",
  ], { encoding: "utf8" });
  assert.equal(red.status, 1);
  assert.match(`${red.stdout}\n${red.stderr}`, /donation unlocks Pro claims: 1/);
});
