import { invoke } from "@tauri-apps/api/core";

/**
 * Context-bound controls for a native-recognized Telegram place. Discovery
 * alone never exposes an OSL action in a Telegram conversation.
 */

export const ADD_ALLOWED_PLACE_COMMAND = "add_allowed_place_record";
export const REMOVE_ALLOWED_PLACE_COMMAND = "remove_allowed_place_record";
export const COMPARE_ALLOWED_PLACE_COMMAND = "compare_allowed_place_direction_state";

export type TelegramPlaceKind = "direct_message" | "group_chat" | "channel" | "public_post";

const TELEGRAM_ALLOWED_PLACE_KINDS: readonly TelegramPlaceKind[] = [
  "direct_message",
  "group_chat",
  "channel",
  "public_post",
];

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
  kind: TelegramPlaceKind;
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

/** What the UI may use after it has directly read an allowed Telegram place. */
export interface TelegramPlaceInspection {
  kind: TelegramPlaceKind | "saved_messages";
  controls: string;
}

const nativeDependencies: TelegramWhitelistDependencies = { invoke };

function escapeHtml(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;").replace(/'/gu, "&#39;");
}

function isTelegramPlaceKind(kind: string): kind is TelegramPlaceKind {
  return TELEGRAM_ALLOWED_PLACE_KINDS.some((candidate) => candidate === kind);
}

function placeStableId(account: string, kind: TelegramPlaceKind, placeId: string): string {
  return `telegram:${account}:${kind}:${placeId}`;
}

function placeKindLabel(kind: TelegramPlaceKind): string {
  switch (kind) {
    case "direct_message": return "direct message";
    case "group_chat": return "group chat";
    case "channel": return "channel";
    case "public_post": return "public post";
  }
}

/** Every recognized, already-allowed Telegram place kind may expose its allow control. */
export function telegramAllowedPlaceControlsVisible(place: TelegramAllowedPlace): boolean {
  if (place.app !== "telegram" || !place.allowed || !place.account || !isTelegramPlaceKind(place.kind)) {
    return false;
  }
  const stableIdPrefix = placeStableId(place.account, place.kind, "");
  return place.stableId.startsWith(stableIdPrefix) && place.stableId.length > stableIdPrefix.length;
}

/** Only an already-allowed Telegram direct message may expose OSL controls. */
export function telegramDirectMessageControlsVisible(place: TelegramAllowedPlace): boolean {
  return place.kind === "direct_message" && telegramAllowedPlaceControlsVisible(place);
}

/** The tick means exactly that both accounts saved the reciprocal allowance. */
export function telegramVerificationTicked(state: TelegramVerificationState | null): boolean {
  return state !== null
    && state.app === "telegram"
    && state.kind === "direct_message"
    && state.firstToSecondAllowed
    && state.secondToFirstAllowed
    && state.state === "two-way"
    && isTelegramPlaceKind(state.kind)
    && state.firstToSecondStableId === placeStableId(state.firstAccount, state.kind, state.secondAccount)
    && state.secondToFirstStableId === placeStableId(state.secondAccount, state.kind, state.firstAccount);
}

export function telegramWhitelistControlsMarkup(
  place: TelegramAllowedPlace,
  verification: TelegramVerificationState | null,
): string {
  if (!telegramAllowedPlaceControlsVisible(place)) return "";
  const kind = place.kind as TelegramPlaceKind;
  const kindLabel = placeKindLabel(kind);
  const ticked = telegramVerificationTicked(verification);
  const stableId = escapeHtml(place.stableId);
  const peer = escapeHtml(place.personName);
  const peerSuffix = peer ? ` with ${peer}` : "";
  const status = kind === "direct_message"
    ? (ticked ? "✓ Both people have allowed this direct message" : "Waiting for the other person to allow this direct message")
    : (ticked ? `✓ This ${kindLabel} is allowed both ways` : `This ${kindLabel} is allowed on this account`);
  return `<section class="telegram-whitelist-controls" data-telegram-whitelist-controls data-telegram-place-kind="${kind}" data-telegram-place-id="${stableId}" aria-label="Telegram ${kindLabel} protection">`
    + `<label><input type="checkbox" data-telegram-whitelist-toggle="${stableId}" checked/> Allow OSL in this ${kindLabel}${peerSuffix}</label>`
    + `<span class="telegram-whitelist-verification" data-telegram-verification-tick="${ticked ? "visible" : "hidden"}" role="status">${status}</span>`
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
 * Read the current allowed Telegram place before choosing controls. Saved
 * Messages is routed exclusively to its owner-only surface, while malformed or
 * unallowed direct/group/channel/public results fail closed.
 */
export function inspectTelegramAllowedPlace(
  readPlace: TelegramPlaceReader,
  verification: TelegramVerificationState | null,
): TelegramPlaceInspection | null {
  const place = readPlace();
  if (!place) return null;
  if (place.kind === "saved_messages") {
    const controls = telegramSavedMessagesControlsMarkup(place);
    return controls ? { kind: "saved_messages", controls } : null;
  }
  if (!telegramAllowedPlaceControlsVisible(place)) return null;
  return {
    kind: place.kind as TelegramPlaceKind,
    controls: telegramWhitelistControlsMarkup(place, verification),
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function parseVerificationState(raw: unknown): TelegramVerificationState | null {
  if (!isRecord(raw)
    || raw.app !== "telegram"
    || typeof raw.kind !== "string"
    || !isTelegramPlaceKind(raw.kind)
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
  if (!telegramAllowedPlaceControlsVisible(place) || !peerAccount) return null;
  try {
    const state = parseVerificationState(await dependencies.invoke(COMPARE_ALLOWED_PLACE_COMMAND, {
      app: "telegram",
      kind: place.kind,
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
  if (!telegramAllowedPlaceControlsVisible(place)) return false;
  if (allowed) {
    await dependencies.invoke(ADD_ALLOWED_PLACE_COMMAND, { record: {
      app: "telegram",
      account: place.account,
      kind: place.kind,
      stable_id: place.stableId,
      person_name: place.personName,
      place_name: place.placeName,
    } });
  } else {
    await dependencies.invoke(REMOVE_ALLOWED_PLACE_COMMAND, { stableId: place.stableId });
  }
  return true;
}
