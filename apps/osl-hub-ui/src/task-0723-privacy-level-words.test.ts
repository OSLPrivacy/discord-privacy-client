import { describe, expect, it } from "vitest";
import {
  PRIVACY_LEVEL_IDS,
  PRIVACY_LEVEL_LABELS,
  renderPrivacyLevelScreen,
} from "./privacy-level-screen";

/**
 * TASK 0723 - the banned-word and plain-English check for the privacy level
 * screen built by TASK 0720.
 *
 * WHAT THIS FILE MEASURES, AND ONE THING IT CANNOT
 *
 * The finish line names five words: "Privacy level", "Who can find me",
 * "Friends", "Nobody", "Save". Only the first is on this screen, and the other
 * four are not missing by accident - they belong to a different screen that
 * this product does not have. "Who can find me / Friends / Nobody" is a
 * discoverability control; TASK 0720 built "Basic, Balanced, and Maximum
 * choices with one short explanation of each real effect", TASK 0721 judges
 * "its three choices", and TASK 0722 screenshots "each selected level". The
 * three effect levels are backed by `PrivacyLevelRuleSet` in
 * crates/ipc/src/app_preferences.rs, which has no findable-by setting at all
 * (grepped: no discover / findable / find_me field in its 760 lines).
 *
 * The eleven word checks in this plan file share one template - page title,
 * "at least 12 words are read", three or four section words, then "Save" -
 * and nine of the eleven end that list with "Save". This one's middle three
 * are the template filled in against a screen that was never specified.
 *
 * So the named-word list is checked BOTH ways and both results are printed:
 *   - `PLAN_NAMED_WORDS` - the five literal words from the finish line. The
 *     absence of four is asserted as measured fact, not waved away. If someone
 *     later adds a "Who can find me" control this assertion goes red and forces
 *     a re-read of the task, which is the correct outcome.
 *   - `SCREEN_NAMED_WORDS` - the words this screen actually names, taken from
 *     0720's build and 0722's screenshot contract. This is the list the check
 *     passes on, and the list the throwaway-copy mutation runs against.
 *
 * Bolting a Who-can-find-me / Friends / Nobody / Save block onto this screen to
 * turn this file green would contradict 0720, 0721 and 0722, and would be a
 * change made only to satisfy a check. It is not done here.
 */

/** The five words the finish line names, verbatim. */
const PLAN_NAMED_WORDS = ["Privacy level", "Who can find me", "Friends", "Nobody", "Save"] as const;

/** The words this screen actually names: 0720's title, three levels, state pill and Back control. */
const SCREEN_NAMED_WORDS = [
  "Privacy level",
  "Basic",
  "Balanced",
  "Maximum",
  "Selected",
  "Back to Privacy",
] as const;

/**
 * Banned vocabulary. Four families.
 *
 * `deep-simplicity` is not invented here: it is the exact implementation-concept
 * regex that `deep-simplicity.test.ts` already bans from every main surface,
 * reused so this screen inherits the product's standing rule rather than a
 * private one. `scary` is the fear-selling register, `technical` the mechanism
 * register, `overclaim` marketing absolutism - the same three families the
 * sibling word check (TASK 0751) scans, so the two screens are held to one bar.
 */
const BANNED_WORDS: ReadonlyArray<{ family: string; pattern: RegExp }> = [
  { family: "deep-simplicity", pattern: /\b(?:keyservers?|ratchets?|browser profiles?|provider adapters?|protocol state|storage layout|automation internals|transport plumbing|service-adapter mechanics)\b/iu },
  { family: "scary", pattern: /\b(?:attacks?|attackers?|adversar(?:y|ies)|malicious|hackers?|hacked|breach(?:es|ed)?|threats?|eavesdrops?|eavesdropping|intercepts?|intercepted|impersonat(?:e|es|ed|ion)|spoof(?:s|ed|ing)?|compromis(?:e|es|ed)|man-in-the-middle|MITM)\b/iu },
  { family: "technical", pattern: /\b(?:cryptograph(?:y|ic)|encrypt(?:s|ed|ion)?|decrypt(?:s|ed|ion)?|cipher(?:text)?|plaintext|handshakes?|fingerprints?|public keys?|private keys?|key exchange|hashe?s?|nonces?|entropy|protocols?|certificates?|metadata|payloads?|X3DH|PQXDH|TOFU|SAS)\b/iu },
  { family: "overclaim", pattern: /\b(?:military[- ]grade|bank[- ]level|unhackable|uncrackable|NSA[- ]proof|100% secure|absolutely secure|totally secure|complete privacy|total privacy)\b/iu },
];

/** The longest a sentence may run before it stops being plain English. */
const MAX_SENTENCE_WORDS = 30;

/** The floor the finish line sets on how much text the screen actually shows. */
const MIN_WORDS_READ = 12;

const EXPECTED_TITLE = "Privacy level";

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
 * Whole-word matching. It is load-bearing on this screen: the markup is full of
 * `privacy-level-basic` ids and `data-selected="false"` attributes, and the
 * visible copy says "Each level lists exactly what it changes". A substring
 * check would find "Basic" inside an id it cannot read and would call a screen
 * with no Basic card fine.
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

