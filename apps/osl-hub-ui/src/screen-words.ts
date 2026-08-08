/**
 * Read the words a person actually sees on a screen, and judge them.
 *
 * The screens live in `main.ts` as template literals, so this reads a screen's
 * own render functions. Static template text is markup. An interpolation is
 * only screen copy when it sits *between* tags: `${cond ? "checked" : ""}`
 * inside an `<input …>` is machinery and is dropped with the tag, while
 * `<strong>${cond ? "Nothing new" : "Notifications are off"}</strong>` is copy
 * and both branches are read.
 *
 * The banned list is Liam's own, copied word for word from
 * `OSL-AUDITS/_STYLE.txt`.
 */

export const BANNED_SCREEN_WORDS = [
  "caller",
  "wired",
  "unwired",
  "production path",
  "adapter",
  "receipt",
  "attestation",
  "predicate",
  "seam",
  "ledger",
  "gate",
  "mutation",
  "schema",
  "enum",
  "verified-live",
  "test-proven",
  "implemented-unwired",
  "ODC",
  "carrier",
] as const;

export interface ScreenWordReport {
  /** The screen's page title: the text of its first `<h2>`. */
  title: string | null;
  /** Every word a person reads, in order, joined back into one line. */
  text: string;
  words: string[];
  wordCount: number;
  presentWords: string[];
  missingWords: string[];
  bannedWords: string[];
}

const QUOTES = new Set(['"', "'"]);
/** Stands in for one `${…}` while a template is scanned. Never appears in copy. */
const HOLE = "\u0000";

/** Cut the named render functions out of a module's source, in order. */
export function sliceScreenSource(source: string, functionNames: readonly string[]): string {
  return functionNames
    .map((name) => {
      const start = source.indexOf(`\nfunction ${name}(`);
      if (start < 0) throw new Error(`screen function ${name} is not in this source`);
      const end = source.indexOf("\nfunction ", start + 1);
      return source.slice(start, end < 0 ? source.length : end);
    })
    .join("\n");
}

function skipQuoted(source: string, open: number): number {
  const quote = source[open]!;
  for (let index = open + 1; index < source.length; index += 1) {
    const char = source[index]!;
    if (char === "\\") {
      index += 1;
      continue;
    }
    if (char === quote || char === "\n") return index;
  }
  return source.length - 1;
}

function skipTemplate(source: string, open: number): number {
  for (let index = open + 1; index < source.length; index += 1) {
    const char = source[index]!;
    if (char === "\\") {
      index += 1;
      continue;
    }
    if (char === "$" && source[index + 1] === "{") {
      index = matchBrace(source, index + 1);
      continue;
    }
    if (char === "`") return index;
  }
  return source.length - 1;
}

