import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { BANNED_SCREEN_WORDS, checkScreenWords, sliceScreenSource } from "./screen-words";

/**
 * TASK 0835 - the banned-word and plain-English check on the Friends panel.
 *
 * The panel is `friendsDialogMarkup` in `main.ts`. It is read as source, not imported:
 * `main.ts` in this lane carries merge damage that stops it parsing, so we read
 * its words out of the file directly, the same way other screen tests do.
 */
const FRIENDS_PANEL = ["friendsDialogMarkup"] as const;
const NAMED_WORDS = ["Friends", "Add friend", "Search friends", "Requests", "Blocked", "Messages"] as const;
const LEAST_WORDS_READ = 12;

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const friendsPanel = sliceScreenSource(mainSource, FRIENDS_PANEL);

describe("Friends panel words", () => {
  it("is titled Friends, names its choices, and uses no banned word", () => {
    const report = checkScreenWords(friendsPanel, NAMED_WORDS);

    console.log(`TASK0835_TITLE=${report.title}`);
    console.log(`TASK0835_WORDS_READ=${report.wordCount}`);
    console.log(`TASK0835_PRESENT=${report.presentWords.join(",")}`);
    console.log(`TASK0835_MISSING=${report.missingWords.join(",") || "(none)"}`);
    console.log(`TASK0835_BANNED_CHECKED=${BANNED_SCREEN_WORDS.length}`);
    console.log(`TASK0835_BANNED_FOUND=${report.bannedWords.join(",") || "(none)"}`);
    console.log(`TASK0835_TEXT=${report.text}`);

    expect(report.title).toBe("Friends");
    expect(report.wordCount).toBeGreaterThanOrEqual(LEAST_WORDS_READ);
    expect(report.missingWords).toEqual([]);
    expect(report.presentWords).toEqual([...NAMED_WORDS]);
    expect(report.bannedWords).toEqual([]);
  });

  it("goes red on a throwaway copy of the screen missing one named word", () => {
    for (const word of NAMED_WORDS) {
      const throwaway = friendsPanel.replaceAll(word, "");
      const broken = checkScreenWords(throwaway, NAMED_WORDS);

      expect(broken.missingWords, `dropping ${word} must be caught`).toEqual([word]);
      expect(broken.presentWords).not.toContain(word);
      if (word === "Friends") expect(broken.title).not.toBe("Friends");
    }
  });

  it("goes red on a throwaway copy of the screen carrying a banned word", () => {
    for (const banned of BANNED_SCREEN_WORDS) {
      const throwaway = friendsPanel.replace("<h2 id=\"friends-dialog-title\">Friends</h2>", `<h2 id="friends-dialog-title">Friends</h2><p>Every ${banned} is shown here.</p>`);
      const broken = checkScreenWords(throwaway, NAMED_WORDS);

      expect(broken.bannedWords, `${banned} must be caught in screen copy`).toContain(banned);
    }
  });

  it("reads copy a person sees and skips the machinery a person does not", () => {
    const report = checkScreenWords(friendsPanel, NAMED_WORDS);

    // Copy that appears on the screen is read.
    expect(report.text).toContain("Add friend");
    expect(report.text).toContain("Paste their invite");
    expect(report.text).toContain("Name them on this device");
    // Attribute values inside a tag are not screen words.
    expect(report.words).not.toContain("aria");
    expect(report.words).not.toContain("friend");
    expect(report.words).not.toContain("dialog");
  });
});
