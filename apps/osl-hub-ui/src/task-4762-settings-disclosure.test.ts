import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const task4762 = readFileSync(
  new URL("../../../keyserver-cf/scripts/task-4762-watcher-table.mjs", import.meta.url),
  "utf8",
);

const disclosureSentence =
  "If you choose Anyone, a stranger holding your handle learns yes, and that cannot be un-learned.";

function anchoredSource(start: string, end: string): string {
  const startIndex = source.indexOf(start);
  expect(startIndex, `missing source anchor: ${start}`).toBeGreaterThanOrEqual(0);
  const endIndex = source.indexOf(end, startIndex);
  expect(endIndex, `missing source anchor: ${end}`).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("TASK 4762 settings disclosure", () => {
  it("puts the audited one-line summary on the settings privacy screen", () => {
    const settings = anchoredSource(
      "function privacySettingsContent",
      "function autoScrubAssistantMarkup",
    );
    expect(task4762).toContain(`TASK_4762_DISCLOSURE_SENTENCE =\n  "${disclosureSentence}"`);
    expect(source.match(new RegExp(disclosureSentence.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "g"))).toHaveLength(1);
    expect(settings).toContain("oslHandleDiscoveryDisclosureSentence");
    expect(settings).toContain("settings-disclosure-sentence");
  });
});
