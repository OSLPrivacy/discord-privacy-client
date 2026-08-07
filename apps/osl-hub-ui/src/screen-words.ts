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

export function checkScreenWords(screenSource: string, requiredWords: readonly string[]): ScreenWordReport {
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
