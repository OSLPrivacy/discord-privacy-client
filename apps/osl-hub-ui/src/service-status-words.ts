/**
 * TASK 0859 — the banned-word, plain-English and forbidden-future-label check
 * for the Service status page.
 *
 * TASK 0763 built {@link checkScreenWords}, which reads only the words a person
 * can SEE: tags are stripped before anything is judged, so a screen cannot
 * satisfy "Messages is present" from a `data-` attribute or a class name, and
 * cannot hide jargon in markup either. This module is that check plus the third
 * thing TASK 0859 asks for — the forbidden future label — and one distinction
 * TASK 0763 did not need.
 *
 * THE DISTINCTION: GENERATED PROSE IS REPORTED, AUTHORED PROSE IS BANNED. The
 * Service status page prints one paragraph it did not write — the reason the
 * backend generated for this surface, copied byte for byte out of the catalog
 * (`services.ts` / `apps/osl-hub/src/native_apps.rs`). `auditTileStatusRoute`
 * (TASK 0853) already splits the page's prose this way and reports future
 * promises in the generated half rather than banning them, for the good reason
 * that rewriting a generated sentence to please a word list is how a screen
 * starts lying. This check keeps that split:
 *
 *   - `page`     — every visible word on the screen. This is the number the
 *                  finish line is read against.
 *   - `authored` — the same screen with the generated reason removed. Nothing
 *                  in here has an excuse: every word of it was typed by the UI,
 *                  so a hit is a defect in this build.
 *
 * Both are reported for every service. Neither is allowed to hide the other.
 */

import {
  PLAIN_ENGLISH_BANNED_WORDS,
  checkScreenWords,
  findBannedWords,
  screenTitle,
  visibleText,
  visibleWords,
  type BannedWordHit,
  type ScreenWordsReport,
} from "./screen-words";
import { FUTURE_PROMISE_PHRASES, futurePromisesIn } from "./tile-status-route";

/** The `<h1>` the Service status page must carry. */
export const SERVICE_STATUS_TITLE = "Service status";

/**
 * The words TASK 0859 names. Matched as written, case included — the same rule
 * TASK 0763 settled on, so a sentence that happens to contain "messages" in the
 * middle of it cannot stand in for the "Messages" row.
 */
export const SERVICE_STATUS_REQUIRED_WORDS: readonly string[] = [
  "Service status",
  "Messages",
  "Friends",
  "Pictures",
  "Current status",
  "Last updated",
];

/** Fewest visible words for the screen to count as read at all. */
export const SERVICE_STATUS_LEAST_WORDS = 12;

/** One future-looking phrase and the sentence it was found in. */
export interface FutureLabelHit {
  phrase: string;
  /** The words around it, so a report says where to look. */
  context: string;
}

export interface ServiceStatusWordsRegion {
  wordCount: number;
  banned: BannedWordHit[];
  future: FutureLabelHit[];
}

export interface ServiceStatusWordsReport extends ScreenWordsReport {
  /** Forbidden future-looking labels anywhere a person can read them. */
  future: FutureLabelHit[];
  /** The same screen minus the prose the backend generated. */
  authored: ServiceStatusWordsRegion;
  /** How many banned terms were actually tried. */
  bannedTermsChecked: number;
  /** How many future phrases were actually tried. */
  futurePhrasesChecked: number;
}

/** Every future-looking phrase a person can read, with the words around it. */
export function findFutureLabels(markup: string): FutureLabelHit[] {
  const text = visibleText(markup);
  return futurePromisesIn(text).map((phrase) => {
    const body = phrase.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&").replace(/\s+/gu, "\\s+");
    const at = new RegExp(`\\b${body}\\b`, "iu").exec(text);
    const from = at === null ? 0 : Math.max(0, at.index - 30);
    return { phrase, context: text.slice(from, from + 90).trim() };
  });
}

export interface ServiceStatusWordsOptions {
  /**
   * Prose the backend generated, copied byte for byte onto the page. Removed
   * before the `authored` region is judged, and only then.
   */
  generatedText?: readonly string[];
  /** The product contract's own banned concepts, read off disk by the caller. */
  bannedWords?: readonly string[];
}

/**
 * Read the Service status page and report every way its words fall short.
 *
 * `ok` is the whole finish line at once: the title is right, at least
 * {@link SERVICE_STATUS_LEAST_WORDS} words are readable, every named word is on
 * the screen, no banned word is, and no future-looking label is.
 */
export function checkServiceStatusWords(
  markup: string,
  options: ServiceStatusWordsOptions = {},
): ServiceStatusWordsReport {
  const bannedWords = options.bannedWords ?? [];
  const base = checkScreenWords(markup, {
    title: SERVICE_STATUS_TITLE,
    requiredWords: SERVICE_STATUS_REQUIRED_WORDS,
    leastWords: SERVICE_STATUS_LEAST_WORDS,
    bannedWords,
  });
  const future = findFutureLabels(markup);

  let authoredText = visibleText(markup);
  for (const generated of options.generatedText ?? []) {
    const needle = visibleText(generated);
    if (needle.length > 0) authoredText = authoredText.split(needle).join(" ");
  }
  authoredText = authoredText.replace(/\s+/gu, " ").trim();

  return {
    ...base,
    future,
    authored: {
      wordCount: visibleWords(authoredText).length,
      banned: findBannedWords(authoredText, bannedWords),
      future: findFutureLabels(authoredText),
    },
    bannedTermsChecked: PLAIN_ENGLISH_BANNED_WORDS.length + bannedWords.filter((word) => word.trim().length > 0).length,
    futurePhrasesChecked: FUTURE_PROMISE_PHRASES.length,
    ok: base.ok && future.length === 0,
  };
}

/** The `<h1>` a screen carries. Re-exported so a caller needs one import. */
export { screenTitle };
