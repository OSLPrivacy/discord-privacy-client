/**
 * TASK 3426 - what the person is told when OSL cannot place the text.
 *
 * "Placing" is OSL putting the finished text into another app's message box
 * for the person (Discord, Telegram, Signal, WhatsApp, a browser compose box).
 * TASK 3412 (gate) settled what happens underneath when the quick front-window
 * grab stops working: try the grab, and after two failures fall back to waiting
 * for the person to bring the app forward. This module is the other half --
 * what the screen says when there is no route left and the text was NOT placed.
 *
 * Three product rules, all executable here rather than left to each caller:
 *
 * 1. Never fail quietly. A failed placement always produces a notice and always
 *    hands it to the sink the caller supplied, so a caller that ignores the
 *    return value still puts something on the screen. Anything the placer
 *    returns that is not an explicit `{ placed: true }` -- `null`, `undefined`,
 *    a thrown error, a malformed object -- counts as a failure. Failing closed
 *    matters more than trusting a shape.
 * 2. Say it in plain words, and name the app. "Placement failed" tells a person
 *    nothing; "OSL could not put your message in Discord" does. The sentence
 *    that the text was not sent anywhere is a separate, fixed sentence so no
 *    caller can paraphrase it away.
 * 3. Never leave the private text in a box the person cannot see. On failure
 *    the private box is restored to the person's own text (it is on screen, in
 *    front of them) and the other app's box is emptied -- including a partial
 *    write that landed before the placer gave up. A half-typed private message
 *    sitting in Discord's box is the exact harm this whole area exists to
 *    prevent.
 *
 * Nothing here sends, copies, or clears the clipboard. It reports and it tidies
 * the two boxes. The backend's own refusal string is passed through
 * `sanitizeBackendMessage` with the private text as a content argument, so a
 * refusal that echoed the draft back cannot re-print the draft inside the
 * notice.
 */

import { sanitizeBackendMessage } from "./backend-failure";

/** A textarea or input, narrowed to the one property this module touches. */
export interface EditableBox {
  value: string;
}

/** Why placing stopped. Each maps to one plain sentence, never a code. */
export type PlacementFailureCause =
  | "windowNotAvailable"
  | "messageBoxNotFound"
  | "appRefused"
  | "placerUnavailable";

export const PLACEMENT_FAILURE_CAUSES: readonly PlacementFailureCause[] = [
  "windowNotAvailable",
  "messageBoxNotFound",
  "appRefused",
  "placerUnavailable",
];

/**
 * The one sentence that says nothing left the device. Exported so callers and
 * tests assert the exact words rather than a paraphrase of them.
 */
export const PLACEMENT_NOT_SENT_SENTENCE = "Your message was not sent anywhere.";

/** Used only when a caller could not name the app; the notice still appears. */
export const UNNAMED_APP = "the other app";

/** Bounded so a hostile window title cannot fill the sheet. */
const MAX_APP_NAME_CHARS = 64;

export interface PlacementFailureNotice {
  /** The app OSL was placing into, as shown to the person. */
  readonly appName: string;
  readonly cause: PlacementFailureCause;
  /** "OSL could not put your message in Discord." */
  readonly headline: string;
  /** One plain sentence for `cause`. */
  readonly reason: string;
  /** Always `PLACEMENT_NOT_SENT_SENTENCE`. */
  readonly notSent: string;
  /** Where the person's text is now, and that the app's box was left empty. */
  readonly whereYourTextIs: string;
  /** The backend's own words, redacted and bounded. `""` when there were none. */
  readonly detail: string;
  /** The four sentences above, in reading order, as one string. */
  readonly message: string;
}

export type PlacementOutcome =
  | { readonly placed: true }
  | { readonly placed: false; readonly cause?: PlacementFailureCause; readonly error?: unknown };

