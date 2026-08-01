import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const threatModel = readFileSync(new URL("./THREAT_MODEL.md", import.meta.url), "utf8");

function transportBoundary() {
  const blocks = [...threatModel.matchAll(/```json transport-boundary-v1\n([\s\S]*?)\n```/gu)];
  assert.equal(blocks.length, 1, "THREAT_MODEL must contain one transport boundary contract");
  return JSON.parse(blocks[0][1]);
}

test("threat model commits to pointer-only eager-fetch transport", () => {
  assert.deepEqual(transportBoundary(), {
    carrier: "opaque_pointer_only",
    payload: "cipher_store_blob",
    inline_fallback: false,
    payload_fetch: "eager",
  });
});
