import { checkScreenWords, type ScreenWordCheck } from "./settings-home-words";

/** The words TASK 0795 requires on the Account screen. */
export const accountScreenRequiredWords: readonly string[] = [
  "Account",
  "Profile",
  "Recovery",
  "Sign out",
  "Delete account",
  "Save",
];

export function checkAccountScreenWords(markup: string): ScreenWordCheck {
  return checkScreenWords(markup, { title: "Account", requiredWords: accountScreenRequiredWords });
}
