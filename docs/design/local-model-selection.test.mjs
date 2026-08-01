import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const selectionPath = new URL("./local-model-selection.md", import.meta.url);

async function shortlistData() {
  const document = await readFile(selectionPath, "utf8");
  const match = document.match(/<!-- local-model-selection-data\n([\s\S]*?)\n-->/);
  assert.ok(match, "selection document must contain machine-readable shortlist data");
  return JSON.parse(match[1]);
}

test("licence-qualified candidates are ordered from smallest install", async () => {
  const shortlist = await shortlistData();

  assert.equal(shortlist.selectionStage, "shortlist");
  assert.equal(shortlist.licenseRequirement, "Apache-2.0");
  assert.deepEqual(
    shortlist.candidates.map(({ license }) => license),
    ["Apache-2.0", "Apache-2.0"],
  );
  assert.deepEqual(
    shortlist.candidates.map(({ artifactBytes }) => artifactBytes),
    [...shortlist.candidates.map(({ artifactBytes }) => artifactBytes)].sort((a, b) => a - b),
  );
});

test("runtime preserves the complete next-token distribution for stego adapters", async () => {
  const shortlist = await shortlistData();

  assert.deepEqual(shortlist.runtime, {
    crate: "llama-cpp-2",
    api: "LlamaContext::get_logits() -> &[f32]",
    distributionAccess: "full-vocabulary-raw-logits",
    samplerBoundary: "adapter-must-derive-probabilities-before-any-runtime-sampler-or-truncation",
  });
});
