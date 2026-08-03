import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync("keyserver-cf/src/lib/stripe-checkout-claims.ts", "utf8");

test("one-time license rows use an opaque entitlement id, not the Stripe payment id", () => {
  assert.match(source, /function oneTimeEntitlementId\(licenseHash: string\): string/);
  assert.match(source, /const entitlementId = oneTimeEntitlementId\(claim\.license_hash\);/);
  assert.match(source, /INSERT OR IGNORE INTO licenses \([\s\S]*license_hash, subscription_id,[\s\S]*\)\.bind\(\s*claim\.license_hash,\s*entitlementId,/);
  assert.doesNotMatch(source, /INSERT OR IGNORE INTO licenses \([\s\S]*\)\.bind\(\s*claim\.license_hash,\s*input\.paymentIntentId,/);
});

test("terminal one-time payment observations revoke through the checkout claim join", () => {
  assert.match(source, /async function revokeOneTimeLicensesForPayment/);
  assert.match(source, /SELECT license_hash FROM stripe_checkout_claims\s+WHERE subscription_id = \?/);
  assert.match(source, /JOIN stripe_checkout_claims\s+ON stripe_checkout_claims\.license_hash = licenses\.license_hash/);
});
