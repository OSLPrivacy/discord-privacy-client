/**
 * The banned-word and plain-English check for an OSL screen.
 *
 * TASK 0735 needs it first, but every "check <screen> words" task in
 * OSL-AUDITS/todo/05-ui-settings.txt asks for the same four measurements, so
 * this is written once and driven by a per-screen entry in
 * scripts/check-screen-words.mjs:
 *
 *   1. the page title is the exact string the task names
 *   2. at least N words are actually read off the screen (default 12)
 *   3. every named word the task lists is present in the visible text
 *   4. zero banned words are found, each one paired with the plain-English
 *      word that should have been used instead
 *
 * "Visible text" means what a person reads. Attribute values -- aria-label,
 * title, alt, data-* -- are stripped with the tags they sit in, so a screen
 * cannot pass by naming a control only to a machine. That matters here: the
 * new-friend-defaults section carries aria-label="New friend defaults", and if
 * attributes counted, the title check would pass on markup with no heading at
 * all.
 */

const BLOCK_ELEMENTS =
  /<\/?(?:p|div|section|article|header|footer|nav|main|aside|h[1-6]|ul|ol|li|dl|dt|dd|table|tr|td|th|form|fieldset|legend|label|button|option|select|textarea|br|hr|figure|figcaption|blockquote|pre|summary|details|strong|small|span|em)\b[^>]*>/gi;

const ENTITIES = new Map([
  ["&nbsp;", " "],
  ["&amp;", "&"],
  ["&lt;", "<"],
  ["&gt;", ">"],
  ["&quot;", '"'],
  ["&#39;", "'"],
  ["&apos;", "'"],
  ["&mdash;", "--"],
  ["&ndash;", "-"],
]);

function decodeEntities(text) {
  let out = text;
  for (const [entity, plain] of ENTITIES) {
    out = out.replaceAll(entity, plain);
  }
  return out;
}

/**
 * The text a person reads, in reading order. Script and style bodies go first
 * (they are never read), then every tag -- and with it every attribute value --
 * is replaced by a space so two words either side of a tag boundary do not fuse
 * into one.
 */
export function visibleTextFromMarkup(markup) {
  const withoutHidden = String(markup)
    .replace(/<!--[\s\S]*?-->/g, " ")
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, " ")
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, " ");
  const spaced = withoutHidden.replace(BLOCK_ELEMENTS, " ").replace(/<[^>]*>/g, " ");
  return decodeEntities(spaced).replace(/\s+/g, " ").trim();
}

/**
 * The page title is the first heading a person sees, not an accessible name.
 * h1 wins over h2 only by position: the first heading in the markup is the one
 * at the top of the screen.
 */
export function pageTitleFromMarkup(markup) {
  const match = String(markup).match(/<h[1-6]\b[^>]*>([\s\S]*?)<\/h[1-6]>/i);
  if (!match) return null;
  const title = visibleTextFromMarkup(match[1]);
  return title.length > 0 ? title : null;
}

/** Words, as a person counts them: letters, with internal apostrophes and hyphens kept. */
export function wordsOf(text) {
  return String(text).match(/[A-Za-z][A-Za-z'’]*(?:-[A-Za-z][A-Za-z'’]*)*/g) ?? [];
}

/**
 * The banned-word list, each entry with the plain-English word that replaces
 * it. Every jargon term here is either already banned by name somewhere in this
 * repo's UI tests (deep-simplicity.test.ts, home-layout.test.ts,
 * ui-simplicity.test.ts, zero-knowledge-ux.test.ts) or is build/crypto
 * vocabulary of the same kind: a word whose meaning a person cannot act on.
 *
 * Words the plan itself uses on these screens are deliberately NOT here --
 * "whitelist" and "auto-whitelist" are page titles in
 * OSL-AUDITS/todo/05-ui-settings.txt (TASK 0740, TASK 0764), and "encrypted"
 * is shipped friend copy. Banning a word the plan requires would make the
 * check unpassable rather than useful.
 */
