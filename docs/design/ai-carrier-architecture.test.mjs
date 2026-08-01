import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const contractPath = new URL("./ai-carrier-architecture.md", import.meta.url);
const source = readFileSync(contractPath, "utf8");
const match = source.match(/### Decision contract[\s\S]*?```json\s+([\s\S]*?)\s+```/u);

assert.ok(match, "the LLM-driven coding decision must have a JSON contract");
const contract = JSON.parse(match[1]);

test("LLM-driven coding remains rejected unless both revival gates are met", () => {
  assert.equal(contract.version, 1);
  assert.deepEqual(contract.llmDrivenCoding, {
    v1Status: "evaluated-and-rejected",
    isFallback: false,
    revival: {
      requiresNegotiatedPerConversationCapability: true,
      requiresMatchingTrustedModelPackArtifactDigest: true,
      requiresCrossCpuMeasuredDecodeFailureRate: true,
    },
  });
});
