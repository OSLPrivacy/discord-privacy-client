import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { defaultLookState, lookScreenMarkup } from "./look-screen";
import {
  PLAIN_ENGLISH_BANNED_WORDS,
  checkScreenWords,
  type ScreenWordsExpectation,
} from "./screen-words";

const REPO = path.join(import.meta.dirname, "..", "..", "..");
const DESIGN_FEEL_DOC = path.join(REPO, "docs", "design", "osl-subjective-design-feel.md");
const NAMED_WORDS = ["Look", "Theme", "Light", "Dark", "Window size", "Save"] as const;
const LEAST_WORDS = 12;

function contractBannedConcepts(): string[] {
  const doc = readFileSync(DESIGN_FEEL_DOC, "utf8");
  const fence = /```json\s*([\s\S]*?)```/u.exec(doc);
  if (fence === null) throw new Error("no JSON fixture block in the design-feel doc");
  const fixture = JSON.parse(fence[1]) as { banned_user_facing_concepts?: string[] };
  const banned = fixture.banned_user_facing_concepts ?? [];
  if (banned.length === 0) throw new Error("design-feel doc declares no banned concepts");
  return banned;
}

function expectation(): ScreenWordsExpectation {
  return {
    title: "Look",
    requiredWords: NAMED_WORDS,
    leastWords: LEAST_WORDS,
    bannedWords: contractBannedConcepts(),
  };
}

describe("TASK 0775 Look screen words", () => {
  it("reads Look as the page title, enough words, every named word, and zero banned words", () => {
    const report = checkScreenWords(lookScreenMarkup(defaultLookState), expectation());

    console.log(
      `TASK 0775 report: title=${JSON.stringify(report.title)} words=${report.wordCount} `
      + `present=${JSON.stringify(report.present)} missing=${JSON.stringify(report.missing)} `
      + `banned=${report.banned.length} ${JSON.stringify(report.banned)} `
      + `bannedTermsChecked=${PLAIN_ENGLISH_BANNED_WORDS.length + contractBannedConcepts().length} ok=${report.ok}`,
    );

    expect(report.title).toBe("Look");
    expect(report.titleMatches).toBe(true);
    expect(report.wordCount).toBeGreaterThanOrEqual(LEAST_WORDS);
    expect(report.enoughWords).toBe(true);
    expect(report.present).toEqual([...NAMED_WORDS]);
    expect(report.missing).toEqual([]);
    expect(report.banned).toEqual([]);
    expect(report.ok).toBe(true);
  });

  it("fails on a throwaway copy missing one named word", () => {
    const markup = lookScreenMarkup(defaultLookState);
    const broken = markup.replace(">Window size<", "><");
    const report = checkScreenWords(broken, expectation());

    console.log(
      `TASK 0775 mutant: dropped=${JSON.stringify("Window size")} `
      + `missing=${JSON.stringify(report.missing)} ok=${report.ok}`,
    );

    expect(broken).not.toBe(markup);
    expect(report.missing).toEqual(["Window size"]);
    expect(report.ok).toBe(false);
  });
});
