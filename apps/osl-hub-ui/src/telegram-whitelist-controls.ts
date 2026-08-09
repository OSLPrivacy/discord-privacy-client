import { invoke } from "@tauri-apps/api/core";

/**
 * Context-bound controls for a native-recognized Telegram place. Discovery
 * alone never exposes an OSL action in a Telegram conversation.
 */

export const ADD_ALLOWED_PLACE_COMMAND = "add_allowed_place_record";
export const REMOVE_ALLOWED_PLACE_COMMAND = "remove_allowed_place_record";
export const COMPARE_ALLOWED_PLACE_COMMAND = "compare_allowed_place_direction_state";

export interface TelegramAllowedPlace {
  app: string;
  account: string;
  kind: string;
  stableId: string;
  personName: string;
  placeName: string;
  allowed: boolean;
}

export interface TelegramVerificationState {
  app: "telegram";
  kind: "direct_message";
  firstAccount: string;
  secondAccount: string;
  firstToSecondStableId: string;
  secondToFirstStableId: string;
  firstToSecondAllowed: boolean;
  secondToFirstAllowed: boolean;
  state: "none" | "one-way" | "two-way";
}

export interface TelegramWhitelistDependencies {
  invoke(command: string, args: Record<string, unknown>): Promise<unknown>;
}

/** The narrow native bridge used to read the Telegram place currently on screen. */
export type TelegramPlaceReader = () => TelegramAllowedPlace | null | undefined;

/** What the UI may use after it has directly read the current Telegram place. */
export interface TelegramPlaceInspection {
  kind: string;
  controls: string;
}

const nativeDependencies: TelegramWhitelistDependencies = { invoke };

function escapeHtml(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;").replace(/'/gu, "&#39;");
}

function directMessageStableId(firstAccount: string, secondAccount: string): string {
  return `telegram:${firstAccount}:direct_message:${secondAccount}`;
}

/** Only an already-allowed Telegram direct message may expose OSL controls. */
export function telegramDirectMessageControlsVisible(place: TelegramAllowedPlace): boolean {
  const stableIdPrefix = directMessageStableId(place.account, "");
  return place.app === "telegram"
    && place.kind === "direct_message"
    && place.allowed
    && place.account.length > 0
    && place.stableId.startsWith(stableIdPrefix)
    && place.stableId.length > stableIdPrefix.length;
}

/** The tick means exactly that both accounts saved the reciprocal allowance. */
export function telegramVerificationTicked(state: TelegramVerificationState | null): boolean {
  return state !== null
    && state.app === "telegram"
    && state.kind === "direct_message"
    && state.firstToSecondAllowed
    && state.secondToFirstAllowed
    && state.state === "two-way"
    && state.firstToSecondStableId === directMessageStableId(state.firstAccount, state.secondAccount)
    && state.secondToFirstStableId === directMessageStableId(state.secondAccount, state.firstAccount);
}

export function telegramWhitelistControlsMarkup(
  place: TelegramAllowedPlace,
  verification: TelegramVerificationState | null,
): string {
  if (!telegramDirectMessageControlsVisible(place)) return "";
  const ticked = telegramVerificationTicked(verification);
  const stableId = escapeHtml(place.stableId);
  const peer = escapeHtml(place.personName);
  return `<section class="telegram-whitelist-controls" data-telegram-whitelist-controls data-telegram-place-id="${stableId}" aria-label="Telegram direct message protection">`
    + `<label><input type="checkbox" data-telegram-whitelist-toggle="${stableId}" checked/> Allow OSL in this direct message with ${peer}</label>`
    + `<span class="telegram-whitelist-verification" data-telegram-verification-tick="${ticked ? "visible" : "hidden"}" role="status">${ticked ? "✓ Both people have allowed this direct message" : "Waiting for the other person to allow this direct message"}</span>`
    + `</section>`;
}

