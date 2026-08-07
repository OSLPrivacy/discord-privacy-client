import type { AccountSecrets, AccountSettings } from "./account-screen";

/**
 * Fixed data for the Linux Account screenshot (TASK 0792).
 *
 * The secrets below are the whole point of the capture. They are real strings
 * of the shape the account actually holds -- a sign-in password, a twelve-word
 * recovery phrase, a Pro activation code, and the two role passwords -- and the
 * fixture hands every one of them to the screen. The check then searches what
 * the capture produced for each of them and requires nothing back. A fixture
 * with empty secrets would make that zero meaningless, so `accountSecretFacts`
 * refuses to build a view from one.
 *
 * The words are chosen so that no six-character run of any secret is a run of
 * the screen's own copy either: the search is for partial leaks as well as
 * whole ones, and "the last four of your code" is exactly the kind of leak a
 * whole-string search walks past.
 */
export const ACCOUNT_SCREEN_WINDOW = { width: 1280, height: 800 } as const;

export const ACCOUNT_SCREEN_SECRETS: AccountSecrets = {
  password: "Th3-brass-lantern-9Fq",
  recoveryPhrase:
    "amber cabin delta frost harbor ivory juniper lantern meadow nickel orbit quartz",
  proCode: "OSL-7QK4-2M9X-5RTB-8WZC",
  stealthPassword: "Qv7-marsh-thimble-2K",
  burnPassword: "Zc4-copper-hinge-8Tn",
} as const;

/**
 * The screen opens away from its defaults on every control that has one, so a
 * Reset that did nothing would show as a Reset that did nothing: the shown name
 * is not the handle, and the lock delay is thirty minutes rather than five.
 */
export const ACCOUNT_SCREEN_SETTINGS: AccountSettings = {
  displayName: "Nora Vale",
  handle: "nora.vale.0324",
  lockMinutes: 30,
  passwordChanged: "12 January 2026",
  recoverySaved: "4 March 2026",
  proCodeEntered: "18 February 2026",
  proPlan: "Pro, this device",
} as const;

/** The three the finish line names, in the order it names them. */
export const ACCOUNT_SCREEN_NAMED_SECRETS: readonly (keyof AccountSecrets)[] = [
  "password",
  "recoveryPhrase",
  "proCode",
] as const;

/**
 * The strings a capture of this screen must not contain, for one secret.
 *
 * Searching for the whole value only catches a screen that printed all of it,
 * and the leak worth worrying about is the helpful one -- the last four of the
 * code, the first two words of the kit. So a one-piece secret contributes every
 * six-character run of itself. A phrase contributes its words instead: runs
 * across its spaces are not evidence of anything, because "delta frost" holds
 * "ta fro" and so does the screen's own line about OSL data from this device.
 * Words shorter than six characters are left out for the same reason.
 */
export function accountSecretProbes(secret: string): string[] {
  const trimmed = secret.trim();
  const probes = new Set<string>();
  if (trimmed.length === 0) return [];
  probes.add(trimmed);
  const parts = trimmed.split(/\s+/u);
  if (parts.length > 1) {
    for (const word of parts) if (word.length >= 6) probes.add(word);
    return [...probes];
  }
  for (let index = 0; index + 6 <= trimmed.length; index += 1) {
    probes.add(trimmed.slice(index, index + 6));
  }
  return [...probes];
}