export const BANNED_WORDS = Object.freeze([
  { jargon: "authenticate", plain: "sign in" },
  { jargon: "authentication", plain: "signing in" },
  { jargon: "credential", plain: "password" },
  { jargon: "credentials", plain: "password" },
  { jargon: "token", plain: "code" },
  { jargon: "nonce", plain: "one-time number" },
  { jargon: "entropy", plain: "randomness" },
  { jargon: "ciphertext", plain: "the scrambled message" },
  { jargon: "plaintext", plain: "the readable message" },
  { jargon: "cryptographic", plain: "protected" },
  { jargon: "cryptography", plain: "protection" },
  { jargon: "keyserver", plain: "the finder" },
  { jargon: "ratchet", plain: "key change" },
  { jargon: "sealer", plain: "the part that locks messages" },
  { jargon: "handshake", plain: "first hello" },
  { jargon: "bootstrap", plain: "set up" },
  { jargon: "provision", plain: "set up" },
  { jargon: "provisioning", plain: "setting up" },
  { jargon: "initialize", plain: "start" },
  { jargon: "initialise", plain: "start" },
  { jargon: "instantiate", plain: "start" },
  { jargon: "configure", plain: "set" },
  { jargon: "configuration", plain: "settings" },
  { jargon: "parameter", plain: "setting" },
  { jargon: "parameters", plain: "settings" },
  { jargon: "enable", plain: "turn on" },
  { jargon: "disable", plain: "turn off" },
  { jargon: "toggle", plain: "switch" },
  { jargon: "persist", plain: "save" },
  { jargon: "persisted", plain: "saved" },
  { jargon: "serialize", plain: "save" },
  { jargon: "cache", plain: "saved copy" },
  { jargon: "cached", plain: "saved" },
  { jargon: "metadata", plain: "details" },
  { jargon: "payload", plain: "message" },
  { jargon: "endpoint", plain: "address" },
  { jargon: "adapter", plain: "app connection" },
  { jargon: "backend", plain: "OSL" },
  { jargon: "frontend", plain: "the screen" },
  { jargon: "middleware", plain: "OSL" },
  { jargon: "webhook", plain: "message from the service" },
  { jargon: "runtime", plain: "while OSL is running" },
  { jargon: "daemon", plain: "background helper" },
  { jargon: "socket", plain: "connection" },
  { jargon: "buffer", plain: "waiting list" },
  { jargon: "schema", plain: "shape" },
  { jargon: "boolean", plain: "yes or no" },
  { jargon: "null", plain: "nothing" },
  { jargon: "undefined", plain: "not set" },
  { jargon: "invoke", plain: "run" },
  { jargon: "execute", plain: "run" },
  { jargon: "terminate", plain: "stop" },
  { jargon: "abort", plain: "stop" },
  { jargon: "idempotent", plain: "safe to repeat" },
  { jargon: "heuristic", plain: "rule of thumb" },
  { jargon: "granular", plain: "detailed" },
  { jargon: "telemetry", plain: "usage reports" },
  { jargon: "onboarding", plain: "getting started" },
  { jargon: "utilize", plain: "use" },
  { jargon: "utilise", plain: "use" },
  { jargon: "leverage", plain: "use" },
  { jargon: "facilitate", plain: "help" },
  { jargon: "commence", plain: "start" },
  { jargon: "ascertain", plain: "find out" },
  { jargon: "subsequently", plain: "then" },
  { jargon: "prior to", plain: "before" },
  { jargon: "in order to", plain: "to" },
  { jargon: "deprecated", plain: "no longer used" },
  { jargon: "synchronize", plain: "keep the same" },
  { jargon: "synchronise", plain: "keep the same" },
  { jargon: "canonical", plain: "the agreed one" },
  { jargon: "namespace", plain: "group" },
  { jargon: "hostname", plain: "computer name" },
  { jargon: "proxy", plain: "stand-in" },
  { jargon: "OAuth", plain: "sign in with the service" },
  { jargon: "E2EE", plain: "end-to-end encrypted" },
  { jargon: "PII", plain: "personal details" },
  { jargon: "MFA", plain: "extra sign-in check" },
  { jargon: "2FA", plain: "extra sign-in check" },
  { jargon: "TOTP", plain: "sign-in code" },
  { jargon: "AES", plain: "message protection" },
  { jargon: "RSA", plain: "message protection" },
  { jargon: "SHA-256", plain: "fingerprint" },
  { jargon: "SHA256", plain: "fingerprint" },
  { jargon: "TLS", plain: "a protected connection" },
  { jargon: "SSL", plain: "a protected connection" },
  { jargon: "DNS", plain: "the address book of the internet" },
  { jargon: "API", plain: "connection" },
  { jargon: "SDK", plain: "toolkit" },
  { jargon: "UUID", plain: "identifier" },
  { jargon: "JSON", plain: "file" },
  { jargon: "XML", plain: "file" },
]);

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Whole words only, so "Allow" never matches inside "allowance" and "API"
 * never matches inside "capital". Multi-word entries such as "prior to" match
 * across a single run of whitespace.
 */
