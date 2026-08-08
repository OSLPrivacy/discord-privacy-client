/** TASK 0843 — banned-word and plain-English check for the Friend page. */

import { checkScreenWords, type ScreenWordCheck } from "./settings-home-words";
import { FRIEND_PAGE_TITLE } from "./friend-page";

export const friendPageRequiredWords: readonly string[] = [
  "Friend",
  "Message",
  "Pictures",
  "Privacy",
  "Remove friend",
  "Block",
];

export function checkFriendPageWords(markup: string): ScreenWordCheck {
  return checkScreenWords(markup, {
    title: FRIEND_PAGE_TITLE,
    requiredWords: friendPageRequiredWords,
  });
}
