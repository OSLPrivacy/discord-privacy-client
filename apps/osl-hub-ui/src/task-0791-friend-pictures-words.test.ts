import { describe, expect, it } from "vitest";
import { renderFriendPicturesScreen, friendPicturesScreenState } from "./friend-pictures-screen";
import {
  checkFriendPicturesWords,
  friendPicturesRequiredWords,
} from "./friend-pictures-words";
import { visibleWords, pageTitle } from "./settings-home-words";
import {
  FRIEND_PICTURES_SCREEN_FRIENDS,
  FRIEND_PICTURES_SCREEN_SETTINGS,
} from "./friend-pictures-screen-data";

// TASK 0791 — check Friend pictures screen words. The markup under test is
// the shipped screen function, so the words being judged are the words the
// app renders.

describe("Friend pictures screen words", () => {
  const state = friendPicturesScreenState(FRIEND_PICTURES_SCREEN_SETTINGS);
  const markup = renderFriendPicturesScreen(state, FRIEND_PICTURES_SCREEN_FRIENDS);

  it("has Friend pictures as the page title", () => {
    expect(pageTitle(markup)).toBe("Friend pictures");
  });

  it("reads at least 12 visible words", () => {
    const words = visibleWords(markup);
    console.info(`TASK0791 wordCount=${words.length}`);
    expect(words.length).toBeGreaterThanOrEqual(12);
  });

  it("shows Friend pictures, Own picture, Hide pictures, and Save", () => {
    const result = checkFriendPicturesWords(markup);
    expect(friendPicturesRequiredWords).toEqual([
      "Friend pictures",
      "Own picture",
      "Hide pictures",
      "Save",
    ]);
    expect(result.missingWords).toEqual([]);
  });

  it("finds zero banned words", () => {
    const result = checkFriendPicturesWords(markup);
    expect(result.bannedFound).toEqual([]);
    expect(result.pass).toBe(true);
  });

  it("fails on a throwaway copy of the screen missing any one named word", () => {
    for (const word of friendPicturesRequiredWords) {
      // A throwaway copy with every visible occurrence of this word blanked
      // out. Word-boundary and case-sensitive: labels, not substrings, so the
      // "Saved" tags on screen must not stand in for a missing "Save" button.
      const escaped = word.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
      const broken = markup.replace(new RegExp(`(>[^<]*?)\\b${escaped}\\b`, "gu"), "$1");
      const result = checkFriendPicturesWords(broken);
      expect(result.pass, `check must fail when "${word}" is missing`).toBe(false);
      if (word === "Friend pictures") {
        // Blanking "Friend pictures" empties the <h1>, so the title check names it too.
        expect(result.title).not.toBe("Friend pictures");
      }
      expect(result.missingWords, `must name "${word}" as the missing word`).toContain(word);
    }
  });

  it("fails on a throwaway copy that reintroduces a banned word", () => {
    const broken = markup.replace(">Layout<", ">Layout configuration<");
    expect(broken).not.toBe(markup);
    const result = checkFriendPicturesWords(broken);
    expect(result.pass).toBe(false);
    expect(result.bannedFound.map((rule) => rule.banned)).toContain("configuration");
    // The report carries the plain-English replacement, so a red names the fix.
    expect(result.bannedFound.find((rule) => rule.banned === "configuration")?.plainEnglish).toBe("settings");
  });
});
