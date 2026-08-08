import { describe, expect, it } from "vitest";
import {
  initialMessageDefaultsScreenState,
  messageDefaultsScreenMarkup,
} from "./message-defaults";
import {
  checkMessageDefaultsWords,
  messageDefaultsRequiredWords,
} from "./message-defaults-words";
import { visibleWords, pageTitle } from "./settings-home-words";

// TASK 0759 — check Message defaults screen words. The markup under test is
// the shipped screen function, so the words being judged are the words the
// app renders.

describe("Message defaults screen words", () => {
  const markup = messageDefaultsScreenMarkup(initialMessageDefaultsScreenState());

  it("has Message defaults as the page title", () => {
    expect(pageTitle(markup)).toBe("Message defaults");
  });

  it("reads at least 12 visible words", () => {
    const words = visibleWords(markup);
    console.info(`TASK0759 wordCount=${words.length}`);
    expect(words.length).toBeGreaterThanOrEqual(12);
  });

  it("shows Message defaults, Protected messages, Burn after reading, Read receipts, and Save", () => {
    const result = checkMessageDefaultsWords(markup);
    expect(messageDefaultsRequiredWords).toEqual([
      "Message defaults",
      "Protected messages",
      "Burn after reading",
      "Read receipts",
      "Save",
    ]);
    expect(result.missingWords).toEqual([]);
  });

  it("finds zero banned words", () => {
    const result = checkMessageDefaultsWords(markup);
    expect(result.bannedFound).toEqual([]);
    expect(result.pass).toBe(true);
  });

  it("fails on a throwaway copy of the screen missing any one named word", () => {
    for (const word of messageDefaultsRequiredWords) {
      // A throwaway copy with every visible occurrence of this word blanked
      // out. Word-boundary and case-sensitive: labels, not substrings, so the
      // "Saved" tags on screen must not stand in for a missing "Save" button.
      const escaped = word.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
      const broken = markup.replace(new RegExp(`(>[^<]*?)\\b${escaped}\\b`, "gu"), "$1");
      const result = checkMessageDefaultsWords(broken);
      expect(result.pass, `check must fail when "${word}" is missing`).toBe(false);
      if (word === "Message defaults") {
        // Blanking "Message defaults" empties the <h1>, so the title check names it too.
        expect(result.title).not.toBe("Message defaults");
      }
      expect(result.missingWords, `must name "${word}" as the missing word`).toContain(word);
    }
  });

  it("fails on a throwaway copy that reintroduces a banned word", () => {
    const broken = markup.replace(">Timer<", ">Timer config<");
    expect(broken).not.toBe(markup);
    const result = checkMessageDefaultsWords(broken);
    expect(result.pass).toBe(false);
    expect(result.bannedFound.map((rule) => rule.banned)).toContain("config");
    // The report carries the plain-English replacement, so a red names the fix.
    expect(result.bannedFound.find((rule) => rule.banned === "config")?.plainEnglish).toBe("settings");
  });
});
