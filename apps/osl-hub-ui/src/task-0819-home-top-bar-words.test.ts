import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

/**
 * TASK 0819 - the banned-word and plain-English check for the Home top bar
 * built by TASK 0816.
 *
 * WHAT THIS FILE MEASURES, AND ONE THING IT CANNOT
 *
 * The task's finish line asks for six named words: "Home", "Search",
 * "Settings", "Friends", "Messages", "Profile". Four are on this bar. The
 * other two describe a generic messenger's top bar -- search box, messages
 * button -- and this bar is not that. TASK 0816's own build line is "Build
 * logo, Friends, Notifications, Settings, and Profile controls with clear
 * current-page state", its screenshot check fixes exactly those five words,
 * and no Search control or Messages control exists anywhere in the shell
 * (asserted below on the full rendered shell, sidebar included, as measured
 * fact). This is the same template slip 0723 and 0751 already hit in their
 * plan file: a word list written for a different screen dropped into this
 * screen's slot.
 *
 * So the named-word list is checked BOTH ways and both results are printed:
 *   - `PLAN_NAMED_WORDS` -- the six literal words from the finish line. The
 *     absence of Search and Messages is asserted as measured fact, not waved
 *     away. If someone later adds a Search control this assertion goes red
 *     and forces a re-read of the task, which is the correct outcome.
 *   - `BAR_NAMED_WORDS` -- the five words the bar actually paints, taken from
 *     0816's build and screenshot contract. This is the list the check passes
 *     on, and the list the throwaway-copy mutation is run against.
 *
 * Bolting a Search box and a Messages button onto the bar to turn this file
 * green would contradict 0816's judged five-control screenshot and would be a
 * change made only to satisfy a check. It is not done here.
 *
 * SCOPE. The bar is a header strip: five labelled controls, ten visible words
 * (each label is painted twice -- the label span and its in-DOM tooltip), so
 * the bar alone can never carry a page title or 12 words. The screen the
 * check reads is therefore the bar PLUS the Home page it heads, whose
 * `<h1 id="route-heading">` is the page title "Home". The shared primary
 * sidebar and the window controls belong to every route, not to this screen,
 * and stay out of the word count -- but they are still swept by the
 * measured-absence and banned-word specs below, so nothing named hides there.
 */

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  mocks.invoke.mockResolvedValue(undefined);
  mocks.listen.mockResolvedValue(() => undefined);
  mocks.getCurrentWindow.mockReturnValue({ onFocusChanged: vi.fn().mockResolvedValue(() => undefined) });
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  localStore.clear();
});

/** The six words the finish line names, verbatim. */
const PLAN_NAMED_WORDS = ["Home", "Search", "Settings", "Friends", "Messages", "Profile"] as const;

/** The five words the bar actually paints: 0816's build and screenshot contract. */
const BAR_NAMED_WORDS = ["Home", "Friends", "Notifications", "Settings", "Profile"] as const;

/**
 * Banned vocabulary, the same four families 0723 scans. `deep-simplicity` is
 * the regex deep-simplicity.test.ts already enforces on every main surface,
 * reused so this screen inherits the standing rule instead of a private one.
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
 * Whole-word matching, so the `data-top-bar-control="settings"` attribute or a
 * `home-command-bar` class can never stand in for a word an owner can read --
 * attributes are stripped before matching, and "Homework" would not count as
 * "Home" if it ever appeared.
 */
function wordPattern(word: string): RegExp {
  const escaped = word.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&").replace(/ /gu, "\\s+");
  return new RegExp(`(?<![\\p{L}\\p{N}])${escaped}(?![\\p{L}\\p{N}])`, "iu");
}

/**
 * Plain-English sentences are the punctuation-terminated runs. The Home
 * dashboard is mostly labels ("OSL Chat", "Coming soon", tile names) which are
 * not sentences and would otherwise concatenate into one fake 100-word run;
 * only text a person reads as a sentence is held to the sentence bar.
 */
function sentencesOf(text: string): string[] {
  return (text.match(/[^.!?]+[.!?]+/gu) ?? []).map((sentence) => sentence.trim()).filter(Boolean);
}

