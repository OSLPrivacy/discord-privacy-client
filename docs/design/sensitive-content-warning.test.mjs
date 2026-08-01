import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const contractPath = new URL("./sensitive-content-warning.md", import.meta.url);
const source = readFileSync(contractPath, "utf8");
const match = source.match(/## Contract test vectors[\s\S]*?```json\s+([\s\S]*?)\s+```/u);

assert.ok(match, "the contract must include its JSON test vectors");
const contract = JSON.parse(match[1]);

function expectedSendBehavior({
  protectedSend,
  finding,
  globalOff,
  categoryMuted,
  dismissedForDraft,
}) {
  if (protectedSend || !finding || globalOff || categoryMuted || dismissedForDraft) {
    return "send";
  }
  return "warning";
}

test("the warning remains a local, non-blocking consequence choice", () => {
  assert.equal(contract.version, 1);
  assert.deepEqual(contract.warning, {
    trigger: "before-unencrypted-send",
    localOnly: true,
    retainsPlaintext: false,
    logsFindings: false,
    canBlock: false,
    canJudge: false,
    choices: ["send-unencrypted", "protect-with-osl", "keep-editing"],
  });
});

test("the warning appears only for an enabled unencrypted finding", () => {
  for (const scenario of contract.cases) {
    assert.equal(expectedSendBehavior(scenario), scenario.expect, scenario.name);
  }
});

test("categories are grouped by their actual detector and v1 stays text-only", () => {
  assert.deepEqual(contract.categories, {
    shippingScope: { text: true, attachments: false },
    detectorFamilies: {
      structuredText: [
        "credential",
        "recovery-material",
        "payment-card",
        "government-identity",
        "precise-location",
      ],
      lexicalText: [
        "profanity",
        "sexual-content",
        "sensitive-health",
        "controlled-substances",
        "potentially-unlawful-conduct",
        "work-sensitive-information",
      ],
      attachmentOnly: ["explicit-imagery", "imagery-targeting-a-group"],
    },
    unsupportedDetectors: ["general-entropy", "slur-or-toxicity-model"],
  });
});
