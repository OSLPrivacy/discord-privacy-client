import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const copyUrl = new URL("./scrub-consent-gate-copy.md", import.meta.url);
const bannedHedges = ["may violate terms", "elevated risk", "some users report"];

function extractTestVector(markdown) {
  const match = markdown.match(/## Test vector\s+```json\s+([\s\S]*?)\s+```/u);
  assert.ok(match, "the consent copy must include an SCR-K1 test vector");
  return JSON.parse(match[1]);
}

function gradeLevel(text) {
  const words = text.match(/[A-Za-z]+(?:'[A-Za-z]+)?/gu) ?? [];
  const sentences = text.match(/[.!?]+/gu) ?? [];
  const syllables = words.reduce((total, word) => {
    const groups = word.toLowerCase().replace(/e$/u, "").match(/[aeiouy]+/gu);
    return total + Math.max(1, groups?.length ?? 0);
  }, 0);

  return 0.39 * (words.length / sentences.length) + 11.8 * (syllables / words.length) - 15.59;
}

test("SCR-K1 makes the Discord termination and partial-run consequences explicit", async () => {
  const copy = extractTestVector(await readFile(copyUrl, "utf8"));
  const warning = Object.values(copy.warning).join(" ");

  assert.equal(copy.id, "SCR-K1");
  assert.equal(copy.service, "Discord");
  assert.match(copy.warning.termination, /permanently terminated/u);
  assert.match(copy.warning.untouchedContent, /never touched/u);
  assert.match(copy.warning.completedDeletes, /does not reverse/u);
  assert.match(copy.warning.partialRun, /half-done/u);
  assert.match(copy.providerEvidence, /forbidden, and can result in an account termination if found/u);
  for (const hedge of bannedHedges) {
    assert.doesNotMatch(warning.toLowerCase(), new RegExp(hedge, "u"));
  }
  assert.ok(gradeLevel(warning) <= 9, "the warning must read at grade 9 or below");
});