function phraseRegExp(phrase, flags = "giu") {
  const body = escapeRegExp(phrase).replace(/\\?\s+/g, "\\s+");
  return new RegExp(`(?<![A-Za-z0-9])${body}(?![A-Za-z0-9])`, flags);
}

export function countPhrase(text, phrase) {
  return (String(text).match(phraseRegExp(phrase)) ?? []).length;
}

export function findBannedWords(text, list = BANNED_WORDS) {
  const found = [];
  for (const entry of list) {
    const count = countPhrase(text, entry.jargon);
    if (count > 0) found.push({ ...entry, count });
  }
  return found;
}

/**
 * The whole finish line of a "check <screen> words" task, as numbers.
 *
 * `ok` is true only when the title matches exactly, the word count clears the
 * floor, no named word is missing, and no banned word is found. Each failure is
 * also written out in `failures` so the caller can print why.
 */
export function checkScreenWords({
  markup,
  expectedTitle,
  requiredWords = [],
  minimumWords = 12,
  bannedWords = BANNED_WORDS,
}) {
  if (typeof markup !== "string" || markup.trim() === "") {
    throw new Error("checkScreenWords needs the screen's markup");
  }
  const visibleText = visibleTextFromMarkup(markup);
  const pageTitle = pageTitleFromMarkup(markup);
  const words = wordsOf(visibleText);

  const present = [];
  const missing = [];
  for (const word of requiredWords) {
    (countPhrase(visibleText, word) > 0 ? present : missing).push(word);
  }
  const banned = findBannedWords(visibleText, bannedWords);

  const failures = [];
  const titleMatches = pageTitle === expectedTitle;
  if (!titleMatches) {
    failures.push(`page title is ${pageTitle === null ? "missing" : `"${pageTitle}"`}, expected "${expectedTitle}"`);
  }
  if (words.length < minimumWords) {
    failures.push(`only ${words.length} words read, need at least ${minimumWords}`);
  }
  if (missing.length > 0) {
    failures.push(`named words missing from the screen: ${missing.join(", ")}`);
  }
  for (const entry of banned) {
    failures.push(`banned word "${entry.jargon}" appears ${entry.count} time(s); say "${entry.plain}"`);
  }

  return {
    pageTitle,
    expectedTitle,
    titleMatches,
    visibleText,
    wordsRead: words.length,
    minimumWords,
    requiredWords: [...requiredWords],
    present,
    missing,
    banned,
    bannedCount: banned.reduce((total, entry) => total + entry.count, 0),
    failures,
    ok: failures.length === 0,
  };
}

/**
 * A throwaway copy of the screen with one named word taken out of the visible
 * text -- the mutation every one of these tasks asks for as its proof that the
 * check can go red. It edits text nodes only, so the markup stays well formed
 * and nothing but the word itself changes.
 */
export function screenCopyMissingWord(markup, word) {
  const pattern = phraseRegExp(word, "giu");
  let changed = 0;
  const copy = String(markup).replace(/>([^<]+)</g, (whole, text) => {
    const replaced = text.replace(pattern, () => {
      changed += 1;
      return " ";
    });
    return `>${replaced}<`;
  });
  if (changed === 0) {
    throw new Error(`screenCopyMissingWord: "${word}" is not in the visible text, so removing it proves nothing`);
  }
  return copy;
}

export function formatReport(prefix, report) {
  const lines = [
    `${prefix}_PAGE_TITLE ${JSON.stringify(report.pageTitle)}`,
    `${prefix}_PAGE_TITLE_MATCHES ${report.titleMatches}`,
    `${prefix}_WORDS_READ ${report.wordsRead} (minimum ${report.minimumWords})`,
    `${prefix}_NAMED_WORDS_PRESENT ${report.present.length}/${report.requiredWords.length} ${JSON.stringify(report.present)}`,
    `${prefix}_NAMED_WORDS_MISSING ${report.missing.length} ${JSON.stringify(report.missing)}`,
    `${prefix}_BANNED_WORDS_FOUND ${report.bannedCount} ${JSON.stringify(report.banned.map((entry) => entry.jargon))}`,
    `${prefix}_BANNED_LIST_SIZE ${BANNED_WORDS.length}`,
    `${prefix}_OK ${report.ok}`,
  ];
  for (const failure of report.failures) lines.push(`${prefix}_FAILURE ${failure}`);
  return lines.join("\n");
}
