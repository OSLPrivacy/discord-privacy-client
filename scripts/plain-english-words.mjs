/**
 * The banned-word list and the plain-English replacements, plus the small
 * readers a screen-words check needs.
 *
 * WHAT IS BANNED, AND WHY
 *
 * A banned word is one that describes OSL's machinery in the vocabulary of the
 * people who built it. The replacement column is the point: every entry says
 * what to write instead, so a failure is a fix and not an argument. The list
 * is drawn from two places, neither of them invented here:
 *
 *   1. Software jargon -- words that name a part of a program (endpoint,
 *      payload, schema, config, hash, socket) or an action only a programmer
 *      takes (instantiate, serialise, parse). Nobody who has not written
 *      software is expected to know these.
 *   2. The GOV.UK style guide's "words to avoid" -- the government plain
 *      English list (utilise, leverage, facilitate, robust, going forward,
 *      key priority, and the rest), plus the stock legal-register padding it
 *      warns about (prior to, in order to, commence, aforementioned).
 *
 * WHAT IS NOT BANNED, AND WHY
 *
 * - The names the apps themselves use. Discord calls a thing a server, a
 *   channel, a thread and a group DM; Telegram has supergroups; WhatsApp has
 *   broadcast lists. Those words are on the person's screen in the app they
 *   already use, so replacing them would make OSL harder to follow, not
 *   easier. A screen naming a place kind is not speaking jargon.
 * - "whitelist". It is jargon by any reasonable reading, but the page title
 *   "Auto-whitelist rules" is fixed by the plan (TASK 0742 checks for it in
 *   the screenshot, TASK 0743 checks for it here), so a check that banned it
 *   would fail a screen for obeying its own specification. The checks print a
 *   note saying so rather than scanning it silently.
 *
 * Every entry is a whole word or phrase, matched case-insensitively with word
 * boundaries, so "cache" does not fire on "cached out" the way a substring
 * search would, and "post" (a place kind) is never touched by "postpone".
 */

/** @typedef {{ word: string, say: string, pattern?: string }} BannedWord */

/** Jargon that names a part of a program, or an act only a programmer performs. */
const SOFTWARE_JARGON = [
  { word: "authenticate", pattern: "authenticat(?:e|es|ed|ing|ion)", say: "check it is you" },
  { word: "credential", pattern: "credentials?", say: "password" },
  { word: "metadata", say: "the extra details kept alongside a message" },
  { word: "endpoint", pattern: "endpoints?", say: "address" },
  { word: "config", pattern: "configs?|configuration|configure[sd]?", say: "settings" },
  { word: "provision", pattern: "provision(?:s|ed|ing)?", say: "set up" },
  { word: "initialise", pattern: "initiali[sz]e[sd]?|initiali[sz]ing", say: "start" },
  { word: "instantiate", pattern: "instantiate[sd]?|instantiating", say: "make" },
  { word: "serialise", pattern: "seriali[sz]e[sd]?|seriali[sz]ing", say: "write out" },
  { word: "parse", pattern: "parse[sd]?|parsing", say: "read" },
  { word: "payload", pattern: "payloads?", say: "what is sent" },
  { word: "schema", pattern: "schemas?", say: "shape" },
  { word: "boolean", pattern: "booleans?", say: "yes or no" },
  { word: "null", say: "nothing" },
  { word: "exception", pattern: "exceptions?", say: "error" },
  { word: "stack trace", pattern: "stack traces?", say: "error details" },
  { word: "backend", pattern: "back-?ends?", say: "the part of OSL on this computer" },
  { word: "frontend", pattern: "front-?ends?", say: "the screen" },
  { word: "API", say: "the way two programs talk" },
  { word: "IPC", say: "the way two parts of OSL talk" },
  { word: "UUID", pattern: "uuids?", say: "identifier" },
  { word: "hash", pattern: "hash(?:es|ed|ing)?", say: "fingerprint" },
  { word: "base64", say: "encoded text" },
  { word: "mutex", pattern: "mutexe?s?", say: "lock" },
  { word: "asynchronous", pattern: "asynchronous(?:ly)?|async", say: "in the background" },
  { word: "cache", pattern: "cache[sd]?|caching", say: "kept copy" },
  { word: "daemon", pattern: "daemons?", say: "background program" },
  { word: "socket", pattern: "sockets?", say: "connection" },
  { word: "handshake", pattern: "handshakes?", say: "first hello" },
  { word: "heuristic", pattern: "heuristics?", say: "rule of thumb" },
  { word: "deprecated", say: "no longer used" },
  { word: "enumerate", pattern: "enumerate[sd]?|enumerating", say: "list" },
  { word: "persist", pattern: "persist(?:s|ed|ing|ence)?", say: "save" },
  { word: "toggle", pattern: "toggle[sd]?|toggling", say: "switch" },
  { word: "regex", pattern: "regexe?s?", say: "pattern" },
  { word: "artifact", pattern: "artif[ai]cts?", say: "file" },
];

