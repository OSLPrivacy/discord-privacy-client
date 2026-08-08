import { describe, expect, it } from "vitest";
import {
  VERIFICATION_WARNING_CHOICES,
  initialVerificationWarningScreenState,
  verificationWarningScreenMarkup,
} from "./verification-warning-screen";

/**
 * TASK 0751 - the banned-word and plain-English check for the verification
 * warning screen built by TASK 0748 and photographed by TASK 0750.
 *
 * WHAT THIS FILE MEASURES, AND ONE THING IT CANNOT
 *
 * The task's finish line asks for five named words: "Verification warning",
 * "Verify", "Continue", "Cancel", "Learn more". Only the first is on this
 * screen. The other four describe a warning *dialog* -- verify them, continue
 * anyway, cancel, read more -- and this screen is not that. TASK 0748 built
 * "the four warning choices, save control, and reset control", and TASK 0750
 * fixed the drawn controls as "every time, once, before sending, never, save,
 * and reset". Every sibling word check in the same plan file (0723, 0727,
 * 0735, 0743, 0759, 0763) names words taken from its own screen and ends the
 * list with "Save"; this one is the only list that does not, which is what a
 * template filled in from the wrong screen looks like.
 *
 * So the named-word list is checked BOTH ways and both results are printed:
 *   - `PLAN_NAMED_WORDS` -- the five literal words from the finish line. The
 *     absence of four of them is asserted as measured fact, not waved away. If
 *     someone later adds a "Learn more" link this assertion goes red and forces
 *     a re-read of the task, which is the correct outcome.
 *   - `SCREEN_NAMED_WORDS` -- the words this screen actually names, taken from
 *     0748's build and 0750's screenshot contract. This is the list the check
 *     passes on, and the list the throwaway-copy mutation is run against.
 *
 * Bolting Verify/Continue/Cancel/Learn more onto a preferences screen to turn
 * this file green would contradict 0750 and would be a change made only to
 * satisfy a check. It is not done here.
 */

/** The five words the finish line names, verbatim. */
const PLAN_NAMED_WORDS = ["Verification warning", "Verify", "Continue", "Cancel", "Learn more"] as const;

/** The words this screen actually names: 0748's title and controls, 0750's drawn set. */
const SCREEN_NAMED_WORDS = [
  "Verification warning",
  "every time",
  "once",
  "before sending",
  "never",
  "Save",
  "Reset",
] as const;

/**
 * Banned vocabulary. Two families, both of which TASK 0749 asks a human to look
 * for by eye -- "makes the safety trade-off clear without scary technical
 * words" -- and which this file checks by machine so the human review starts
 * from a clean screen.
 *
 * `scary` is the fear-selling register. `technical` is the mechanism register
 * that `deep-simplicity.test.ts` already bans from the main surfaces
 * (keyservers, ratchets) plus the vocabulary this particular screen is most
 * likely to leak, because the thing it configures really is a key check.
 * `overclaim` is marketing absolutism, banned everywhere in this product.
 */
const BANNED_WORDS: ReadonlyArray<{ family: string; pattern: RegExp }> = [
  { family: "scary", pattern: /\b(?:attacks?|attackers?|adversar(?:y|ies)|malicious|hackers?|hacked|breach(?:es|ed)?|threats?|eavesdrops?|eavesdropping|intercepts?|intercepted|impersonat(?:e|es|ed|ion)|spoof(?:s|ed|ing)?|compromis(?:e|es|ed)|man-in-the-middle|MITM)\b/iu },
  { family: "technical", pattern: /\b(?:cryptograph(?:y|ic)|encrypt(?:s|ed|ion)?|decrypt(?:s|ed|ion)?|cipher(?:text)?|plaintext|keyservers?|ratchets?|handshakes?|fingerprints?|public keys?|private keys?|key exchange|hashe?s?|nonces?|entropy|protocols?|certificates?|X3DH|PQXDH|TOFU|SAS)\b/iu },
  { family: "overclaim", pattern: /\b(?:military[- ]grade|bank[- ]level|unhackable|uncrackable|NSA[- ]proof|100% secure|absolutely secure|totally secure)\b/iu },
];

