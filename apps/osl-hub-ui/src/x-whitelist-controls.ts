/**
 * The small, context-bound X direct-message allow control.  X posts never get
 * these controls: OSL can only offer them after the current place has already
 * been identified as an allowed direct message by the native side.
 */

export const ADD_ALLOWED_PLACE_COMMAND = "add_allowed_place_record";
export const REMOVE_ALLOWED_PLACE_COMMAND = "remove_allowed_place_record";
export const COMPARE_ALLOWED_PLACE_COMMAND = "compare_allowed_place_direction_state";

export interface XAllowedPlace {
  app: string;
  account: string;
  kind: string;
  stableId: string;
  personName: string;
  placeName: string;
  allowed: boolean;
}

export interface XVerificationState {
  app: string;
  kind: string;
  firstAccount: string;
  secondAccount: string;
  firstToSecondAllowed: boolean;
  secondToFirstAllowed: boolean;
  state: "none" | "one-way" | "two-way";
}

export interface XWhitelistDependencies {
  invoke(command: string, args: Record<string, unknown>): Promise<unknown>;
}

function escapeHtml(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;").replace(/'/gu, "&#39;");
}

/** The only place that may render this X control. */
export function xDirectMessageControlsVisible(place: XAllowedPlace): boolean {
  return place.app === "x" && place.kind === "direct_message" && place.allowed;
}

/** A verification tick means that both named accounts saved the reciprocal DM allowance. */
export function xVerificationTicked(state: XVerificationState): boolean {
  return state.app === "x"
    && state.kind === "direct_message"
    && state.firstToSecondAllowed
    && state.secondToFirstAllowed
    && state.state === "two-way";
}

export function xWhitelistControlsMarkup(place: XAllowedPlace, verification: XVerificationState): string {
  if (!xDirectMessageControlsVisible(place)) return "";
  const ticked = xVerificationTicked(verification);
  const peer = escapeHtml(place.personName);
  return `<section class="x-whitelist-controls" data-x-whitelist-controls data-x-place-id="${escapeHtml(place.stableId)}" aria-label="X direct message protection">`
    + `<label><input type="checkbox" data-x-whitelist-toggle="${escapeHtml(place.stableId)}" checked/> Allow OSL in this direct message with ${peer}</label>`
    + `<span class="x-whitelist-verification" data-x-verification-tick="${ticked ? "visible" : "hidden"}" aria-live="polite">${ticked ? "✓ Both people have allowed this direct message" : "Waiting for the other person to allow this direct message"}</span>`
    + `</section>`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function verificationState(raw: unknown): XVerificationState | null {
  if (!isRecord(raw)
    || raw.app !== "x" || raw.kind !== "direct_message"
    || typeof raw.firstAccount !== "string" || typeof raw.secondAccount !== "string"
    || typeof raw.firstToSecondAllowed !== "boolean" || typeof raw.secondToFirstAllowed !== "boolean"
    || !["none", "one-way", "two-way"].includes(String(raw.state))) return null;
  return raw as unknown as XVerificationState;
}

/** Reads the native reciprocal state; malformed responses fail closed (no tick). */
export async function loadXVerificationState(
  place: XAllowedPlace,
  peerAccount: string,
  dependencies: XWhitelistDependencies,
): Promise<XVerificationState | null> {
  if (!xDirectMessageControlsVisible(place) || !peerAccount) return null;
  return verificationState(await dependencies.invoke(COMPARE_ALLOWED_PLACE_COMMAND, {
    app: "x", kind: "direct_message", firstAccount: place.account, secondAccount: peerAccount,
  }));
}

/** Persists the exact allowed place selected by the visible checkbox. */
export async function setXDirectMessageAllowed(
  place: XAllowedPlace,
  allowed: boolean,
  dependencies: XWhitelistDependencies,
): Promise<boolean> {
  if (!xDirectMessageControlsVisible(place)) return false;
  if (allowed) {
    await dependencies.invoke(ADD_ALLOWED_PLACE_COMMAND, { record: {
      app: "x", account: place.account, kind: "direct_message", stableId: place.stableId,
      personName: place.personName, placeName: place.placeName,
    } });
  } else {
    await dependencies.invoke(REMOVE_ALLOWED_PLACE_COMMAND, { stableId: place.stableId });
  }
  return true;
}
