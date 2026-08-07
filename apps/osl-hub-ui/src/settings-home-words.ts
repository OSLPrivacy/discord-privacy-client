/**
 * TASK 0719 — the banned-word and plain-English check.
 *
 * The plan runs this same check against every settings screen ("Run the
 * banned-word and plain-English check on this screen"), so the list and the
 * checker are generic; only the expected title and required words differ per
 * screen. A screen passes when its title is right, it shows at least twelve
 * visible words, every required word is among them, and none of its visible
 * words is on the banned list.
 *
 * The banned list is jargon a non-technical user should never meet, each with
 * the plain-English replacement the UI uses instead. Product names the plan
 * itself keeps as screen titles (Whitelisting, Scrub, Auto-whitelist rules)
 * are deliberately NOT banned.
 */

export interface PlainEnglishRule {
  /** The word or phrase that must never be visible. Matched whole-word, case-insensitively. */
  readonly banned: string;
  /** What the UI says instead. */
  readonly plainEnglish: string;
}

export const bannedWords: readonly PlainEnglishRule[] = [
  { banned: "appearance", plainEnglish: "Look" },
  { banned: "adapter", plainEnglish: "connection" },
  { banned: "scope", plainEnglish: "access" },
  { banned: "cryptographic", plainEnglish: "locked" },
  { banned: "out of band", plainEnglish: "in person or in another app" },
  { banned: "config", plainEnglish: "settings" },
  { banned: "configuration", plainEnglish: "settings" },
  { banned: "configure", plainEnglish: "set up" },
  { banned: "endpoint", plainEnglish: "address" },
  { banned: "token", plainEnglish: "code" },
  { banned: "metadata", plainEnglish: "message details" },
  { banned: "authentication", plainEnglish: "sign-in" },
  { banned: "daemon", plainEnglish: "background helper" },
  { banned: "sidecar", plainEnglish: "helper app" },
  { banned: "IPC", plainEnglish: "app messaging" },
  { banned: "initialize", plainEnglish: "start" },
];

/** Visible text only: scripts, styles, tags, and attributes are not words a user reads. */
export function visibleText(markup: string): string {
  return markup
    .replace(/<script[\s\S]*?<\/script>/giu, " ")
    .replace(/<style[\s\S]*?<\/style>/giu, " ")
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

export function visibleWords(markup: string): string[] {
  const text = visibleText(markup);
  return text.length === 0 ? [] : text.split(" ");
}

/** The text of the first <h1>, because "X is the page title" means the heading, not a guess. */
export function pageTitle(markup: string): string | null {
  const match = /<h1[^>]*>([\s\S]*?)<\/h1>/iu.exec(markup);
  if (!match) return null;
  return visibleText(match[1]!);
}

function bannedPattern(banned: string): RegExp {
  const escaped = banned.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&").replace(/\s+/gu, "\\s+");
  return new RegExp(`\\b${escaped}\\b`, "iu");
}

export interface ScreenWordCheck {
  readonly title: string | null;
  readonly wordCount: number;
  /** Required words that the visible text does not contain. */
  readonly missingWords: string[];
  /** Banned words found in the visible text, with their plain-English replacements. */
  readonly bannedFound: PlainEnglishRule[];
  readonly pass: boolean;
}

export function checkScreenWords(
  markup: string,
  expected: { readonly title: string; readonly requiredWords: readonly string[] },
): ScreenWordCheck {
  const title = pageTitle(markup);
  const text = visibleText(markup);
  const wordCount = visibleWords(markup).length;
  const missingWords = expected.requiredWords.filter((word) => !text.includes(word));
  const bannedFound = bannedWords.filter((rule) => bannedPattern(rule.banned).test(text));
  const pass =
    title === expected.title && wordCount >= 12 && missingWords.length === 0 && bannedFound.length === 0;
  return { title, wordCount, missingWords, bannedFound, pass };
}

/** The words TASK 0719 names for the Settings home, exactly as the screen must show them. */
export const settingsHomeRequiredWords: readonly string[] = [
  "Settings",
  "Privacy",
  "Notifications",
  "Account",
  "Apps and sending",
  "Look",
  "Home",
];

export function checkSettingsHomeWords(markup: string): ScreenWordCheck {
  return checkScreenWords(markup, { title: "Settings", requiredWords: settingsHomeRequiredWords });
}
