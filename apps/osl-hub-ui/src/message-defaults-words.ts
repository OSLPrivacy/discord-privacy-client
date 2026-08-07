/**
 * TASK 0759 — the banned-word and plain-English check for the Message
 * defaults screen. The list and the checker live in `settings-home-words.ts`
 * (TASK 0719); this file only names what THIS screen must show.
 */

import { checkScreenWords, type ScreenWordCheck } from "./settings-home-words";
import { MESSAGE_DEFAULTS_TITLE } from "./message-defaults";

/** The words TASK 0759 names, exactly as the screen must show them. */
export const messageDefaultsRequiredWords: readonly string[] = [
  "Message defaults",
  "Protected messages",
  "Burn after reading",
  "Read receipts",
  "Save",
];

export function checkMessageDefaultsWords(markup: string): ScreenWordCheck {
  return checkScreenWords(markup, {
    title: MESSAGE_DEFAULTS_TITLE,
    requiredWords: messageDefaultsRequiredWords,
  });
}
