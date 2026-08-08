import { describe, expect, it } from "vitest";
import { settingsHomePageMarkup } from "./settings-home";
import {
  bannedWords,
  checkSettingsHomeWords,
  pageTitle,
  settingsHomeRequiredWords,
  visibleWords,
} from "./settings-home-words";

// TASK 0719 — check Settings home words. The markup under test is the shipped
// screen function, so the words being judged are the words the app renders.

describe("Settings home words", () => {
  const markup = settingsHomePageMarkup();

  it("has Settings as the page title", () => {
    expect(pageTitle(markup)).toBe("Settings");
  });

  it("reads at least 12 visible words", () => {
    const words = visibleWords(markup);
    expect(words.length).toBeGreaterThanOrEqual(12);
  });

  it("shows Settings, Privacy, Notifications, Account, Apps and sending, Look, and Home", () => {
    const result = checkSettingsHomeWords(markup);
    expect(settingsHomeRequiredWords).toEqual([
      "Settings",
      "Privacy",
      "Notifications",
      "Account",
      "Apps and sending",
      "Look",
      "Home",
    ]);
    expect(result.missingWords).toEqual([]);
  });

  it("finds zero banned words", () => {
    const result = checkSettingsHomeWords(markup);
    expect(result.bannedFound).toEqual([]);
    expect(result.pass).toBe(true);
  });

  it("fails on a throwaway copy of the screen missing any one named word", () => {
    for (const word of settingsHomeRequiredWords) {
      // A throwaway copy with every visible occurrence of this word blanked
      // out. Word-boundary and case-sensitive: labels, not substrings.
      const escaped = word.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
      const broken = markup.replace(new RegExp(`(>[^<]*?)\\b${escaped}\\b`, "gu"), "$1");
      const result = checkSettingsHomeWords(broken);
      expect(result.pass, `check must fail when "${word}" is missing`).toBe(false);
      if (word === "Settings") {
        // Blanking "Settings" empties the <h1>, so the title check names it too.
        expect(result.title).not.toBe("Settings");
      }
      expect(result.missingWords, `must name "${word}" as the missing word`).toContain(word);
    }
  });

  it("fails on a throwaway copy that reintroduces a banned word", () => {
    const broken = markup.replace(">Look<", ">Appearance<");
    const result = checkSettingsHomeWords(broken);
    expect(result.pass).toBe(false);
    expect(result.bannedFound.map((rule) => rule.banned)).toContain("appearance");
    // The report carries the plain-English replacement, so a red names the fix.
    expect(result.bannedFound.find((rule) => rule.banned === "appearance")?.plainEnglish).toBe("Look");
  });

  it("keeps every banned word paired with a non-empty plain-English replacement", () => {
    for (const rule of bannedWords) {
      expect(rule.banned.length).toBeGreaterThan(0);
      expect(rule.plainEnglish.length).toBeGreaterThan(0);
      expect(rule.banned.toLowerCase()).not.toBe(rule.plainEnglish.toLowerCase());
    }
  });
});
