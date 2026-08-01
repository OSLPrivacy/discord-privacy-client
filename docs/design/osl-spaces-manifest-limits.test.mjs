import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const contractPath = new URL("./osl-spaces-manifest-limits.md", import.meta.url);
const source = readFileSync(contractPath, "utf8");
const match = source.match(/## Contract vector[\s\S]*?```json\s+([\s\S]*?)\s+```/u);

assert.ok(match, "the manifest-limit finding must include a machine-checkable vector");
const contract = JSON.parse(match[1]);

function deviceCapacity({ blobCapBytes, entryBytes, framingBytes }) {
  return Math.floor((blobCapBytes - framingBytes) / entryBytes);
}

test("the 64 KiB manifest cannot exceed its stated recipient-device ceiling", () => {
  assert.equal(
    deviceCapacity({
      blobCapBytes: contract.blobCapBytes,
      entryBytes: contract.maxSealedEntryBytes,
      framingBytes: 0,
    }),
    contract.zeroFramingDeviceUpperBound,
  );
  assert.equal(
    deviceCapacity({
      blobCapBytes: contract.blobCapBytes,
      entryBytes: contract.maxSealedEntryBytes,
      framingBytes: contract.minimumFramingBytes,
    }),
    contract.positiveFramingDeviceUpperBound,
  );
});

test("a roughly-200-member, five-device Space fits below the framing-aware bound", () => {
  const requiredEntries =
    contract.approximateMembersAtFiveDevices * contract.assumedDevicesPerMember;
  assert.ok(
    requiredEntries <= contract.positiveFramingDeviceUpperBound,
    "the documented approximate member count must fit the framing-aware device ceiling",
  );
  assert.equal(contract.requiresBoundedEntrySelection, true);
});
