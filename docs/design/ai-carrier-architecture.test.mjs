import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const contractPath = new URL("./ai-carrier-architecture.md", import.meta.url);
const source = readFileSync(contractPath, "utf8");
const match = source.match(/## Credits purchase contract[\s\S]*?```json\s+([\s\S]*?)\s+```/u);

assert.ok(match, "the architecture must include the credits purchase contract");
const contract = JSON.parse(match[1]);

function purchaseAvailability({ cloudGenerationOffered, checkoutGateGreen }) {
  return cloudGenerationOffered && checkoutGateGreen ? "available" : "not-offered";
}

test("v1 has no cloud-credit purchase path", () => {
  assert.deepEqual(contract.v1, {
    cloudGenerationOffered: false,
    creditPurchaseOffered: false,
    supportedCryptoChains: [],
  });
});

test("a future credit purchase stays gated and belongs to T11", () => {
  assert.equal(contract.future.checkoutOwner, "T11");
  assert.equal(contract.future.requiresCheckoutGate, true);
  assert.equal(contract.future.proEntitlementIsCreditBalance, false);
  assert.equal(
    purchaseAvailability({ cloudGenerationOffered: false, checkoutGateGreen: true }),
    "not-offered",
  );
  assert.equal(
    purchaseAvailability({ cloudGenerationOffered: true, checkoutGateGreen: false }),
    "not-offered",
  );
  assert.equal(
    purchaseAvailability({ cloudGenerationOffered: true, checkoutGateGreen: true }),
    "available",
  );
});
