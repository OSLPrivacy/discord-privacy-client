import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { BANNED_SCREEN_WORDS, checkScreenWords, sliceScreenSource } from "./screen-words";

/**
 * TASK 0727 - the banned-word and plain-English check on the Notifications
 * screen.
 *
 * The screen is `notificationSettingsContent` plus the OSL Chat block it calls,
 * both in `main.ts`. They are read as source, not imported: `main.ts` in this
 * lane carries merge damage that stops it parsing (five stacked duplicate
 * declarations), so every test here that touches a screen reads its words out
 * of the file, the same way `ui-simplicity.test.ts` and `0126` already do.
 */
const NOTIFICATIONS_SCREEN = ["notificationSettingsContent", "oslChatNotificationSettings"] as const;
const NAMED_WORDS = ["Notifications", "Messages", "Friends", "Sounds", "Save"] as const;
const LEAST_WORDS_READ = 12;

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const notificationsScreen = sliceScreenSource(mainSource, NOTIFICATIONS_SCREEN);

describe("Notifications screen words", () => {
  it("is titled Notifications, names its choices, and uses no banned word", () => {
    const report = checkScreenWords(notificationsScreen, NAMED_WORDS);

    console.log(`TASK0727_TITLE=${report.title}`);
    console.log(`TASK0727_WORDS_READ=${report.wordCount}`);
    console.log(`TASK0727_PRESENT=${report.presentWords.join(",")}`);
    console.log(`TASK0727_MISSING=${report.missingWords.join(",") || "(none)"}`);
    console.log(`TASK0727_BANNED_CHECKED=${BANNED_SCREEN_WORDS.length}`);
    console.log(`TASK0727_BANNED_FOUND=${report.bannedWords.join(",") || "(none)"}`);
    console.log(`TASK0727_TEXT=${report.text}`);

    expect(report.title).toBe("Notifications");
    expect(report.wordCount).toBeGreaterThanOrEqual(LEAST_WORDS_READ);
    expect(report.missingWords).toEqual([]);
    expect(report.presentWords).toEqual([...NAMED_WORDS]);
    expect(report.bannedWords).toEqual([]);
  });

  it("goes red on a throwaway copy of the screen missing one named word", () => {
    for (const word of NAMED_WORDS) {
      const throwaway = notificationsScreen.replaceAll(word, "");
      const broken = checkScreenWords(throwaway, NAMED_WORDS);

      expect(broken.missingWords, `dropping ${word} must be caught`).toEqual([word]);
      expect(broken.presentWords).not.toContain(word);
      if (word === "Notifications") expect(broken.title).not.toBe("Notifications");
    }
  });

  it("goes red on a throwaway copy of the screen carrying a banned word", () => {
    for (const banned of BANNED_SCREEN_WORDS) {
      const throwaway = notificationsScreen.replace("<h2>Notifications</h2>", `<h2>Notifications</h2><p>Every ${banned} is shown here.</p>`);
      const broken = checkScreenWords(throwaway, NAMED_WORDS);

      expect(broken.bannedWords, `${banned} must be caught in screen copy`).toContain(banned);
    }
  });

  it("reads copy a person sees and skips the machinery a person does not", () => {
    const report = checkScreenWords(notificationsScreen, NAMED_WORDS);

    // Copy that only appears on one branch is still read.
    expect(report.text).toContain("Nothing new");
    expect(report.text).toContain("Notifications are off");
    // Attribute values inside a tag are not screen words.
    expect(report.words).not.toContain("checkbox");
    expect(report.words).not.toContain("checked");
    expect(report.words).not.toContain("aria");
  });
});