function matchBrace(source: string, open: number): number {
  let depth = 0;
  for (let index = open; index < source.length; index += 1) {
    const char = source[index]!;
    if (char === "\\") {
      index += 1;
      continue;
    }
    if (QUOTES.has(char)) {
      index = skipQuoted(source, index);
      continue;
    }
    if (char === "`") {
      index = skipTemplate(source, index);
      continue;
    }
    if (char === "{") depth += 1;
    else if (char === "}") {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  return source.length - 1;
}

/** Every template literal body in a chunk of source, comments and plain strings skipped. */
function templateBodies(source: string): string[] {
  const bodies: string[] = [];
  for (let index = 0; index < source.length; index += 1) {
    const char = source[index]!;
    if (char === "/" && source[index + 1] === "/") {
      const newline = source.indexOf("\n", index);
      if (newline < 0) break;
      index = newline;
      continue;
    }
    if (char === "/" && source[index + 1] === "*") {
      const close = source.indexOf("*/", index);
      if (close < 0) break;
      index = close + 1;
      continue;
    }
    if (QUOTES.has(char)) {
      index = skipQuoted(source, index);
      continue;
    }
    if (char === "`") {
      const end = skipTemplate(source, index);
      bodies.push(source.slice(index + 1, end));
      index = end;
    }
  }
  return bodies;
}

interface TemplatePart {
  kind: "static" | "hole";
  text: string;
}

function splitTemplate(body: string): TemplatePart[] {
  const parts: TemplatePart[] = [];
  let statics = "";
  for (let index = 0; index < body.length; index += 1) {
    const char = body[index]!;
    if (char === "\\") {
      statics += body.slice(index, index + 2);
      index += 1;
      continue;
    }
    if (char === "$" && body[index + 1] === "{") {
      const end = matchBrace(body, index + 1);
      parts.push({ kind: "static", text: statics });
      statics = "";
      parts.push({ kind: "hole", text: body.slice(index + 2, end) });
      index = end;
      continue;
    }
    statics += char;
  }
  parts.push({ kind: "static", text: statics });
  return parts;
}

/** `const previewText = "Hide message previews…";` is copy too. Collect those. */
function namedCopy(source: string): Map<string, string> {
  const named = new Map<string, string>();
  const declaration = /\bconst\s+([A-Za-z_$][\w$]*)\s*=\s*(?:"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)')\s*;/gu;
  for (const match of source.matchAll(declaration)) named.set(match[1]!, match[2] ?? match[3] ?? "");
  const conditional = /\bconst\s+([A-Za-z_$][\w$]*)\s*=\s*[^;?]+\?\s*"([^"]*)"\s*:\s*"([^"]*)"\s*;/gu;
  for (const match of source.matchAll(conditional)) named.set(match[1]!, `${match[2]} ${match[3]}`);
  return named;
}

/**
 * The copy inside one `${…}`. A bare name resolves to the copy it was given.
 * Otherwise only a branch value counts: a string sitting after `?`, `:` or
 * `??`. That keeps `"Nothing new"` and drops the `"en-US"` handed to
 * `toLocaleString`, which nobody reads.
 */
function copyFromHole(expression: string, named: Map<string, string>): string {
  const bare = named.get(expression.trim());
  if (bare !== undefined) return bare;
  const nested = templateBodies(expression).map((body) => markupFromTemplate(body, named));
  const branches: string[] = [];
  for (let index = 0; index < expression.length; index += 1) {
    const char = expression[index]!;
    if (char === "`") {
      index = skipTemplate(expression, index);
      continue;
    }
    if (!QUOTES.has(char)) continue;
    const end = skipQuoted(expression, index);
    const before = expression.slice(0, index).replace(/\s+$/u, "");
    const previous = before.at(-1) ?? "";
    if (previous === "?" || previous === ":") branches.push(expression.slice(index + 1, end));
    index = end;
  }
  return [...nested, ...branches].join(" ");
}

/** One template literal's markup, with the copy in its holes spliced in. */
function markupFromTemplate(body: string, named: Map<string, string>): string {
  const holes: string[] = [];
  let skeleton = "";
  for (const part of splitTemplate(body)) {
    if (part.kind === "static") {
      skeleton += part.text;
      continue;
    }
    skeleton += `${HOLE}${holes.length}${HOLE}`;
    holes.push(part.text);
  }

  let markup = "";
  let insideTag = false;
  for (let index = 0; index < skeleton.length; index += 1) {
    const char = skeleton[index]!;
    if (char === "<") insideTag = true;
    if (char === HOLE) {
      const close = skeleton.indexOf(HOLE, index + 1);
      const hole = holes[Number(skeleton.slice(index + 1, close))]!;
      if (!insideTag) markup += ` ${copyFromHole(hole, named)} `;
      index = close;
      continue;
    }
    markup += char;
    if (char === ">") insideTag = false;
  }
  return markup;
}

export function screenMarkup(screenSource: string): string {
  const named = namedCopy(screenSource);
  return templateBodies(screenSource).map((body) => markupFromTemplate(body, named)).join(" ");
}

/** Extract one named top-level function without compiling the surrounding module. */
export function sliceFunctionSource(source: string, name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}(`);
  const end = source.indexOf(`function ${nextName}(`, start + 1);
  if (start < 0 || end <= start) throw new Error(`cannot slice ${name} before ${nextName}`);
  return source.slice(start, end);
}

/** Reconstruct the user-visible template copy from a render function's source. */
export function screenMarkupFromSource(screenSource: string): string {
  return screenMarkup(screenSource);
}

function stripTags(markup: string): string {
  return markup.replace(/<[^<>]*>/gu, " ").replace(/&[a-z]+;/giu, " ");
}

export function readScreenTitle(screenSource: string): string | null {
  const heading = /<h2[^<>]*>([\s\S]*?)<\/h2>/u.exec(screenMarkup(screenSource));
  if (!heading) return null;
  const title = stripTags(heading[1]!).replace(/\s+/gu, " ").trim();
  return title.length ? title : null;
}

export function readScreenText(screenSource: string): string {
  return stripTags(screenMarkup(screenSource)).replace(/\s+/gu, " ").trim();
}

export function readScreenWords(screenSource: string): string[] {
  return readScreenText(screenSource)
    .split(/[^A-Za-z]+/u)
    .filter((word) => word.length > 0);
}

/** Banned words, plus their plain plural, matched whole and case-blind. */
export function findBannedWords(text: string): string[] {
  return BANNED_SCREEN_WORDS.filter((banned) => {
    const escaped = banned.replace(/[/\\^$*+?.()|[\]{}]/gu, "\\$&").replace(/\s+/gu, "\\s+");
    return new RegExp(`\\b${escaped}s?\\b`, "iu").test(text);
  });
}

export function checkScreenWords(screenSource: string, requiredWords: readonly string[]): ScreenWordReport;
export function checkScreenWords(markup: string, expected: ScreenWordsExpectation): ScreenWordsReport;
export function checkScreenWords(markup: string, expected: { title: string; requiredWords: readonly string[] }): ScreenWordReport;
export function checkScreenWords(
  screenSource: string,
  requiredWordsOrExpectation: readonly string[] | ScreenWordsExpectation | { title: string; requiredWords: readonly string[] },
): ScreenWordReport | ScreenWordsReport {
  if (!Array.isArray(requiredWordsOrExpectation)) {
    const expectation = requiredWordsOrExpectation as ScreenWordsExpectation | { title: string; requiredWords: readonly string[] };
    if ("leastWords" in expectation) return checkPlainEnglishScreenWords(screenSource, expectation);
    const text = visibleText(screenSource);
    const words = visibleWords(screenSource);
    const presentWords = expectation.requiredWords.filter((word) => text.includes(word));
    const bannedWords = BANNED_SCREEN_WORDS.filter((banned) => {
      const escaped = banned.replace(/[/\\^$*+?.()|[\]{}]/gu, "\\$&").replace(/\s+/gu, "\\s+");
      return new RegExp(`\\b${escaped}s?\\b`, "iu").test(text);
    });
    return {
      title: screenTitle(screenSource) || null,
      text,
      words,
      wordCount: words.length,
      presentWords: [...presentWords],
      missingWords: expectation.requiredWords.filter((word) => !presentWords.includes(word)),
      bannedWords: [...bannedWords],
    };
  }
  const requiredWords = requiredWordsOrExpectation as readonly string[];
  const text = readScreenText(screenSource);
  const words = readScreenWords(screenSource);
  const presentWords = requiredWords.filter((word) => words.includes(word));
  return {
    title: readScreenTitle(screenSource),
    text,
    words,
    wordCount: words.length,
    presentWords: [...presentWords],
    missingWords: requiredWords.filter((word) => !presentWords.includes(word)),
    bannedWords: findBannedWords(text),
  };
}

// ---------------------------------------------------------------------------
// lane/q's plain-English screen-words checker.
//
// Two lanes built a screen-words checker in this one file with different
// shapes: RC's takes (screenSource, requiredWords), lane/q's takes
// (markup, expectation) and returns per-hit detail. Every other name in the
// two files is disjoint, so both survive; only lane/q's two entry points are
// renamed, because RC's bare names already have four callers.
// ---------------------------------------------------------------------------

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
  bannedWords?: readonly string[];
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

export function findPlainEnglishBannedWords(markup: string, extraBanned: readonly string[] = []): BannedWordHit[] {
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
export function checkPlainEnglishScreenWords(
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
  const banned = findPlainEnglishBannedWords(markup, expectation.bannedWords ?? []);
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
