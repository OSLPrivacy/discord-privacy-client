import { describe, expect, it } from "vitest";
import { builtInFriendPage, friendPageMarkup } from "./friend-page";
import { checkFriendPageWords, friendPageRequiredWords } from "./friend-page-words";
import { bannedWords, visibleWords } from "./settings-home-words";

// TASK 0843 — check the words produced by the shipped Friend page renderer.

describe("Friend page words", () => {
  const markup = friendPageMarkup(builtInFriendPage);

  it("has the right title, reads enough words, shows every named word, and has no banned words", () => {
    const report = checkFriendPageWords(markup);

    console.info(`TASK0843_TITLE=${report.title}`);
    console.info(`TASK0843_WORDS_READ=${report.wordCount}`);
    console.info(`TASK0843_PRESENT=${friendPageRequiredWords.filter((word) => !report.missingWords.includes(word)).join(",")}`);
    console.info(`TASK0843_MISSING=${report.missingWords.join(",") || "(none)"}`);
    console.info(`TASK0843_BANNED_CHECKED=${bannedWords.length}`);
    console.info(`TASK0843_BANNED_FOUND=${report.bannedFound.map((rule) => rule.banned).join(",") || "(none)"}`);

    expect(report.title).toBe("Friend");
    expect(visibleWords(markup).length).toBeGreaterThanOrEqual(12);
    expect(report.missingWords).toEqual([]);
    expect(report.bannedFound).toEqual([]);
    expect(report.pass).toBe(true);
  });

  it("fails on a throwaway copy missing one named word", () => {
    const missing = "Pictures";
    const throwaway = markup.replace(">Pictures<", "><");
    const report = checkFriendPageWords(throwaway);

    console.info(`TASK0843_THROWAWAY_REMOVED=${missing}`);
    console.info(`TASK0843_THROWAWAY_PASS=${report.pass}`);
    console.info(`TASK0843_THROWAWAY_MISSING=${report.missingWords.join(",")}`);

    expect(throwaway).not.toBe(markup);
    expect(report.pass).toBe(false);
    expect(report.missingWords).toEqual([missing]);
  });
});
