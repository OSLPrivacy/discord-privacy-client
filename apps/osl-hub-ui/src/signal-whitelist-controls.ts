import { invoke } from "@tauri-apps/api/core";

/**
 * Context-bound controls for a native-recognized Signal direct message.
 * Discovery alone is not enough to expose an OSL action: the current place
 * must already be an allowed Signal DM with a self-consistent stable ID.
 */

export const ADD_ALLOWED_PLACE_COMMAND = "add_allowed_place_record";
export const REMOVE_ALLOWED_PLACE_COMMAND = "remove_allowed_place_record";
export const COMPARE_ALLOWED_PLACE_COMMAND = "compare_allowed_place_direction_state";

export interface SignalAllowedPlace {
  app: string;
  account: string;
  kind: string;
  stableId: string;
  personName: string;
  placeName: string;
  allowed: boolean;
}

export interface SignalVerificationState {
  app: "signal";
  kind: "direct_message";
  firstAccount: string;
  secondAccount: string;
  firstToSecondStableId: string;
  secondToFirstStableId: string;
  firstToSecondAllowed: boolean;
  secondToFirstAllowed: boolean;
  state: "none" | "one-way" | "two-way";
  verificationTicked: boolean;
}

export interface SignalWhitelistDependencies {
  invoke(command: string, args: Record<string, unknown>): Promise<unknown>;
}

const nativeDependencies: SignalWhitelistDependencies = { invoke };

function escapeHtml(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;").replace(/'/gu, "&#39;");
}

function directMessageStableId(firstAccount: string, secondAccount: string): string {
  return `signal:${firstAccount}:direct_message:${secondAccount}`;
}

/** Only an already-allowed Signal direct message may expose OSL controls. */
export function signalDirectMessageControlsVisible(place: SignalAllowedPlace): boolean {
  const stableIdPrefix = directMessageStableId(place.account, "");
  return place.app === "signal"
    && place.kind === "direct_message"
    && place.allowed
    && place.account.length > 0
    && place.stableId.startsWith(stableIdPrefix)
    && place.stableId.length > stableIdPrefix.length;
}

/** A tick means exactly that both named Signal accounts saved reciprocal allowances. */
export function signalVerificationTicked(state: SignalVerificationState | null): boolean {
  return state !== null
    && state.app === "signal"
    && state.kind === "direct_message"
    && state.firstAccount !== state.secondAccount
    && state.firstToSecondAllowed
    && state.secondToFirstAllowed
    && state.state === "two-way"
    && state.verificationTicked
    && state.firstToSecondStableId === directMessageStableId(state.firstAccount, state.secondAccount)
    && state.secondToFirstStableId === directMessageStableId(state.secondAccount, state.firstAccount);
}

export function signalWhitelistControlsMarkup(
  place: SignalAllowedPlace,
  verification: SignalVerificationState | null,
): string {
  if (!signalDirectMessageControlsVisible(place)) return "";
  const ticked = signalVerificationTicked(verification);
  const stableId = escapeHtml(place.stableId);
  const peer = escapeHtml(place.personName);
  return `<section class="signal-whitelist-controls" data-signal-whitelist-controls data-signal-place-id="${stableId}" aria-label="Signal direct message protection">`
    + `<label><input type="checkbox" data-signal-whitelist-toggle="${stableId}" checked/> Allow OSL in this direct message with ${peer}</label>`
    + `<span class="signal-whitelist-verification" data-signal-verification-tick="${ticked ? "visible" : "hidden"}" role="status">${ticked ? "✓ Both people have allowed this direct message" : "Waiting for the other person to allow this direct message"}</span>`
    + `</section>`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function parseVerificationState(raw: unknown): SignalVerificationState | null {
  if (!isRecord(raw)
    || raw.app !== "signal"
    || raw.kind !== "direct_message"
    || typeof raw.firstAccount !== "string"
    || typeof raw.secondAccount !== "string"
    || typeof raw.firstToSecondStableId !== "string"
    || typeof raw.secondToFirstStableId !== "string"
    || typeof raw.firstToSecondAllowed !== "boolean"
    || typeof raw.secondToFirstAllowed !== "boolean"
    || typeof raw.verificationTicked !== "boolean"
    || !["none", "one-way", "two-way"].includes(String(raw.state))) return null;
  const state = raw as unknown as SignalVerificationState;
  const savedDirections = Number(state.firstToSecondAllowed) + Number(state.secondToFirstAllowed);
  const expectedState = savedDirections === 2 ? "two-way" : savedDirections === 1 ? "one-way" : "none";
  if (state.state !== expectedState || state.verificationTicked !== (savedDirections === 2)) return null;
  return state;
}

/** Read reciprocal native state; rejected or malformed responses fail closed. */
export async function loadSignalVerificationState(
  place: SignalAllowedPlace,
  peerAccount: string,
  dependencies: SignalWhitelistDependencies = nativeDependencies,
): Promise<SignalVerificationState | null> {
  if (!signalDirectMessageControlsVisible(place) || !peerAccount || peerAccount === place.account) return null;
  try {
    const state = parseVerificationState(await dependencies.invoke(COMPARE_ALLOWED_PLACE_COMMAND, {
      app: "signal",
      kind: "direct_message",
      firstAccount: place.account,
      secondAccount: peerAccount,
    }));
    if (!state || state.firstAccount !== place.account || state.secondAccount !== peerAccount) return null;
    return signalVerificationTicked(state) || state.state !== "two-way" ? state : null;
  } catch {
    return null;
  }
}

/** Persist the exact Signal DM selected by the visible checked control. */
export async function setSignalDirectMessageAllowed(
  place: SignalAllowedPlace,
  allowed: boolean,
  dependencies: SignalWhitelistDependencies = nativeDependencies,
): Promise<boolean> {
  if (!signalDirectMessageControlsVisible(place)) return false;
  if (allowed) {
    await dependencies.invoke(ADD_ALLOWED_PLACE_COMMAND, { record: {
      app: "signal",
      account: place.account,
      kind: "direct_message",
      stable_id: place.stableId,
      person_name: place.personName,
      place_name: place.placeName,
    } });
  } else {
    await dependencies.invoke(REMOVE_ALLOWED_PLACE_COMMAND, { stableId: place.stableId });
  }
  return true;
}
