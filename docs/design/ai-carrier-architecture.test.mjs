import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const documentPath = new URL("./ai-carrier-architecture.md", import.meta.url);

test("AI carrier architecture records the rejected cloud-privacy alternatives", async () => {
  const document = await readFile(documentPath, "utf8");
  const compactDocument = document.replace(/\s+/g, " ");

  assert.match(compactDocument, /## Why not encrypt the cloud call\?/);
  assert.match(compactDocument, /FHE for LLM inference.*20 s prefill.*18 s per token/);
  assert.match(compactDocument, /MPC.*200 s.*1\.8 GB.*single output token/);
  assert.match(compactDocument, /embedding.*92%.*32-token inputs.*exactly/i);
  assert.match(compactDocument, /Confidential computing.*unnecessary.*plaintext is never sent/);
  assert.match(compactDocument, /trusting the silicon vendor/i);
});
