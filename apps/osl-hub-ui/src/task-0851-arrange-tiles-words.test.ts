import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  PLAIN_ENGLISH_BANNED_WORDS,
  checkScreenWords,
  screenMarkupFromSource,
  screenTitle,
  sliceFunctionSource,
  visibleWords,
  type ScreenWordsExpectation,
} from "./screen-words";

const NAMED_WORDS = ["Arrange tiles", "Home", "Move up", "Move down", "Hide", "Done"] as const;
const LEAST_WORDS = 12;

function contractBannedConcepts(): string[] {
  const source = readFileSync(new URL("../../../docs/design/osl-subjective-design-feel.md", import.meta.url), "utf8");
  const fence = /```json\s*([\s\S]*?)```/u.exec(source);
  if (fence === null) throw new Error("design-feel contract has no JSON fixture block");
  const fixture = JSON.parse(fence[1]) as { banned_user_facing_concepts?: string[] };
  const banned = fixture.banned_user_facing_concepts ?? [];
  if (banned.length === 0) throw new Error("design-feel contract declares no banned concepts");
  return banned;
}

function expectation(): ScreenWordsExpectation {
  return {
    title: "Arrange tiles",
    requiredWords: NAMED_WORDS,
    leastWords: LEAST_WORDS,
    bannedWords: contractBannedConcepts(),
  };
}

function arrangeTilesMarkup(): string {
  // Read the production render function directly. This branch has unrelated
  // merge damage elsewhere in main.ts, so importing the whole module would
  // prevent this focused screen check from reaching its own assertions.
  const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
  const renderFunction = sliceFunctionSource(source, "arrangeTilesContent", "workspaceContent");
  return screenMarkupFromSource(renderFunction);
}

describe("TASK 0851 Arrange tiles screen words", () => {
  it("reads the production screen with the right title, named words, and no banned words", () => {
    const markup = arrangeTilesMarkup();
    const report = checkScreenWords(markup, expectation());

    console.log(
      `TASK0851 title=${JSON.stringify(report.title)} words_read=${report.wordCount} ` +
        `present=${JSON.stringify(report.present)} missing=${JSON.stringify(report.missing)} ` +
        `banned_found=${report.banned.length} banned_terms_checked=${PLAIN_ENGLISH_BANNED_WORDS.length + contractBannedConcepts().length}`,
    );

    expect(screenTitle(markup)).toBe("Arrange tiles");
    expect(report.wordCount).toBeGreaterThanOrEqual(LEAST_WORDS);
    expect(report.enoughWords).toBe(true);
    expect(report.present).toEqual([...NAMED_WORDS]);
    expect(report.missing).toEqual([]);
    expect(report.banned).toEqual([]);
    expect(report.ok).toBe(true);
  });

  it("reads visible copy rather than attributes or markup", () => {
    const words = visibleWords(arrangeTilesMarkup());
    expect(words).toContain("Move");
    expect(words).toContain("up");
    expect(words).toContain("down");
    expect(words).not.toContain("data-tile-move");
    expect(words).not.toContain("aria-label");
  });

  it("fails on a throwaway copy missing each named word", () => {
    const markup = arrangeTilesMarkup();
    for (const named of NAMED_WORDS) {
      const throwaway = markup.split(named).join("");
      const report = checkScreenWords(throwaway, expectation());
      expect(throwaway).not.toBe(markup);
      expect(report.missing, `dropping ${JSON.stringify(named)} must be caught`).toContain(named);
      expect(report.ok).toBe(false);
      console.log(`TASK0851 mutant_missing=${JSON.stringify(named)} ok=${report.ok} missing=${JSON.stringify(report.missing)}`);
    }
  });

  it("fails on throwaway copies containing banned jargon or too few words", () => {
    const markup = arrangeTilesMarkup();
    for (const banned of ["payload", "provider adapters"] as const) {
      const throwaway = markup.replace("Arrange tiles</h1>", `Arrange tiles</h1><p>${banned}</p>`);
      const report = checkScreenWords(throwaway, expectation());
      expect(report.banned.map((hit) => hit.term.toLowerCase())).toContain(banned);
      expect(report.ok).toBe(false);
    }

    const tooShort = "<h1>Arrange tiles</h1><p>Home Move up Move down Hide Done</p>";
    const report = checkScreenWords(tooShort, expectation());
    expect(report.missing).toEqual([]);
    expect(report.wordCount).toBeLessThan(LEAST_WORDS);
    expect(report.ok).toBe(false);
  });
});
