import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const contractPath = new URL("./offline-controls-and-opened-receipts.md", import.meta.url);
const source = readFileSync(contractPath, "utf8");
const match = source.match(/## T2 reconciliation contract[\s\S]*?```json\s+([\s\S]*?)\s+```/u);

assert.ok(match, "the design must state its reconciled T2 evidence");
const contract = JSON.parse(match[1]);

function featureStatus(evidence) {
  return evidence.receiptTransportWired
    && evidence.serverEnforcedBurn
    && evidence.blobReservationWired
    && evidence.userFacingControlsWired
    ? "end-to-end-wired"
    : "partial-not-end-to-end";
}

test("T2 does not upgrade local primitives into an end-to-end feature claim", () => {
  assert.equal(contract.version, 1);
  assert.equal(contract.evidence.localControlPrimitives, true);
  assert.equal(contract.evidence.senderReceiptFormatting, true);
  assert.equal(contract.evidence.receiptTransportWired, false);
  assert.equal(contract.evidence.serverEnforcedBurn, false);
  assert.equal(contract.evidence.blobReservationWired, false);
  assert.equal(contract.evidence.userFacingControlsWired, false);
  assert.equal(contract.status, featureStatus(contract.evidence));
});
