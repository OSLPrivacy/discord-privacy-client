import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import test from "node:test";

test("Checkout success fixture shows activation-code redemption facts without a dead end", () => {
  const output = execFileSync("node", ["scripts/check-checkout-success-page-words.mjs"], { encoding: "utf8" });
  assert.match(output, /activation codes: 1 \(OSL-AB12-CD34-EF56-GH78\)/);
  assert.match(output, /startDateRule: "Your month starts when you enter the code in the OSL app\."/);
  assert.match(output, /noRenewal: "This code does not renew\."/);
  assert.match(output, /noCardStorage: "OSL does not store your card\."/);
  assert.match(output, /appRedemption: "Redeem this code in the OSL app\."/);
  assert.match(output, /app redemption links: 1/);
  assert.match(output, /dead end: no/);
});

test("Checkout success checker rejects a fixture with no app redemption action", () => {
  const red = spawnSync("node", [
    "scripts/check-checkout-success-page-words.mjs",
    "docs/fixtures/checkout-success-dead-end.html",
  ], { encoding: "utf8" });
  assert.equal(red.status, 1);
  assert.match(`${red.stdout}\n${red.stderr}`, /dead end: yes/);
  assert.match(`${red.stdout}\n${red.stderr}`, /missing app redemption link for the visible activation code/);
});
