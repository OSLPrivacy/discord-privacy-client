import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const contractPath = new URL("./entitlement.md", import.meta.url);
const source = readFileSync(contractPath, "utf8");
const match = source.match(/### Credits boundary vector[\s\S]*?```json\s+([\s\S]*?)\s+```/u);

assert.ok(match, "the entitlement contract must expose a credits boundary vector");
const boundary = JSON.parse(match[1]);

function encryptionAvailability({ entitlementState, creditBalance }) {
  void entitlementState;
  void creditBalance;
  return "available";
}

function cloudAiAvailability({ entitlementState, creditBalance }) {
  if (entitlementState !== "ACTIVE") return "word-bank";
  return creditBalance > 0 ? "cloud-ai" : "word-bank";
}

test("credits never become a Pro entitlement or balance", () => {
  assert.equal(boundary.credits.purchase, "separate-one-time");
  assert.equal(boundary.credits.balanceGrantsPro, false);
  assert.equal(boundary.credits.proCreatesBalance, false);
});

test("zero credits affect optional cloud AI only", () => {
  assert.equal(boundary.encryption.dependsOnEntitlement, false);
  assert.equal(boundary.encryption.dependsOnCredits, false);
  assert.equal(boundary.zeroBalance.degrades, "optional-cloud-ai-only");
  assert.equal(boundary.zeroBalance.fallback, "word-bank");
  assert.equal(cloudAiAvailability({ entitlementState: "ACTIVE", creditBalance: 0 }), "word-bank");
  assert.equal(encryptionAvailability({ entitlementState: "ACTIVE", creditBalance: 0 }), "available");
  assert.equal(encryptionAvailability({ entitlementState: "EXPIRED", creditBalance: 25 }), "available");
});