/** Saved Messages is an owner-only place and never inherits reciprocal DM controls. */
export function telegramSavedMessagesControlsVisible(place: TelegramAllowedPlace): boolean {
  const stableIdPrefix = `telegram:${place.account}:saved_messages:`;
  return place.app === "telegram"
    && place.kind === "saved_messages"
    && place.allowed
    && place.account.length > 0
    && place.stableId.startsWith(stableIdPrefix)
    && place.stableId.length > stableIdPrefix.length;
}

/** Render only the owner-scoped allowance for an allowed Saved Messages place. */
export function telegramSavedMessagesControlsMarkup(place: TelegramAllowedPlace): string {
  if (!telegramSavedMessagesControlsVisible(place)) return "";
  const stableId = escapeHtml(place.stableId);
  return `<section class="telegram-saved-messages-controls" data-telegram-saved-messages-controls data-telegram-place-id="${stableId}" aria-label="Telegram Saved Messages protection">`
    + `<label><input type="checkbox" data-telegram-saved-messages-toggle="${stableId}" checked/> Allow OSL in ${escapeHtml(place.placeName)}</label>`
    + `</section>`;
}

/**
 * Read the current Telegram place before choosing controls. Saved Messages is
 * routed exclusively to its owner-only surface, never to reciprocal DM UI.
 */
export function inspectTelegramAllowedPlace(
  readPlace: TelegramPlaceReader,
  verification: TelegramVerificationState | null,
): TelegramPlaceInspection | null {
  const place = readPlace();
  if (!place) return null;
  const controls = place.kind === "saved_messages"
    ? telegramSavedMessagesControlsMarkup(place)
    : telegramWhitelistControlsMarkup(place, verification);
  return { kind: place.kind, controls };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function parseVerificationState(raw: unknown): TelegramVerificationState | null {
  if (!isRecord(raw)
    || raw.app !== "telegram"
    || raw.kind !== "direct_message"
    || typeof raw.firstAccount !== "string"
    || typeof raw.secondAccount !== "string"
    || typeof raw.firstToSecondStableId !== "string"
    || typeof raw.secondToFirstStableId !== "string"
    || typeof raw.firstToSecondAllowed !== "boolean"
    || typeof raw.secondToFirstAllowed !== "boolean"
    || !["none", "one-way", "two-way"].includes(String(raw.state))) return null;
  const state = raw as unknown as TelegramVerificationState;
  const savedDirections = Number(state.firstToSecondAllowed) + Number(state.secondToFirstAllowed);
  const expectedState = savedDirections === 2 ? "two-way" : savedDirections === 1 ? "one-way" : "none";
  return state.state === expectedState ? state : null;
}

/** Read the reciprocal native state; rejected or malformed responses fail closed. */
export async function loadTelegramVerificationState(
  place: TelegramAllowedPlace,
  peerAccount: string,
  dependencies: TelegramWhitelistDependencies = nativeDependencies,
): Promise<TelegramVerificationState | null> {
  if (!telegramDirectMessageControlsVisible(place) || !peerAccount) return null;
  try {
    const state = parseVerificationState(await dependencies.invoke(COMPARE_ALLOWED_PLACE_COMMAND, {
      app: "telegram",
      kind: "direct_message",
      firstAccount: place.account,
      secondAccount: peerAccount,
    }));
    if (!state || state.firstAccount !== place.account || state.secondAccount !== peerAccount) return null;
    return telegramVerificationTicked(state) || state.state !== "two-way" ? state : null;
  } catch {
    return null;
  }
}

/** Persist the exact Telegram DM selected by the visible checked control. */
export async function setTelegramDirectMessageAllowed(
  place: TelegramAllowedPlace,
  allowed: boolean,
  dependencies: TelegramWhitelistDependencies = nativeDependencies,
): Promise<boolean> {
  if (!telegramDirectMessageControlsVisible(place)) return false;
  if (allowed) {
    await dependencies.invoke(ADD_ALLOWED_PLACE_COMMAND, { record: {
      app: "telegram",
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