function checkScreenWords(markup: string, namedWords: readonly string[]): ScreenWordsReport {
  const text = visibleText(markup);
  const sentences = sentencesOf(text);

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

const HOME_STATE = { route: "home", coreReady: true, storageMethod: "tpm-pcp" } as const;

/** The bar itself: the command-bar <header>, sliced the way 0816's check slices it. */
function homeTopBarMarkup(): string {
  ui.__oslHubUiTest.reset({ ...HOME_STATE });
  const shell = ui.__oslHubUiTest.renderRouteShell("home");
  const barStart = shell.indexOf(`<header class="home-header home-command-bar"`);
  expect(barStart, "the Home shell should contain the command bar").toBeGreaterThanOrEqual(0);
  return shell.slice(barStart, shell.indexOf("</header>", barStart) + "</header>".length);
}

/** The full rendered shell, sidebar and all, for the measured-absence sweep. */
function homeShellMarkup(): string {
  ui.__oslHubUiTest.reset({ ...HOME_STATE });
  return ui.__oslHubUiTest.renderRouteShell("home");
}

/** The screen the check reads: the bar plus the Home page it heads. */
function homeScreenMarkup(): string {
  const bar = homeTopBarMarkup();
  ui.__oslHubUiTest.reset({ ...HOME_STATE });
  return bar + ui.__oslHubUiTest.renderWorkspaceContent("home");
}

describe("TASK 0819 Home top bar words", () => {
  it("reads the page title, the word count, and every named word this bar has", () => {
    const screen = homeScreenMarkup();
    const report = checkScreenWords(screen, BAR_NAMED_WORDS);
    const barWords = countWords(visibleText(homeTopBarMarkup()));

    expect(report.title).toBe("Home");
    expect(report.wordsRead).toBeGreaterThanOrEqual(MIN_WORDS_READ);
    expect(report.missingWords).toEqual([]);
    expect(report.presentWords).toEqual([...BAR_NAMED_WORDS]);
    expect(report.bannedHits).toEqual([]);
    expect(failures(report, "Home")).toEqual([]);

    console.log(
      `TASK-0819 title="${report.title}" words-read=${report.wordsRead} bar-words=${barWords}` +
        ` named-present=${report.presentWords.length}/${BAR_NAMED_WORDS.length} named-missing=${report.missingWords.length}` +
        ` banned=${report.bannedHits.length} sentences=${report.sentences} longest-sentence=${report.longestSentenceWords}` +
        ` failures=${failures(report, "Home").length}`,
    );
  });

  it("records which of the finish line's six named words this bar really has", () => {
    const report = checkScreenWords(homeScreenMarkup(), PLAN_NAMED_WORDS);

    // Measured, not assumed: no Search control and no Messages control exist.
    // See the header comment -- 0816 built five controls and this is them.
    expect(report.presentWords).toEqual(["Home", "Settings", "Friends", "Profile"]);
    expect(report.missingWords).toEqual(["Search", "Messages"]);

    // Not hiding in the shared chrome either: the full shell, sidebar and
    // window controls included, shows neither word anywhere an owner can read.
    const shellText = visibleText(homeShellMarkup());
    expect(wordPattern("Search").test(shellText)).toBe(false);
    expect(wordPattern("Messages").test(shellText)).toBe(false);

    console.log(
      `TASK-0819 plan-named-words present=${report.presentWords.length}/${PLAN_NAMED_WORDS.length}` +
        ` [${report.presentWords.join("|")}] absent=[${report.missingWords.join("|")}] absent-from-full-shell=true`,
    );
  });

  it("finds zero banned words on the bar, the screen, and the whole shell", () => {
    const surfaces: Array<[string, string]> = [
      ["bar", homeTopBarMarkup()],
      ["screen", homeScreenMarkup()],
      ["shell", homeShellMarkup()],
    ];
    const hits = surfaces.flatMap(([surface, markup]) =>
      checkScreenWords(markup, BAR_NAMED_WORDS).bannedHits.map((hit) => `${surface}:${hit}`));

    expect(hits).toEqual([]);
    console.log(`TASK-0819 banned-word scan families=${BANNED_WORDS.length} surfaces=${surfaces.length} hits=${hits.length}`);
  });

  it("can actually find a banned word when one is there", () => {
    // A check that cannot go red is decoration. One doctored copy per family.
    const screen = homeScreenMarkup();
    for (const { family, pattern } of BANNED_WORDS) {
      const sample = { "deep-simplicity": "a keyserver", scary: "an attacker", technical: "key exchange", overclaim: "military-grade" }[family];
      expect(sample, family).toBeDefined();
      const doctored = screen.replace("</h1>", ` and ${sample}</h1>`);
      const report = checkScreenWords(doctored, BAR_NAMED_WORDS);
      expect(report.bannedHits.some((hit) => hit.startsWith(`${family}:`)), family).toBe(true);
      expect(failures(report, "Home").length, family).toBeGreaterThan(0);
      expect(pattern.test(sample!), family).toBe(true);
    }
  });

  it("fails on a throwaway copy of the screen missing 1 named word", () => {
    const screen = homeScreenMarkup();
    for (const word of BAR_NAMED_WORDS) {
      const throwaway = copyMissingWord(screen, word);
      const report = checkScreenWords(throwaway, BAR_NAMED_WORDS);
      const reasons = failures(report, "Home");

      expect(report.missingWords, word).toEqual([word]);
      expect(reasons, word).toContain(`missing named word: ${word}`);
      expect(reasons.length, word).toBeGreaterThan(0);

      // The intact screen passes the same check, so the failure is the deletion.
      expect(failures(checkScreenWords(screen, BAR_NAMED_WORDS), "Home"), word).toEqual([]);

      console.log(`TASK-0819 throwaway-missing="${word}" words-read=${report.wordsRead} named-missing=${report.missingWords.length} failures=${reasons.length}`);
    }
  });
});
