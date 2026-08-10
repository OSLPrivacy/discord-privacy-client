/**
 * The Settings home: the first thing a user sees when they open Settings.
 *
 * Each choice carries one plain line saying what lives behind it. The label
 * alone does not tell a non-technical user whether "Scrub" deletes their
 * messages or their account, so every choice carries its own explanation and
 * the screen is judged on both being visible at once.
 *
 * Every visible word is held to the plain-English word check in
 * `settings-home-words.ts` (TASK 0719): the screen must show Privacy,
 * "Apps and sending", Look, and a Home link, and may never show a word from
 * the banned list ("Appearance" is now "Look").
 */

export type SettingsHomeChoiceId =
  | "account"
  | "apps"
  | "friends"
  | "privacy"
  | "whitelisting"
  | "scrub"
  | "cleanup"
  | "notifications"
  | "window-sounds"
  | "appearance"
  | "about";

export interface SettingsHomeChoice {
  readonly id: SettingsHomeChoiceId;
  readonly label: string;
  /** One line. No second sentence, no line break, no jargon. */
  readonly explanation: string;
}

export const settingsHomeChoices: readonly SettingsHomeChoice[] = [
  {
    id: "account",
    label: "Account",
    explanation: "Your OSL identity, password, and recovery phrase on this device.",
  },
  {
    id: "apps",
    label: "Apps and sending",
    explanation: "Which apps and accounts OSL may open, and how it sends your messages.",
  },
  {
    id: "friends",
    label: "Friends",
    explanation: "Who you trust, who may contact you, and which accounts friends can see.",
  },
  {
    id: "privacy",
    label: "Privacy",
    explanation: "Your privacy level, and who can find you or read what you protect.",
  },
  {
    id: "whitelisting",
    label: "Whitelisting",
    explanation: "The exact chats each verified person is allowed to be protected in.",
  },
  {
    id: "scrub",
    label: "Scrub",
    explanation: "Find what a service already stores about you, then ask it to delete it.",
  },
  {
    id: "cleanup",
    label: "Cleanup",
    explanation: "Remove many old messages and local copies at once, after you confirm.",
  },
  {
    id: "notifications",
    label: "Notifications",
    explanation: "Which alerts OSL raises on this device, and how much they show.",
  },
  {
    id: "window-sounds",
    label: "Window & sounds",
    explanation: "Where the window opens, how it moves, and which sounds this device plays.",
  },
  {
    id: "appearance",
    label: "Look",
    explanation: "Theme, text size, and motion, changed for this device only.",
  },
  {
    id: "about",
    label: "About",
    explanation: "Build version, update checks, licence, and where the source lives.",
  },
];

const explanationsById = new Map<string, string>(
  settingsHomeChoices.map((choice) => [choice.id, choice.explanation]),
);

/**
 * Refuses rather than guesses: a choice rendered without its own line would
 * ship a Settings home that reads as complete while one option is unexplained.
 */
export function settingsHomeExplanation(id: string): string {
  const explanation = explanationsById.get(id);
  if (explanation === undefined) throw new Error(`Settings home has no explanation for "${id}"`);
  return explanation;
}

export function isSettingsHomeChoiceId(value: unknown): value is SettingsHomeChoiceId {
  return typeof value === "string" && explanationsById.has(value);
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

export interface SettingsHomeChoiceMarkupOptions {
  readonly active: boolean;
}

/** One choice button: the label, then its single explanation line beneath it. */
export function settingsHomeChoiceMarkup(
  id: string,
  label: string,
  options: SettingsHomeChoiceMarkupOptions,
): string {
  const explanation = settingsHomeExplanation(id);
  const active = options.active;
  return `<button class="settings-home-choice ${active ? "active" : ""}" data-settings="${escapeHtml(id)}" data-settings-home-choice="${escapeHtml(id)}" type="button" ${active ? 'aria-current="true"' : ""}><strong class="settings-home-choice-label">${escapeHtml(label)}</strong><small class="settings-home-choice-explanation" data-settings-home-explanation="${escapeHtml(id)}">${escapeHtml(explanation)}</small></button>`;
}

/**
 * The whole Settings home menu. `settingsContent()` in main.ts renders exactly
 * this, and so does the Linux screenshot capture, so the pixels that get judged
 * come from the shipped function rather than a screenshot-only copy of it.
 */
export function settingsHomeMenuMarkup(
  items: ReadonlyArray<readonly [string, string]>,
  activeId: string,
): string {
  const missing = settingsHomeChoices
    .map((choice) => choice.id)
    .filter((id) => !items.some(([itemId]) => itemId === id));
  if (missing.length > 0) {
    throw new Error(`Settings home is missing choices: ${missing.join(", ")}`);
  }
  return items
    .map(([id, label]) => settingsHomeChoiceMarkup(id, label, { active: id === activeId }))
    .join("");
}

/** The menu exactly as the Settings home ships it, for captures and tests. */
export function settingsHomeMenuItems(): ReadonlyArray<readonly [string, string]> {
  return settingsHomeChoices.map((choice) => [choice.id, choice.label] as const);
}

/**
 * The whole Settings home screen: a Home link back out, the page title, then
 * the menu. This is the surface the plain-English word check reads — the words
 * it judges are the words the shipped screen renders, not a copy of them.
 */
export function settingsHomePageMarkup(activeId: string = ""): string {
  return `<main class="content-viewport settings-home-page" aria-labelledby="settings-home-title"><header class="settings-home-header"><button class="settings-home-back" type="button" data-route="home">Home</button><h1 id="settings-home-title" tabindex="-1">Settings</h1></header><nav class="settings-home-menu" aria-label="Settings">${settingsHomeMenuMarkup(settingsHomeMenuItems(), activeId)}</nav></main>`;
}