export interface PlacementRequest {
  /** The app being placed into, e.g. "Discord". */
  readonly appName: string;
  /** What the person wrote. Restored into the private box if placing fails. */
  readonly privateText: string;
  /** OSL's own box, the one the person can see. */
  readonly privateBox: EditableBox;
  /** The other app's message box. Left empty when placing fails. */
  readonly otherAppBox: EditableBox;
  /** The placer. Anything but an explicit `{ placed: true }` is a failure. */
  readonly place: () => PlacementOutcome | null | undefined | Promise<PlacementOutcome | null | undefined>;
  /** Where the notice goes on screen. Called with `null` when placing worked. */
  readonly show: (notice: PlacementFailureNotice | null) => void;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

/**
 * A window or process name reaches this from the operating system, so it is
 * flattened to one short line before it is put in a sentence. It is never
 * emptied into silence: a blank name becomes `UNNAMED_APP` so the notice still
 * reads as a sentence.
 */
export function placementAppName(value: unknown): string {
  if (typeof value !== "string") return UNNAMED_APP;
  const flattened = value
    .replace(/[\u0000-\u001f\u007f\u202a-\u202e\u2066-\u2069]+/gu, " ")
    .replace(/\s+/gu, " ")
    .trim();
  if (!flattened) return UNNAMED_APP;
  const characters = Array.from(flattened);
  return characters.length <= MAX_APP_NAME_CHARS
    ? flattened
    : `${characters.slice(0, MAX_APP_NAME_CHARS).join("")}…`;
}

function reasonSentence(cause: PlacementFailureCause, appName: string): string {
  if (cause === "windowNotAvailable") return `${appName} did not come to the front, so there was nowhere to put it.`;
  if (cause === "messageBoxNotFound") return `OSL could not find the message box in ${appName}.`;
  if (cause === "appRefused") return `${appName} would not accept the text.`;
  return "The part of OSL that types into other apps did not answer.";
}

function isPlacementFailureCause(value: unknown): value is PlacementFailureCause {
  return typeof value === "string" && PLACEMENT_FAILURE_CAUSES.includes(value as PlacementFailureCause);
}

/**
 * Build the notice. `rawError` is whatever the placer said; `privateText` is
 * passed only so any fragment of it that the error echoed back is redacted out
 * of `detail` before the notice reaches the screen.
 */
export function placementFailureNotice(
  appName: unknown,
  cause: PlacementFailureCause = "placerUnavailable",
  rawError: unknown = "",
  privateText = "",
): PlacementFailureNotice {
  const name = placementAppName(appName);
  const safeCause = isPlacementFailureCause(cause) ? cause : "placerUnavailable";
  const headline = `OSL could not put your message in ${name}.`;
  const reason = reasonSentence(safeCause, name);
  const whereYourTextIs = `Your text is still in the OSL box below. Nothing was left in ${name}.`;
  const detail = sanitizeBackendMessage(rawError, [privateText], "");
  return {
    appName: name,
    cause: safeCause,
    headline,
    reason,
    notSent: PLACEMENT_NOT_SENT_SENTENCE,
    whereYourTextIs,
    detail,
    message: `${headline} ${reason} ${PLACEMENT_NOT_SENT_SENTENCE} ${whereYourTextIs}`,
  };
}

/**
 * The notice as it appears in the protected sheet. `role="alert"` because this
 * is the one thing on the screen the person must not miss, and the app name is
 * carried in a data attribute as well as the sentence so a QA run can read it
 * without parsing prose.
 */
export function placementFailureNoticeMarkup(notice: PlacementFailureNotice | null): string {
  if (!notice) return "";
  const detail = notice.detail
    ? `<p class="placement-failure-detail">${escapeHtml(notice.detail)}</p>`
    : "";
  return `<section class="placement-failure" role="alert" data-placement-failure="${escapeHtml(notice.cause)}" data-placement-app="${escapeHtml(notice.appName)}">`
    + `<p class="placement-failure-headline">${escapeHtml(notice.headline)}</p>`
    + `<p class="placement-failure-reason">${escapeHtml(notice.reason)}</p>`
    + `<p class="placement-failure-not-sent" data-placement-not-sent>${escapeHtml(notice.notSent)}</p>`
    + `<p class="placement-failure-where">${escapeHtml(notice.whereYourTextIs)}</p>`
    + detail
    + `</section>`;
}

/**
 * Put the person's text into the other app's box, or explain why not.
 *
 * Returns `null` when it was placed, and the notice when it was not. Either
 * way `show` has already been called, so a caller that drops the return value
 * cannot turn a failure into silence.
 */
export async function placeProtectedTextOrExplain(
  request: PlacementRequest,
): Promise<PlacementFailureNotice | null> {
  const { appName, privateText, privateBox, otherAppBox, place, show } = request;
  let cause: PlacementFailureCause = "placerUnavailable";
  let rawError: unknown = "";
  let placed = false;
  try {
    const outcome = await place();
    if (outcome && outcome.placed === true) {
      placed = true;
    } else if (outcome && outcome.placed === false) {
      cause = isPlacementFailureCause(outcome.cause) ? outcome.cause : "appRefused";
      rawError = outcome.error ?? "";
    }
    // Anything else -- null, undefined, a shape that is neither -- keeps the
    // "placerUnavailable" default. An unreadable answer is not a placement.
  } catch (error) {
    cause = "appRefused";
    rawError = error;
  }
  if (placed) {
    show(null);
    return null;
  }
  // Fail closed, in this order: the person's text back where they can see it,
  // then the other app's box emptied of anything a half-finished write left.
  privateBox.value = privateText;
  otherAppBox.value = "";
  const notice = placementFailureNotice(appName, cause, rawError, privateText);
  show(notice);
  return notice;
}