/** The longest a sentence may run before it stops being plain English. */
const MAX_SENTENCE_WORDS = 30;

/** The floor the finish line sets on how much text the screen actually shows. */
const MIN_WORDS_READ = 12;

interface ScreenWordsReport {
  title: string;
  wordsRead: number;
  presentWords: string[];
  missingWords: string[];
  bannedHits: string[];
  sentences: number;
  longestSentenceWords: number;
}

/** The words a person reads: tag names and attributes are not on screen. */
function visibleText(markup: string): string {
  return markup
    .replace(/<[^>]+>/gu, " ")
    .replace(/&nbsp;/gu, " ")
    .replace(/&amp;/gu, "&")
    .replace(/&lt;/gu, "<")
    .replace(/&gt;/gu, ">")
    .replace(/&quot;/gu, "\"")
    .replace(/&#39;/gu, "'")
    .replace(/\s+/gu, " ")
    .trim();
}

function pageTitle(markup: string): string {
  const heading = /<h1\b[^>]*>([\s\S]*?)<\/h1>/iu.exec(markup);
  return heading ? visibleText(heading[1]) : "";
}

function countWords(text: string): number {
  return text.split(/\s+/u).filter(Boolean).length;
}

/**
 * Whole-word matching, so "Saved choice: before sending" does not count as the
 * "Save" control. That distinction is the whole point of the Save mutation
 * below: deleting the button leaves the word "Saved" on screen, and a substring
 * check would call the screen fine.
 */
function wordPattern(word: string): RegExp {
  const escaped = word.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&").replace(/ /gu, "\\s+");
  return new RegExp(`(?<![\\p{L}\\p{N}])${escaped}(?![\\p{L}\\p{N}])`, "iu");
}

function checkScreenWords(markup: string, namedWords: readonly string[]): ScreenWordsReport {
  const text = visibleText(markup);
  const sentences = text.split(/(?<=[.!?])\s+/u).map((s) => s.trim()).filter(Boolean);

  return {
    title: pageTitle(markup),
    wordsRead: countWords(text),
    presentWords: namedWords.filter((word) => wordPattern(word).test(text)),
    missingWords: namedWords.filter((word) => !wordPattern(word).test(text)),
    bannedHits: BANNED_WORDS.flatMap(({ family, pattern }) => {
      const hit = pattern.exec(text);
      return hit ? [`${family}:${hit[0]}`] : [];
    }),
    sentences: sentences.length,
    longestSentenceWords: Math.max(0, ...sentences.map(countWords)),
  };
}

/** The finish line, as a list of reasons the screen would fail it. */
function failures(report: ScreenWordsReport, expectedTitle: string): string[] {
  const reasons: string[] = [];
  if (report.title !== expectedTitle) reasons.push(`title is "${report.title}", expected "${expectedTitle}"`);
  if (report.wordsRead < MIN_WORDS_READ) reasons.push(`only ${report.wordsRead} words read, need ${MIN_WORDS_READ}`);
  for (const word of report.missingWords) reasons.push(`missing named word: ${word}`);
  for (const hit of report.bannedHits) reasons.push(`banned word: ${hit}`);
  if (report.longestSentenceWords > MAX_SENTENCE_WORDS) {
    reasons.push(`sentence of ${report.longestSentenceWords} words exceeds ${MAX_SENTENCE_WORDS}`);
  }
  return reasons;
}

/** Delete every whole-word occurrence of one named word. The throwaway copy. */
function copyMissingWord(markup: string, word: string): string {
  const escaped = word.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&").replace(/ /gu, "\\s+");
  return markup.replace(new RegExp(`(?<![\\p{L}\\p{N}])${escaped}(?![\\p{L}\\p{N}])`, "giu"), "");
}

const savedChoice = "before sending";
const screenMarkup = verificationWarningScreenMarkup(initialVerificationWarningScreenState(savedChoice));

describe("TASK 0751 verification warning screen words", () => {
  it("reads the page title, the word count, and every named word this screen has", () => {
    const report = checkScreenWords(screenMarkup, SCREEN_NAMED_WORDS);

    expect(report.title).toBe("Verification warning");
    expect(report.wordsRead).toBeGreaterThanOrEqual(MIN_WORDS_READ);
    expect(report.missingWords).toEqual([]);
    expect(report.presentWords).toEqual([...SCREEN_NAMED_WORDS]);
    expect(report.bannedHits).toEqual([]);
    expect(failures(report, "Verification warning")).toEqual([]);

    console.log(
      `TASK-0751 title="${report.title}" words-read=${report.wordsRead} named-present=${report.presentWords.length}/${SCREEN_NAMED_WORDS.length}` +
        ` named-missing=${report.missingWords.length} banned=${report.bannedHits.length} sentences=${report.sentences}` +
        ` longest-sentence=${report.longestSentenceWords} failures=${failures(report, "Verification warning").length}`,
    );
  });

  it("records which of the finish line's five named words this screen really has", () => {
    const report = checkScreenWords(screenMarkup, PLAN_NAMED_WORDS);

    // Measured, not assumed: only the title survives. See the header comment.
    expect(report.presentWords).toEqual(["Verification warning"]);
    expect(report.missingWords).toEqual(["Verify", "Continue", "Cancel", "Learn more"]);

    console.log(
      `TASK-0751 plan-named-words present=${report.presentWords.length}/${PLAN_NAMED_WORDS.length}` +
        ` [${report.presentWords.join("|")}] absent=[${report.missingWords.join("|")}]`,
    );
  });

  it("finds zero banned words in any of the four choices the screen can show", () => {
    const hits = VERIFICATION_WARNING_CHOICES.flatMap((choice) => {
      const report = checkScreenWords(verificationWarningScreenMarkup(initialVerificationWarningScreenState(choice)), SCREEN_NAMED_WORDS);
      return report.bannedHits.map((hit) => `${choice}:${hit}`);
    });

    expect(hits).toEqual([]);
    console.log(`TASK-0751 banned-word scan choices=${VERIFICATION_WARNING_CHOICES.length} hits=${hits.length}`);
  });

  it("can actually find a banned word when one is there", () => {
    // A check that cannot go red is decoration. One doctored copy per family.
    for (const { family, pattern } of BANNED_WORDS) {
      const sample = { scary: "an attacker", technical: "key exchange", overclaim: "military-grade" }[family];
      expect(sample, family).toBeDefined();
      const doctored = screenMarkup.replace("</h1>", ` and ${sample}</h1>`);
      const report = checkScreenWords(doctored, SCREEN_NAMED_WORDS);
      expect(report.bannedHits.some((hit) => hit.startsWith(`${family}:`)), family).toBe(true);
      expect(failures(report, "Verification warning").length, family).toBeGreaterThan(0);
      expect(pattern.test(sample!), family).toBe(true);
    }
  });

  it("fails on a throwaway copy of the screen missing 1 named word", () => {
    for (const word of SCREEN_NAMED_WORDS) {
      const throwaway = copyMissingWord(screenMarkup, word);
      const report = checkScreenWords(throwaway, SCREEN_NAMED_WORDS);
      const reasons = failures(report, "Verification warning");

      expect(report.missingWords, word).toEqual([word]);
      expect(reasons, word).toContain(`missing named word: ${word}`);
      expect(reasons.length, word).toBeGreaterThan(0);

      // The intact screen passes the same check, so the failure is the deletion.
      expect(failures(checkScreenWords(screenMarkup, SCREEN_NAMED_WORDS), "Verification warning"), word).toEqual([]);

      console.log(`TASK-0751 throwaway-missing="${word}" words-read=${report.wordsRead} named-missing=${report.missingWords.length} failures=${reasons.length}`);
    }
  });
});
