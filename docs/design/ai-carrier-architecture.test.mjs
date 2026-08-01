import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

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
