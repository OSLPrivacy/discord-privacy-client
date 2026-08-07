/**
 * TASK 0791 — the banned-word and plain-English check for the Friend
 * pictures screen. The list and the checker live in `settings-home-words.ts`
 * (TASK 0719); this file only names what THIS screen must show.
 */

import { checkScreenWords, type ScreenWordCheck } from "./settings-home-words";
import { FRIEND_PICTURES_SCREEN_TITLE } from "./friend-pictures-screen";

/** The words TASK 0791 names, exactly as the screen must show them. */
export const friendPicturesRequiredWords: readonly string[] = [
  "Friend pictures",
  "Own picture",
  "Hide pictures",
  "Save",
];

export function checkFriendPicturesWords(markup: string): ScreenWordCheck {
  return checkScreenWords(markup, {
    title: FRIEND_PICTURES_SCREEN_TITLE,
    requiredWords: friendPicturesRequiredWords,
  });
}
