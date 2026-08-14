import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { appsAndSendingScreenMarkup, type AppsAndSendingModel } from "./apps-and-sending-screen";
import {
  PLAIN_ENGLISH_BANNED_WORDS,
  checkPlainEnglishScreenWords,
  findPlainEnglishBannedWords,
  screenTitle,
  visibleWords,
  type ScreenWordsExpectation,
} from "./screen-words";

const REPO = path.join(import.meta.dirname, "..", "..", "..");

const FIXTURE = path.join(
  import.meta.dirname,
  "..",
  "screenshots",
  "fixtures",
  "task-0760-apps-and-sending.json",
);

const DESIGN_FEEL_DOC = path.join(REPO, "docs", "design", "osl-subjective-design-feel.md");

/** The words TASK 0763 names. Each must be readable on the screen. */
const NAMED_WORDS = ["Apps and sending", "Connected apps", "Send messages", "Remove app", "Save"] as const;

const LEAST_WORDS = 12;

function savedModel(): AppsAndSendingModel {
  return JSON.parse(readFileSync(FIXTURE, "utf8")) as AppsAndSendingModel;
}

/**
 * The banned concepts the shipped product contract declares, read from the doc
 * the Rust design-feel test reads. Copying the list into this file would let
 * the two drift apart silently, and this check would then pass on a screen the
 * contract bans.
 */
function contractBannedConcepts(): string[] {
  const doc = readFileSync(DESIGN_FEEL_DOC, "utf8");
  const fence = /```json\s*([\s\S]*?)```/u.exec(doc);
  if (fence === null) throw new Error("no JSON fixture block in the design-feel doc");
  const fixtures = JSON.parse(fence[1]) as { banned_user_facing_concepts?: string[] };
  const banned = fixtures.banned_user_facing_concepts ?? [];
  if (banned.length === 0) throw new Error("design-feel doc declares no banned concepts");
  return banned;
}

function expectation(): ScreenWordsExpectation {
  return {
    title: "Apps and sending",
    requiredWords: NAMED_WORDS,
    leastWords: LEAST_WORDS,
    bannedWords: contractBannedConcepts(),
  };
}

describe("TASK 0763 Apps and sending screen words", () => {
  it("reads the screen: right title, enough words, every named word, zero banned words", () => {
    const markup = appsAndSendingScreenMarkup(savedModel());
    const report = checkPlainEnglishScreenWords(markup, expectation());

    console.log(
      `TASK 0763 report: title=${JSON.stringify(report.title)} words=${report.wordCount} ` +
        `present=${report.present.length}/${NAMED_WORDS.length} missing=${JSON.stringify(report.missing)} ` +
        `banned=${report.banned.length} ${JSON.stringify(report.banned)} ` +
        `bannedTermsChecked=${PLAIN_ENGLISH_BANNED_WORDS.length + contractBannedConcepts().length}`,
    );

    expect(report.title).toBe("Apps and sending");
    expect(screenTitle(markup)).toBe("Apps and sending");
    expect(report.wordCount).toBeGreaterThanOrEqual(LEAST_WORDS);
    expect(report.enoughWords).toBe(true);
    expect(report.missing).toEqual([]);
    expect(report.present).toEqual([...NAMED_WORDS]);
    expect(report.banned).toEqual([]);
    expect(report.ok).toBe(true);
  });

  it("reads the words a person sees, not the markup around them", () => {
    const markup = appsAndSendingScreenMarkup(savedModel());
    const words = visibleWords(markup);

    // Class names, data attributes and aria labels are markup, not reading.
    expect(words).not.toContain("apps-sending-title");
    expect(words).not.toContain("aria-labelledby");
    expect(words).toContain("Apps");
    expect(words).toContain("Save");
    // A word only reachable through an attribute must not count as present.
    const attributeOnly = '<section aria-label="Remove app"><h1>Apps and sending</h1></section>';
    expect(checkPlainEnglishScreenWords(attributeOnly, expectation()).missing).toContain("Remove app");

    // The save note ends "after you save." -- prose is not the Save button.
    const noteOnly = "<h1>Apps and sending</h1><p>Changes apply to new messages after you save.</p>";
    expect(checkPlainEnglishScreenWords(noteOnly, expectation()).missing).toContain("Save");
  });

  it("fails on a throwaway copy of the screen that is missing one named word", () => {
    const markup = appsAndSendingScreenMarkup(savedModel());

    for (const word of NAMED_WORDS) {
      // A throwaway string copy -- the screen module is not touched.
      const broken = markup.split(word).join("");
      const report = checkPlainEnglishScreenWords(broken, expectation());

      expect(broken).not.toBe(markup);
      expect(report.missing).toEqual([word]);
      expect(report.present).toEqual(NAMED_WORDS.filter((named) => named !== word));
      expect(report.ok).toBe(false);
      console.log(`TASK 0763 mutant: dropped ${JSON.stringify(word)} -> ok=${report.ok} missing=${JSON.stringify(report.missing)}`);
    }
  });

  it("fails on a throwaway copy that says a banned word, and on one too short to read", () => {
    const markup = appsAndSendingScreenMarkup(savedModel());

    // One from the plain-English list, one from the shipped product contract.
    for (const banned of ["payload", "provider adapters"]) {
      const jargon = markup.replace(
        "Send messages",
        `Send messages by ${banned}`,
      );
      const report = checkPlainEnglishScreenWords(jargon, expectation());
      expect(report.banned.map((hit) => hit.term.toLowerCase())).toContain(banned.toLowerCase());
      expect(report.ok).toBe(false);
      console.log(`TASK 0763 mutant: said ${JSON.stringify(banned)} -> ok=${report.ok} banned=${JSON.stringify(report.banned)}`);
    }

    // Plural and singular both trip the ban, so a reworded screen cannot slip through.
    expect(findPlainEnglishBannedWords("<p>one keyserver</p>", contractBannedConcepts())).toHaveLength(1);
    expect(findPlainEnglishBannedWords("<p>two keyservers</p>", contractBannedConcepts())).toHaveLength(1);

    const tooShort = "<h1>Apps and sending</h1><p>Connected apps Send messages Remove app Save</p>";
    const shortReport = checkPlainEnglishScreenWords(tooShort, expectation());
    expect(shortReport.missing).toEqual([]);
    expect(shortReport.wordCount).toBeLessThan(LEAST_WORDS);
    expect(shortReport.enoughWords).toBe(false);
    expect(shortReport.ok).toBe(false);
    console.log(`TASK 0763 mutant: short screen -> words=${shortReport.wordCount} ok=${shortReport.ok}`);
  });
});
