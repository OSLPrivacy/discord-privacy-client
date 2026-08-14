import { describe, expect, it } from "vitest";
import { homeProtectionPanelMarkup } from "./home-protection-panel";
import { checkScreenWords } from "./screen-words";

const REQUIRED_WORDS = ["Protection", "Message protection", "Verification", "Review settings", "Learn more"] as const;
const DATA = {
  protection: "Protection",
  messageProtection: "Messages are protected",
  verification: "Verification is required before sending",
  reviewSettings: "Review settings",
  learnMore: "Learn more",
} as const;

function report(markup = homeProtectionPanelMarkup(DATA)) {
  return checkScreenWords(markup, { title: "Protection", requiredWords: REQUIRED_WORDS });
}

describe("Home protection panel words", () => {
  it("has the plain-English title and named controls", () => {
    const result = report();
    console.log(`TASK0827_TITLE=${result.title}`);
    console.log(`TASK0827_WORDS_READ=${result.wordCount}`);
    console.log(`TASK0827_PRESENT=${result.presentWords.join(",")}`);
    console.log(`TASK0827_MISSING=${result.missingWords.join(",") || "(none)"}`);
    console.log(`TASK0827_BANNED_CHECKED=${19}`);
    console.log(`TASK0827_BANNED_FOUND=${result.bannedWords.join(",") || "(none)"}`);
    expect(result.title).toBe("Protection");
    expect(result.wordCount).toBeGreaterThanOrEqual(12);
    expect(result.presentWords).toEqual([...REQUIRED_WORDS]);
    expect(result.bannedWords).toEqual([]);
  });

  it("goes red when any one named word is removed from a throwaway copy", () => {
    for (const word of REQUIRED_WORDS) {
      const broken = homeProtectionPanelMarkup(DATA).split(word).join("");
      const result = report(broken);
      console.log(`TASK0827_BREAK word=${word} pass=${result.missingWords.length === 0 && result.bannedWords.length === 0} missing=${result.missingWords.join(",")}`);
      expect(result.missingWords, `dropping ${word} must be caught`).toEqual([word]);
      expect(result.bannedWords).toEqual([]);
    }
  });
});
