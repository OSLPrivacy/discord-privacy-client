import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const documentPath = new URL("./rn-pin-recovery.md", import.meta.url);
const source = readFileSync(documentPath, "utf8");
const match = source.match(/## Contract test vectors[\s\S]*?```json\s+([\s\S]*?)\s+```/u);

assert.ok(match, "the RN pin recovery design must expose machine-readable contract vectors");
const contract = JSON.parse(match[1]);

test("unpin requires two out-of-band identity confirmations", () => {
  assert.equal(contract.version, 1);
  assert.equal(contract.pinLowering, "explicit_both_sides_confirmed_out_of_band_unpin_only");
  assert.deepEqual(contract.requires, [
    "same-relationship-binding",
    "two-identity-signatures",
    "independent-local-confirmations",
    "out-of-band-comparison",
    "unexpired-single-use-request",
  ]);
});

test("unpin neither creates an automatic downgrade nor loses RN messages", () => {
  assert.deepEqual(contract.forbids, [
    "automatic-expiry",
    "local-only-unpin",
    "network-only-confirmation",
    "session-delete-lowers-pin",
    "v3-retry-of-pending-rn-wire",
  ]);
  assert.deepEqual(contract.postApply, {
    deleteRnSession: true,
    clearPin: true,
    preserveQueuedRnPlaintext: true,
    requiresFreshBootstrapForRn: true,
  });
});
