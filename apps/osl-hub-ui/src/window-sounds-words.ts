/** TASK 0783 — the plain-English check for Window and sounds. */

import { checkScreenWords, type ScreenWordCheck } from "./settings-home-words";

export const windowSoundsRequiredWords: readonly string[] = [
  "Window and sounds",
  "Window size",
  "Alert sounds",
  "Message sound",
  "Save",
];

export function checkWindowSoundsWords(markup: string): ScreenWordCheck {
  return checkScreenWords(markup, {
    title: "Window and sounds",
    requiredWords: windowSoundsRequiredWords,
  });
}