/** The GOV.UK "words to avoid" list, and the stock padding it warns against. */
const GOV_UK_WORDS_TO_AVOID = [
  { word: "utilise", pattern: "utili[sz]e[sd]?|utili[sz]ing|utili[sz]ation", say: "use" },
  { word: "leverage", pattern: "leverage[sd]?|leveraging", say: "use" },
  { word: "facilitate", pattern: "facilitate[sd]?|facilitating", say: "help" },
  { word: "empower", pattern: "empower(?:s|ed|ing)?", say: "let" },
  { word: "robust", say: "say what it actually does" },
  { word: "seamless", pattern: "seamless(?:ly)?", say: "say what actually happens" },
  { word: "overarching", say: "main" },
  { word: "going forward", say: "from now on" },
  { word: "in order to", say: "to" },
  { word: "prior to", say: "before" },
  { word: "subsequently", say: "then" },
  { word: "aforementioned", say: "name the thing again" },
  { word: "commence", pattern: "commence[sd]?|commencing", say: "start" },
  { word: "terminate", pattern: "terminate[sd]?|terminating|termination", say: "stop" },
  { word: "ascertain", pattern: "ascertain(?:s|ed|ing)?", say: "find out" },
  { word: "requisite", say: "needed" },
  { word: "liaise", pattern: "liaise[sd]?|liaising", say: "talk to" },
  { word: "streamline", pattern: "streamline[sd]?|streamlining", say: "simplify" },
  { word: "delivery", pattern: "deliverables?", say: "what you get" },
  { word: "e.g.", pattern: "e\\.g\\.", say: "for example" },
  { word: "i.e.", pattern: "i\\.e\\.", say: "that is" },
  { word: "N/A", pattern: "n/a", say: "none" },
];

/** Every banned word, with the plain-English replacement for each. */
export const BANNED_WORDS = [...SOFTWARE_JARGON, ...GOV_UK_WORDS_TO_AVOID];

/**
 * Words a screen may use that a naive jargon sweep would flag, with the reason
 * each is kept. Printed by the checks so the exemptions are visible rather
 * than silent.
 */
export const KEPT_WORDS = [
  {
    word: "whitelist",
    why: 'the plan fixes the page titles "Auto-whitelist rules" (TASK 0742, TASK 0743) and "Whitelisting" (TASK 0766, TASK 0767)',
  },
  {
    word: "server, channel, thread, group DM, supergroup, broadcast list, story, reel",
    why: "these are the apps' own names for a place, shown to the person in the app already",
  },
];

