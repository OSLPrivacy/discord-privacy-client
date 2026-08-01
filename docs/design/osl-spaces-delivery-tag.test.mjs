import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const contractPath = new URL("./osl-spaces-delivery-tag.md", import.meta.url);
const source = readFileSync(contractPath, "utf8");
const match = source.match(/## Contract test vector[\s\S]*?```json\s+([\s\S]*?)\s+```/u);

assert.ok(match, "the delivery-tag finding must include a machine-checkable vector");
const contract = JSON.parse(match[1]);

function canComputeTag({ derivationInputs, observerKnownInputs }) {
  const observerInputs = new Set(observerKnownInputs);
  return derivationInputs.every((input) => observerInputs.has(input));
}

test("group-shared derivation lets every Space member compute a peer's feed tag", () => {
  assert.equal(canComputeTag(contract.prohibited), true);
  assert.equal(contract.prohibited.observerCanComputeRecipientTag, true);
});

test("recipient-private input prevents another Space member computing a peer's tag", () => {
  assert.equal(canComputeTag(contract.recipientIsolatedExample), false);
  assert.equal(contract.recipientIsolatedExample.observerCanComputeRecipientTag, false);
  assert.equal(
    canComputeTag({
      derivationInputs: contract.recipientIsolatedExample.derivationInputs,
      observerKnownInputs: contract.recipientIsolatedExample.recipientKnownInputs,
    }),
    true,
  );
});

test("the contract records a joint decision without selecting it for T21", () => {
  assert.equal(contract.version, 1);
  assert.deepEqual(contract.eligibleDirections, [
    "pairwise-or-recipient-device-state",
    "per-member-subscription-secret",
  ]);
  assert.deepEqual(contract.requiredProperties, {
    groupSharedStateAloneProhibited: true,
    recipientPrivateInputRequired: true,
    otherSpaceMemberCannotComputeRecipientTag: true,
    t1AndT6JointDecisionRequired: true,
    t21DoesNotSelectDesign: true,
  });
});
