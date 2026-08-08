/**
 * TASK 0763 - the banned-word and plain-English check for a rendered screen.
 *
 * The check reads the words a person can actually SEE. It strips tags, so the
 * text it judges never includes an `aria-label`, a `data-` attribute or a class
 * name: a screen cannot satisfy "Save is present" with a hidden attribute, and
 * cannot smuggle jargon past the ban by hiding it in markup either.
 *
 * The banned vocabulary comes in two halves:
 *
 * - the product contract's own `banned_user_facing_concepts`, which the caller
 *   passes in (they live in `docs/design/osl-subjective-design-feel.md`, and a
 *   browser bundle must not read markdown off disk); and
 * - `PLAIN_ENGLISH_BANNED_WORDS` below - implementation vocabulary that is
 *   correct English but not plain English for the person using the app.
 */

/** One banned term and the word on screen that tripped it. */
export interface BannedWordHit {
  /** The banned term as it was declared. */
  term: string;
  /** What the screen actually said, in the screen's own casing. */
  found: string;
}

export interface ScreenWordsExpectation {
  /** The `<h1>` text this screen must carry. */
  title: string;
  /** Words and phrases a person must be able to read on the screen. */
  requiredWords: readonly string[];
  /** Fewest visible words for the screen to count as read at all. */
  leastWords: number;
  /** Extra banned terms from the product contract, on top of the plain-English list. */
  bannedWords: readonly string[];
}

export interface ScreenWordsReport {
  /** The `<h1>` text, or "" when the screen has none. */
  title: string;
  titleMatches: boolean;
  /** Every visible word, in reading order. */
  words: string[];
  wordCount: number;
  enoughWords: boolean;
  present: string[];
  missing: string[];
  banned: BannedWordHit[];
  ok: boolean;
}

/**
 * Implementation vocabulary that must never reach a person. Each of these is a
 * word an engineer reaches for when describing what the code does rather than
 * what the person gets: "adapter", "endpoint" and "payload" describe our
 * plumbing, "boolean"/"null" describe our types, "OAuth"/"IPC"/"webhook"
 * describe our transports, and "ciphertext"/"nonce" describe our crypto.
 */
export const PLAIN_ENGLISH_BANNED_WORDS: readonly string[] = [
  "adapter",
  "api",
  "async",
  "backend",
  "boolean",
  "callback",
  "ciphertext",
  "daemon",
  "deserialize",
  "endpoint",
  "enum",
  "frontend",
  "handshake",
  "idempotent",
  "ipc",
  "middleware",
  "mutex",
  "nonce",
  "null",
  "oauth",
  "payload",
  "regex",
  "serialize",
  "socket",
  "stderr",
  "stdout",
  "struct",
  "webhook",
];

const ENTITIES: ReadonlyArray<readonly [RegExp, string]> = [
  [/&nbsp;/gu, " "],
  [/&lt;/gu, "<"],
  [/&gt;/gu, ">"],
  [/&quot;/gu, '"'],
  [/&#39;/gu, "'"],
  [/&amp;/gu, "&"],
];

/** The text a person can read: no tags, no attributes, no entities. */
export function visibleText(markup: string): string {
  let text = markup
    .replace(/<script[\s\S]*?<\/script>/giu, " ")
    .replace(/<style[\s\S]*?<\/style>/giu, " ")
    .replace(/<[^>]*>/gu, " ");
  for (const [pattern, plain] of ENTITIES) text = text.replace(pattern, plain);
  return text.replace(/\s+/gu, " ").trim();
}

/** Visible words, punctuation dropped, in reading order. */
export function visibleWords(markup: string): string[] {
  return visibleText(markup)
    .split(/\s+/u)
    .map((word) => word.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, ""))
    .filter((word) => word.length > 0);
}

/** The `<h1>` text, or "" when the screen has no heading at all. */
export function screenTitle(markup: string): string {
  const match = /<h1\b[^>]*>([\s\S]*?)<\/h1>/iu.exec(markup);
  return match === null ? "" : visibleText(match[1]);
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
}

/**
 * Terms are declared in whichever number the contract uses, so "keyservers"
 * must still catch a screen that says "keyserver". Match the singular stem and
 * allow the plural back on, the way the Rust design-feel contract does.
 */
function bannedTermPattern(term: string): RegExp {
  const stem = term.trim().replace(/s$/iu, "");
  return new RegExp(`(?<![\\p{L}\\p{N}])${escapeRegExp(stem)}(?:e?s)?(?![\\p{L}\\p{N}])`, "giu");
}

export function findBannedWords(markup: string, extraBanned: readonly string[] = []): BannedWordHit[] {
  const text = visibleText(markup);
  const hits: BannedWordHit[] = [];
  const seen = new Set<string>();
  for (const term of [...PLAIN_ENGLISH_BANNED_WORDS, ...extraBanned]) {
    if (term.trim().length === 0) continue;
    for (const match of text.matchAll(bannedTermPattern(term))) {
      const key = `${term}::${match[0].toLowerCase()}`;
      if (seen.has(key)) continue;
      seen.add(key);
      hits.push({ term, found: match[0] });
    }
  }
  return hits;
}

/**
 * A required word counts as present only when the screen shows it as written -
 * same casing, same spacing. Case matters: this screen's save note ends "after
 * you save.", and a case-insensitive check would let that sentence stand in for
 * the Save button, reporting a button that is not there. Whitespace in the
 * needle is collapsed first, so a line break in the markup does not read as a
 * missing word.
 */
export function checkScreenWords(
  markup: string,
  expectation: ScreenWordsExpectation,
): ScreenWordsReport {
  const haystack = visibleText(markup);
  const words = visibleWords(markup);
  const title = screenTitle(markup);
  const present: string[] = [];
  const missing: string[] = [];
  for (const required of expectation.requiredWords) {
    const needle = required.replace(/\s+/gu, " ").trim();
    if (haystack.includes(needle)) present.push(required);
    else missing.push(required);
  }
  const banned = findBannedWords(markup, expectation.bannedWords);
  const titleMatches = title === expectation.title;
  const enoughWords = words.length >= expectation.leastWords;
  return {
    title,
    titleMatches,
    words,
    wordCount: words.length,
    enoughWords,
    present,
    missing,
    banned,
    ok: titleMatches && enoughWords && missing.length === 0 && banned.length === 0,
  };
}
