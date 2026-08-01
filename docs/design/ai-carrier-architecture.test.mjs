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

import { readFile } from "node:fs/promises";

// documentPath: reuse contractPath declared above.

test("AI carrier architecture records the rejected cloud-privacy alternatives", async () => {
  const document = await readFile(contractPath, "utf8");
  const compactDocument = document.replace(/\s+/g, " ");

  assert.match(compactDocument, /## Why not encrypt the cloud call\?/);
  assert.match(compactDocument, /FHE for LLM inference.*20 s prefill.*18 s per token/);
  assert.match(compactDocument, /MPC.*200 s.*1\.8 GB.*single output token/);
  assert.match(compactDocument, /embedding.*92%.*32-token inputs.*exactly/i);
  assert.match(compactDocument, /Confidential computing.*unnecessary.*plaintext is never sent/);
  assert.match(compactDocument, /trusting the silicon vendor/i);
});

const architecturePath = new URL("./ai-carrier-architecture.md", import.meta.url);

function currentFreeCoverRecord() {
  const document = readFileSync(architecturePath, "utf8");
  const match = document.match(
    /<!-- current-free-cover-record\n([\s\S]*?)\n-->/,
  );

  assert.ok(match, "the current free-cover record must be declared");
  return JSON.parse(match[1]);
}

test("records the fixed free-cover beacon and its required replacement", () => {
  assert.deepEqual(currentFreeCoverRecord(), {
    currentCarrier: "fixed beacon string",
    currentValue: "🔒 OSL private message",
    observerSignal: "an exact-match classifier identifies it",
    productionStealth: "off",
    requiredFreeCarrier: "word-bank carrier only",
  });
});

const decisionMatch = source.match(/### Decision contract[\s\S]*?```json\s+([\s\S]*?)\s+```/u);

assert.ok(decisionMatch, "the LLM-driven coding decision must have a JSON contract");
const decisionContract = JSON.parse(decisionMatch[1]);

test("LLM-driven coding remains rejected unless both revival gates are met", () => {
  assert.equal(decisionContract.version, 1);
  assert.deepEqual(decisionContract.llmDrivenCoding, {
    v1Status: "evaluated-and-rejected",
    isFallback: false,
    revival: {
      requiresNegotiatedPerConversationCapability: true,
      requiresMatchingTrustedModelPackArtifactDigest: true,
      requiresCrossCpuMeasuredDecodeFailureRate: true,
    },
  });
});
