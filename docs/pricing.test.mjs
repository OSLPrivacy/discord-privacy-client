import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import test from "node:test";

test("TASK 1528 Pricing fixture repeats the shared pricing facts with one working purchase route", () => {
  const output = execFileSync("node", ["scripts/check-pricing-page-words.mjs"], { encoding: "utf8" });
  assert.match(output, /fact_1: "5 dollars"/);
  assert.match(output, /fact_2: "one month from code entry"/);
  assert.match(output, /fact_3: "no renewal"/);
  assert.match(output, /fact_4: "no OSL card storage"/);
  assert.match(output, /working purchase routes: 1/);
  assert.match(output, /check-pricing-page-words: complete\./);
});

test("TASK 1528 checker rejects a fixture missing the shared pricing facts", () => {
  const red = spawnSync("node", [
    "scripts/check-pricing-page-words.mjs",
    "docs/fixtures/pricing-missing-facts.html",
  ], { encoding: "utf8" });
  assert.equal(red.status, 1);
  const combined = `${red.stdout}\n${red.stderr}`;
  assert.match(combined, /fact_1: MISSING/);
  assert.match(combined, /missing shared pricing fact\(s\)/);
});

test("TASK 1528 checker rejects a fixture that routes to more than the healthy purchase path", () => {
  const red = spawnSync("node", [
    "scripts/check-pricing-page-words.mjs",
    "docs/fixtures/pricing-dead-route.html",
  ], { encoding: "utf8" });
  assert.equal(red.status, 1);
  const combined = `${red.stdout}\n${red.stderr}`;
  assert.match(combined, /purchase routes: 2/);
  assert.match(combined, /expected exactly 1 working purchase route, found 2/);
});
