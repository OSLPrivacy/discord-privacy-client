/**
 * Context-bound Messenger direct-message allow controls. Group chats and
 * communities never receive these controls, and an unallowed place renders
 * nothing so stale or malformed discovery cannot broaden access.
 */

export const ADD_ALLOWED_PLACE_COMMAND = "add_allowed_place_record";
export const REMOVE_ALLOWED_PLACE_COMMAND = "remove_allowed_place_record";
export const COMPARE_ALLOWED_PLACE_COMMAND = "compare_allowed_place_direction_state";

export interface MessengerAllowedPlace {
  app: string;
  account: string;
  kind: string;
  stableId: string;
  personName: string;
  placeName: string;
  allowed: boolean;
}

export interface MessengerVerificationState {
  app: string;
  kind: string;
  firstAccount: string;
  secondAccount: string;
  firstToSecondAllowed: boolean;
  secondToFirstAllowed: boolean;
  state: "none" | "one-way" | "two-way";
  verificationTicked: boolean;
}

export interface MessengerWhitelistDependencies {
  invoke(command: string, args: Record<string, unknown>): Promise<unknown>;
}

function escapeHtml(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;").replace(/'/gu, "&#39;");
}

/** Only a native-recognized, already-allowed Messenger DM may show controls. */
export function messengerDirectMessageControlsVisible(place: MessengerAllowedPlace): boolean {
  return place.app === "messenger" && place.kind === "direct_message" && place.allowed;
}

/** The tick is fail-closed and means exactly two reciprocal saved allowances. */
export function messengerVerificationTicked(state: MessengerVerificationState): boolean {
  return state.app === "messenger"
    && state.kind === "direct_message"
    && state.firstToSecondAllowed
    && state.secondToFirstAllowed
    && state.state === "two-way"
    && state.verificationTicked;
}

export function messengerWhitelistControlsMarkup(
  place: MessengerAllowedPlace,
  verification: MessengerVerificationState,
): string {
  if (!messengerDirectMessageControlsVisible(place)) return "";
  const ticked = messengerVerificationTicked(verification);
  const peer = escapeHtml(place.personName);
  return `<section class="messenger-whitelist-controls" data-messenger-whitelist-controls data-messenger-place-id="${escapeHtml(place.stableId)}" aria-label="Messenger direct message protection">`
    + `<label><input type="checkbox" data-messenger-whitelist-toggle="${escapeHtml(place.stableId)}" checked/> Allow OSL in this direct message with ${peer}</label>`
    + `<span class="messenger-whitelist-verification" data-messenger-verification-tick="${ticked ? "visible" : "hidden"}" aria-live="polite">${ticked ? "✓ Both people have allowed this direct message" : "Waiting for the other person to allow this direct message"}</span>`
    + `</section>`;
}

/** Only an already-allowed Messenger community may render community controls. */
export function messengerCommunityControlsVisible(place: MessengerAllowedPlace): boolean {
  return place.app === "messenger" && place.kind === "community" && place.allowed;
}

/**
 * Community controls are deliberately a distinct surface from DM controls.
 * A group chat must not inherit community scope merely because it is allowed.
 */
export function messengerCommunityControlsMarkup(place: MessengerAllowedPlace): string {
  if (!messengerCommunityControlsVisible(place)) return "";
  const community = escapeHtml(place.placeName);
  return `<section class="messenger-community-controls" data-messenger-community-controls data-messenger-place-id="${escapeHtml(place.stableId)}" aria-label="Messenger community protection">`
    + `<label><input type="checkbox" data-messenger-community-toggle="${escapeHtml(place.stableId)}" checked/> Allow OSL in ${community}</label>`
    + `</section>`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function verificationState(raw: unknown): MessengerVerificationState | null {
  if (!isRecord(raw)
    || raw.app !== "messenger" || raw.kind !== "direct_message"
    || typeof raw.firstAccount !== "string" || typeof raw.secondAccount !== "string"
    || typeof raw.firstToSecondAllowed !== "boolean" || typeof raw.secondToFirstAllowed !== "boolean"
    || typeof raw.verificationTicked !== "boolean"
    || !["none", "one-way", "two-way"].includes(String(raw.state))) return null;
  return raw as unknown as MessengerVerificationState;
}

/** Reads reciprocal native state; malformed responses fail closed with no tick. */
export async function loadMessengerVerificationState(
  place: MessengerAllowedPlace,
  peerAccount: string,
  dependencies: MessengerWhitelistDependencies,
): Promise<MessengerVerificationState | null> {
  if (!messengerDirectMessageControlsVisible(place) || !peerAccount) return null;
  return verificationState(await dependencies.invoke(COMPARE_ALLOWED_PLACE_COMMAND, {
    app: "messenger", kind: "direct_message", firstAccount: place.account, secondAccount: peerAccount,
  }));
}

/** Persists the exact allowed Messenger DM selected by the visible checkbox. */
export async function setMessengerDirectMessageAllowed(
  place: MessengerAllowedPlace,
  allowed: boolean,
  dependencies: MessengerWhitelistDependencies,
): Promise<boolean> {
  if (!messengerDirectMessageControlsVisible(place)) return false;
  if (allowed) {
    await dependencies.invoke(ADD_ALLOWED_PLACE_COMMAND, { record: {
      app: "messenger", account: place.account, kind: "direct_message", stableId: place.stableId,
      personName: place.personName, placeName: place.placeName,
    } });
  } else {
    await dependencies.invoke(REMOVE_ALLOWED_PLACE_COMMAND, { stableId: place.stableId });
  }
  return true;
}
