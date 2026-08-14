/**
 * Context-bound Instagram direct-message allow controls. Group chats and
 * public posts never receive these controls, and an unallowed place renders
 * nothing so stale or malformed discovery cannot broaden access.
 */

export const ADD_ALLOWED_PLACE_COMMAND = "add_allowed_place_record";
export const REMOVE_ALLOWED_PLACE_COMMAND = "remove_allowed_place_record";
export const COMPARE_ALLOWED_PLACE_COMMAND = "compare_allowed_place_direction_state";

export interface InstagramAllowedPlace {
  app: string;
  account: string;
  kind: string;
  stableId: string;
  personName: string;
  placeName: string;
  allowed: boolean;
}

export interface InstagramVerificationState {
  app: string;
  kind: string;
  firstAccount: string;
  secondAccount: string;
  firstToSecondAllowed: boolean;
  secondToFirstAllowed: boolean;
  savedDirections: number;
  verificationState: "hidden" | "visible";
  state: "none" | "one-way" | "two-way";
}

export interface InstagramWhitelistDependencies {
  invoke(command: string, args: Record<string, unknown>): Promise<unknown>;
}

/** The narrow native bridge used to read the Instagram place currently on screen. */
export type InstagramPlaceReader = () => InstagramAllowedPlace | null | undefined;

/** What the UI may use after it has directly read the current Instagram place. */
export interface InstagramPlaceInspection {
  kind: string;
  controls: string;
}
function escapeHtml(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;").replace(/'/gu, "&#39;");
}

/** Only a native-recognized, already-allowed Instagram DM may show controls. */
export function instagramDirectMessageControlsVisible(place: InstagramAllowedPlace): boolean {
  return place.app === "instagram" && place.kind === "direct_message" && place.allowed;
}

/** The tick is fail-closed and means exactly two reciprocal saved allowances. */
export function instagramVerificationTicked(state: InstagramVerificationState): boolean {
  return state.app === "instagram"
    && state.kind === "direct_message"
    && state.firstToSecondAllowed
    && state.secondToFirstAllowed
    && state.savedDirections === 2
    && state.verificationState === "visible"
    && state.state === "two-way";
}

export function instagramWhitelistControlsMarkup(
  place: InstagramAllowedPlace,
  verification: InstagramVerificationState,
): string {
  if (!instagramDirectMessageControlsVisible(place)) return "";
  const ticked = instagramVerificationTicked(verification);
  const peer = escapeHtml(place.personName);
  return `<section class="instagram-whitelist-controls" data-instagram-whitelist-controls data-instagram-place-id="${escapeHtml(place.stableId)}" aria-label="Instagram direct message protection">`
    + `<label><input type="checkbox" data-instagram-whitelist-toggle="${escapeHtml(place.stableId)}" checked/> Allow OSL in this direct message with ${peer}</label>`
    + `<span class="instagram-whitelist-verification" data-instagram-verification-tick="${ticked ? "visible" : "hidden"}" aria-live="polite">${ticked ? "✓ Both people have allowed this direct message" : "Waiting for the other person to allow this direct message"}</span>`
    + `</section>`;
}

/**
 * Reads the current place before deciding which controls may be rendered.
 * A missing reader result is not a place match and therefore renders nothing.
 */
export function inspectInstagramAllowedPlace(
  readPlace: InstagramPlaceReader,
  verification: InstagramVerificationState,
): InstagramPlaceInspection | null {
  const place = readPlace();
  if (!place) return null;
  return { kind: place.kind, controls: instagramWhitelistControlsMarkup(place, verification) };
}
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function verificationState(raw: unknown): InstagramVerificationState | null {
  if (!isRecord(raw)
    || raw.app !== "instagram" || raw.kind !== "direct_message"
    || typeof raw.firstAccount !== "string" || typeof raw.secondAccount !== "string"
    || typeof raw.firstToSecondAllowed !== "boolean" || typeof raw.secondToFirstAllowed !== "boolean"
    || !Number.isInteger(raw.savedDirections) || Number(raw.savedDirections) < 0 || Number(raw.savedDirections) > 2
    || !["hidden", "visible"].includes(String(raw.verificationState))
    || !["none", "one-way", "two-way"].includes(String(raw.state))) return null;
  return raw as unknown as InstagramVerificationState;
}

/** Reads reciprocal native state; malformed responses fail closed with no tick. */
export async function loadInstagramVerificationState(
  place: InstagramAllowedPlace,
  peerAccount: string,
  dependencies: InstagramWhitelistDependencies,
): Promise<InstagramVerificationState | null> {
  if (!instagramDirectMessageControlsVisible(place) || !peerAccount) return null;
  return verificationState(await dependencies.invoke(COMPARE_ALLOWED_PLACE_COMMAND, {
    app: "instagram", kind: "direct_message", firstAccount: place.account, secondAccount: peerAccount,
  }));
}

/** Persists the exact allowed Instagram DM selected by the visible checkbox. */
export async function setInstagramDirectMessageAllowed(
  place: InstagramAllowedPlace,
  allowed: boolean,
  dependencies: InstagramWhitelistDependencies,
): Promise<boolean> {
  if (!instagramDirectMessageControlsVisible(place)) return false;
  if (allowed) {
    await dependencies.invoke(ADD_ALLOWED_PLACE_COMMAND, { record: {
      app: "instagram", account: place.account, kind: "direct_message", stableId: place.stableId,
      personName: place.personName, placeName: place.placeName,
    } });
  } else {
    await dependencies.invoke(REMOVE_ALLOWED_PLACE_COMMAND, { stableId: place.stableId });
  }
  return true;
}