function escapeForRegex(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** The matcher for one banned word: whole word or phrase, any case. */
export function bannedWordRegex(entry) {
  const body = entry.pattern ?? escapeForRegex(entry.word);
  const leading = /^[\p{L}\p{N}]/u.test(entry.pattern ?? entry.word) ? "\\b" : "";
  const trailing = /[\p{L}\p{N}]$/u.test(entry.pattern ?? entry.word) ? "\\b" : "";
  return new RegExp(`${leading}(?:${body})${trailing}`, "giu");
}

/** Visible text of a fragment of markup: tags out, entities back, spaces squashed. */
export function visibleText(markup) {
  return markup
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/giu, " ")
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/giu, " ")
    .replace(/<[^>]+>/gu, " ")
    .replace(/&nbsp;/giu, " ")
    .replace(/&#39;/gu, "'")
    .replace(/&quot;/gu, '"')
    .replace(/&lt;/gu, "<")
    .replace(/&gt;/gu, ">")
    .replace(/&amp;/gu, "&")
    .replace(/\s+/gu, " ")
    .trim();
}

/**
 * The words a person reads on the screen. A token counts as a word when it
 * carries a letter, so "38" and "7" are not counted as words people read and
 * a stray dash is not either.
 */
export function readWords(text) {
  return text.split(/\s+/u).filter((token) => /\p{L}/u.test(token));
}

/**
 * The name a screen gives itself, from whichever of the two ways it uses.
 *
 * `aria-label` carries the words directly. `aria-labelledby` names an element
 * elsewhere on the screen and borrows its words; that is the same accessible
 * name to a screen reader, so a check that only read `aria-label` would fail a
 * correctly-labelled screen for choosing the other spelling. Returns null when
 * the screen is unlabelled, or when it points at an id that is not there --
 * a dangling `aria-labelledby` gives a screen reader nothing to say.
 */
function screenLabel(markup) {
  const direct = markup.match(/<section\b[^>]*\baria-label="([^"]*)"/iu);
  if (direct) return { text: visibleText(direct[1]), why: null };
  const indirect = markup.match(/<section\b[^>]*\baria-labelledby="([^"]*)"/iu);
  if (!indirect) {
    return { text: null, why: "the screen's outer section carries no aria-label or aria-labelledby" };
  }
  const id = indirect[1].trim().split(/\s+/u)[0];
  const target = markup.match(
    new RegExp(`<([a-z0-9]+)\\b[^>]*\\bid="${escapeForRegex(id)}"[^>]*>([\\s\\S]*?)<\\/\\1>`, "iu"),
  );
  if (!target) {
    return { text: null, why: `aria-labelledby names id "${id}", which is not on the screen` };
  }
  return { text: visibleText(target[2]), why: null };
}

/**
 * The page title: the screen's own heading. Both the heading element and the
 * label the screen gives itself have to say it, and say the same thing --
 * a heading that disagrees with the label is not a title anyone can rely on.
 */
export function pageTitle(markup) {
  const heading = markup.match(/<h[12][^>]*>([\s\S]*?)<\/h[12]>/iu);
  const labelled = screenLabel(markup);
  if (!heading) return { title: null, why: "no <h1> or <h2> heading in the screen" };
  const headingText = visibleText(heading[1]);
  if (labelled.text === null) return { title: null, why: labelled.why };
  if (headingText !== labelled.text) {
    return {
      title: null,
      why: `heading "${headingText}" and screen label "${labelled.text}" disagree`,
    };
  }
  return { title: headingText, why: null };
}

/** Which required words the screen says, and which it does not. */
export function requiredWordReport(text, requiredWords) {
  return requiredWords.map((word) => {
    const pattern = new RegExp(
      `\\b${escapeForRegex(word).replace(/\s+/gu, "\\s+")}\\b`,
      "iu",
    );
    return { word, present: pattern.test(text) };
  });
}

/** Every banned word on the screen, with where it is and what to say instead. */
export function bannedWordReport(text, banned = BANNED_WORDS) {
  const found = [];
  for (const entry of banned) {
    for (const match of text.matchAll(bannedWordRegex(entry))) {
      found.push({
        word: entry.word,
        say: entry.say,
        found: match[0],
        at: match.index,
        snippet: text.slice(Math.max(0, match.index - 30), match.index + match[0].length + 30),
      });
    }
  }
  return found.sort((left, right) => left.at - right.at);
}
