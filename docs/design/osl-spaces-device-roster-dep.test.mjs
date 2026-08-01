import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const contractPath = new URL("./osl-spaces-device-roster-dep.md", import.meta.url);
const source = readFileSync(contractPath, "utf8");
const match = source.match(/## Contract test vectors[\s\S]*?```json\s+([\s\S]*?)\s+```/u);

assert.ok(match, "the dependency contract must include device-roster test vectors");
const contract = JSON.parse(match[1]);

function deliveryAllowed({ activeDeviceIds, targetDeviceIds }) {
  const active = new Set(activeDeviceIds);
  return (
    targetDeviceIds.length === active.size &&
    targetDeviceIds.every((deviceId) => active.has(deviceId))
  );
}

test("every active device is an independent required delivery target", () => {
  for (const scenario of [contract.singleDevice, contract.twoDevices, contract.staleDevice]) {
    const actual = deliveryAllowed(scenario) ? "deliver" : "reject";
    assert.equal(actual, scenario.expect);
  }
});

test("D17 and Device Transfer cannot be implemented as account-level fallback", () => {
  assert.equal(contract.version, 1);
  assert.deepEqual(contract.deviceRosterRequiredBefore, [
    "d17-multi-device-delivery",
    "space-fan-out",
  ]);
  assert.deepEqual(contract.requirements, {
    distinctPrekeyBundlePerDevice: true,
    perDeviceCopy: true,
    perDeviceAcknowledgement: true,
    noAccountFallbackTarget: true,
    deviceTransferExplicitAndOwnerAuthenticated: true,
    deviceTransferDestinationProvesPossession: true,
    deviceTransferWorksOffline: true,
  });
});
