import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import test from "node:test";

function runCheck(page) {
  const result = spawnSync(
    "node",
    ["scripts/check-donate-page-words.mjs", ...(page ? [page] : [])],
    { encoding: "utf8" },
  );
  return { status: result.status, output: `${result.stdout}\n${result.stderr}` };
}

test("Donate page says donations do not unlock Pro and has no Pro-code claim", () => {
  const output = execFileSync("node", ["scripts/check-donate-page-words.mjs"], { encoding: "utf8" });
  assert.match(output, /required sentence: "Donations do not unlock Pro\."/);
  assert.match(output, /donation Pro-code claims: 0/);
  assert.match(output, /donation unlocks Pro claims: 0/);
});

test("Donate page says donations support OSL, lists plain payment choices, promises no Pro code", () => {
  const green = runCheck();
  assert.equal(green.status, 0);
  assert.match(green.output, /support sentence: "Donations support OSL\."/);
  assert.match(green.output, /required sentence: "Donations do not unlock Pro\."/);
  assert.match(green.output, /payment choices: 3 \(Card, Bitcoin, Monero\)/);
  assert.match(green.output, /Pro-code promises: 0/);
});

test("Donate page check rejects a fixture saying a donation unlocks Pro", () => {
  const red = runCheck("docs/fixtures/donate-unlocks-pro.html");
  assert.equal(red.status, 1);
  assert.match(red.output, /donation unlocks Pro claims: 1/);
});

test("Donate page check rejects a page with no support sentence", () => {
  const red = runCheck("docs/fixtures/donate-no-support.html");
  assert.equal(red.status, 1);
  assert.match(red.output, /support sentence: MISSING/);
  assert.match(red.output, /missing support sentence: Donations support OSL\./);
});

test("Donate page check rejects a page with no plain payment choices", () => {
  const red = runCheck("docs/fixtures/donate-no-payment-choices.html");
  assert.equal(red.status, 1);
  assert.match(red.output, /payment choices: 0/);
  assert.match(red.output, /found 0 plain payment choice\(s\), need at least 2/);
});

test("Donate page check rejects a page promising a Pro code for a donation", () => {
  const red = runCheck("docs/fixtures/donate-pro-code-promise.html");
  assert.equal(red.status, 1);
  assert.match(red.output, /Pro-code promises: 1/);
  assert.match(red.output, /promise: Donate \$20 and we send you a Pro code\./);
});
