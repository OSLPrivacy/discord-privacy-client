import { describe, expect, it } from "vitest";
import { defaultWindowSoundsSettings, windowSoundsSettingsMarkup } from "./window-sounds-settings";
import { checkWindowSoundsWords, windowSoundsRequiredWords } from "./window-sounds-words";
import { pageTitle, visibleWords } from "./settings-home-words";

describe("TASK 0783 Window and sounds words", () => {
  const markup = windowSoundsSettingsMarkup(defaultWindowSoundsSettings);

  it("has Window and sounds as the page title", () => {
    expect(pageTitle(markup)).toBe("Window and sounds");
  });

  it("reads at least 12 visible words", () => {
    const words = visibleWords(markup);
    console.info(`TASK0783 wordCount=${words.length}`);
    expect(words.length).toBeGreaterThanOrEqual(12);
  });

  it("shows every named word", () => {
    const result = checkWindowSoundsWords(markup);
    expect(windowSoundsRequiredWords).toEqual([
      "Window and sounds",
      "Window size",
      "Alert sounds",
      "Message sound",
      "Save",
    ]);
    expect(result.missingWords).toEqual([]);
  });

  it("finds zero banned words", () => {
    const result = checkWindowSoundsWords(markup);
    console.info(`TASK0783 bannedWords=${result.bannedFound.length}`);
    expect(result.bannedFound).toEqual([]);
    expect(result.pass).toBe(true);
  });

  it("fails on a throwaway copy missing each named word", () => {
    for (const word of windowSoundsRequiredWords) {
      const escaped = word.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
      const broken = markup.replace(new RegExp(`(>[^<]*?)\\b${escaped}\\b`, "gu"), "$1");
      const result = checkWindowSoundsWords(broken);
      expect(result.pass, `check must fail when "${word}" is missing`).toBe(false);
      expect(result.missingWords, `must name "${word}" as missing`).toContain(word);
    }
  });
});
