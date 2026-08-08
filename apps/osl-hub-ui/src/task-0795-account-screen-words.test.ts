import { describe, expect, it } from "vitest";
import { accountScreenState, renderAccountScreen } from "./account-screen";
import { ACCOUNT_SCREEN_SECRETS, ACCOUNT_SCREEN_SETTINGS } from "./account-screen-data";
import { accountScreenRequiredWords, checkAccountScreenWords } from "./account-screen-words";
import { pageTitle, visibleWords } from "./settings-home-words";

describe("TASK 0795 Account screen words", () => {
  const markup = renderAccountScreen(accountScreenState(ACCOUNT_SCREEN_SECRETS, ACCOUNT_SCREEN_SETTINGS));

  it("reports the required title, readable words, labels, and no banned words", () => {
    const result = checkAccountScreenWords(markup);
    console.info(`TASK0795_TITLE=${result.title}`);
    console.info(`TASK0795_WORDS_READ=${result.wordCount}`);
    console.info(`TASK0795_REQUIRED=${accountScreenRequiredWords.join("|")}`);
    console.info(`TASK0795_BANNED=${result.bannedFound.map((rule) => rule.banned).join("|") || "none"}`);
    expect(pageTitle(markup)).toBe("Account");
    expect(visibleWords(markup).length).toBeGreaterThanOrEqual(12);
    expect(result.missingWords).toEqual([]);
    expect(result.bannedFound).toEqual([]);
    expect(result.pass).toBe(true);
  });

  it("fails on a throwaway copy missing each named word", () => {
    for (const word of accountScreenRequiredWords) {
      const escaped = word.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
      const broken = markup.replace(new RegExp(`(>[^<]*?)\\b${escaped}\\b`, "gu"), "$1");
      const result = checkAccountScreenWords(broken);
      console.info(`TASK0795_MISSING_${word.replace(/\s+/gu, "_")}=pass:${!result.pass},missing:${result.missingWords.join("|")}`);
      expect(result.pass, `check must fail when "${word}" is missing`).toBe(false);
      expect(result.missingWords).toContain(word);
    }
  });
});