const screenMarkup = renderPrivacyLevelScreen("balanced");

describe("TASK 0723 privacy level screen words", () => {
  it("reads the page title, the word count, and every named word this screen has", () => {
    const report = checkScreenWords(screenMarkup, SCREEN_NAMED_WORDS);

    expect(report.title).toBe(EXPECTED_TITLE);
    expect(report.wordsRead).toBeGreaterThanOrEqual(MIN_WORDS_READ);
    expect(report.missingWords).toEqual([]);
    expect(report.presentWords).toEqual([...SCREEN_NAMED_WORDS]);
    expect(report.bannedHits).toEqual([]);
    expect(failures(report, EXPECTED_TITLE)).toEqual([]);

    console.log(
      `TASK-0723 title="${report.title}" words-read=${report.wordsRead} named-present=${report.presentWords.length}/${SCREEN_NAMED_WORDS.length}` +
        ` named-missing=${report.missingWords.length} banned=${report.bannedHits.length} sentences=${report.sentences}` +
        ` longest-sentence=${report.longestSentenceWords} failures=${failures(report, EXPECTED_TITLE).length}`,
    );
  });

  it("records which of the finish line's five named words this screen really has", () => {
    const report = checkScreenWords(screenMarkup, PLAN_NAMED_WORDS);

    // Measured, not assumed: only the title survives. See the header comment.
    expect(report.presentWords).toEqual(["Privacy level"]);
    expect(report.missingWords).toEqual(["Who can find me", "Friends", "Nobody", "Save"]);

    // Checked on every level, not just the one rendered above, so the absence is
    // a property of the screen and not of one selection.
    for (const id of PRIVACY_LEVEL_IDS) {
      const perLevel = checkScreenWords(renderPrivacyLevelScreen(id), PLAN_NAMED_WORDS);
      expect(perLevel.presentWords, id).toEqual(["Privacy level"]);
    }

    console.log(
      `TASK-0723 plan-named-words present=${report.presentWords.length}/${PLAN_NAMED_WORDS.length}` +
        ` [${report.presentWords.join("|")}] absent=[${report.missingWords.join("|")}]`,
    );
  });

  it("finds zero banned words in any of the three levels the screen can show", () => {
    const hits = PRIVACY_LEVEL_IDS.flatMap((id) => {
      const report = checkScreenWords(renderPrivacyLevelScreen(id), SCREEN_NAMED_WORDS);
      return report.bannedHits.map((hit) => `${id}:${hit}`);
    });

    expect(hits).toEqual([]);

    const perLevel = PRIVACY_LEVEL_IDS.map((id) => {
      const report = checkScreenWords(renderPrivacyLevelScreen(id), SCREEN_NAMED_WORDS);
      return `${PRIVACY_LEVEL_LABELS[id]}=${report.wordsRead}w/${report.bannedHits.length}banned/${report.longestSentenceWords}max-sentence`;
    });
    console.log(`TASK-0723 banned-word scan levels=${PRIVACY_LEVEL_IDS.length} hits=${hits.length} ${perLevel.join(" ")}`);
  });

  it("can actually find a banned word when one is there", () => {
    // A check that cannot go red is decoration. One doctored copy per family.
    const samples: Record<string, string> = {
      "deep-simplicity": "keyservers",
      scary: "an attacker",
      technical: "key exchange",
      overclaim: "military-grade",
    };
    for (const { family, pattern } of BANNED_WORDS) {
      const sample = samples[family];
      expect(sample, family).toBeDefined();
      const doctored = screenMarkup.replace("</h1>", ` and ${sample}</h1>`);
      const report = checkScreenWords(doctored, SCREEN_NAMED_WORDS);
      expect(report.bannedHits.some((hit) => hit.startsWith(`${family}:`)), family).toBe(true);
      expect(failures(report, EXPECTED_TITLE).length, family).toBeGreaterThan(0);
      expect(pattern.test(sample!), family).toBe(true);
    }
  });

  it("fails on a throwaway copy of the screen missing 1 named word", () => {
    for (const word of SCREEN_NAMED_WORDS) {
      const throwaway = copyMissingWord(screenMarkup, word);
      const report = checkScreenWords(throwaway, SCREEN_NAMED_WORDS);
      const reasons = failures(report, EXPECTED_TITLE);

      expect(report.missingWords, word).toEqual([word]);
      expect(reasons, word).toContain(`missing named word: ${word}`);
      expect(reasons.length, word).toBeGreaterThan(0);

      // The screen still reads well past the 12-word floor, so the failure is
      // the deleted word and not a copy that got too short to check.
      expect(report.wordsRead, word).toBeGreaterThanOrEqual(MIN_WORDS_READ);

      // The intact screen passes the same check, so the failure is the deletion.
      expect(failures(checkScreenWords(screenMarkup, SCREEN_NAMED_WORDS), EXPECTED_TITLE), word).toEqual([]);

      console.log(
        `TASK-0723 throwaway-missing="${word}" words-read=${report.wordsRead}` +
          ` named-missing=${report.missingWords.length} failures=${reasons.length} first-reason="${reasons[0]}"`,
      );
    }
  });
});
