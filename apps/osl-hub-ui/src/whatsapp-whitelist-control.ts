import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./preferences";

/** The WhatsApp kind that can acquire this reciprocal inline OSL control. */
export const WHATSAPP_DIRECT_MESSAGE_KIND = "direct_message";
export const WHATSAPP_CHANNEL_KIND = "channel";

/** The narrow native shape read before rendering a WhatsApp place control. */
export interface WhatsAppAllowedPlace {
  app: "whatsapp";
  account: string;
  kind: string;
  stableId: string;
  personName: string;
  placeName: string;
  allowed: boolean;
}

export type WhatsAppPlaceReader = () => WhatsAppAllowedPlace | null | undefined;

export interface WhatsAppPlaceInspection {
  kind: string;
  controls: string;
}

export interface WhatsAppDirectMessageWhitelistState {
  app: "whatsapp";
  kind: "direct_message";
  firstAccount: string;
  secondAccount: string;
  firstToSecondStableId: string;
  secondToFirstStableId: string;
  firstToSecondAllowed: boolean;
  secondToFirstAllowed: boolean;
  state: "none" | "one-way" | "two-way";
}

function exact(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(value);
  return actual.length === keys.length && keys.every((key) => Object.hasOwn(value, key));
}

function nonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.length > 0 && !value.includes("\0");
}

/**
 * A control is eligible only when the saved direction facts and their derived
 * state agree.  Treat a widened or inconsistent receipt as unallowed.
 */
export function parseWhatsAppDirectMessageWhitelistState(raw: unknown): WhatsAppDirectMessageWhitelistState | null {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const value = raw as Record<string, unknown>;
  const keys = [
    "app", "kind", "firstAccount", "secondAccount", "firstToSecondStableId",
    "secondToFirstStableId", "firstToSecondAllowed", "secondToFirstAllowed", "state",
  ];
  if (!exact(value, keys)
    || value.app !== "whatsapp"
    || value.kind !== WHATSAPP_DIRECT_MESSAGE_KIND
    || ![value.firstAccount, value.secondAccount, value.firstToSecondStableId, value.secondToFirstStableId].every(nonEmptyString)
    || typeof value.firstToSecondAllowed !== "boolean"
    || typeof value.secondToFirstAllowed !== "boolean"
    || !["none", "one-way", "two-way"].includes(String(value.state))) return null;

  const savedDirections = Number(value.firstToSecondAllowed) + Number(value.secondToFirstAllowed);
  if ((savedDirections === 0 && value.state !== "none")
    || (savedDirections === 1 && value.state !== "one-way")
    || (savedDirections === 2 && value.state !== "two-way")) return null;
  return value as unknown as WhatsAppDirectMessageWhitelistState;
}

export function whatsappDirectMessageIsAllowed(state: WhatsAppDirectMessageWhitelistState | null): boolean {
  return state?.state === "two-way" && state.firstToSecondAllowed && state.secondToFirstAllowed;
}

/**
 * Deliberately returns no OSL-owned DOM for an unallowed direct message.  The
 * tick is inside the control so it cannot be displayed independently of the
 * two-way allowance that earned it.
 */
export function whatsappDirectMessageControlMarkup(state: WhatsAppDirectMessageWhitelistState | null): string {
  if (!whatsappDirectMessageIsAllowed(state)) return "";
  return '<button class="osl-whatsapp-direct-control" type="button" data-osl-whatsapp-control="protected-direct-message" aria-label="OSL protection controls"><span class="osl-verification-tick" data-osl-verification-tick="visible" aria-label="Verified two-way OSL allowance">✓</span><span>OSL</span></button>';
}

/** Channels have their own allowance surface and never inherit DM controls. */
export function whatsappChannelControlsVisible(place: WhatsAppAllowedPlace): boolean {
  const stableIdPrefix = `whatsapp:${place.account}:${WHATSAPP_CHANNEL_KIND}:`;
  return place.app === "whatsapp"
    && place.kind === WHATSAPP_CHANNEL_KIND
    && place.allowed
    && nonEmptyString(place.account)
    && place.stableId.startsWith(stableIdPrefix)
    && place.stableId.length > stableIdPrefix.length;
}

/** Render the channel-scoped control only after directly reading an allowed channel. */
export function whatsappChannelControlsMarkup(place: WhatsAppAllowedPlace): string {
  if (!whatsappChannelControlsVisible(place)) return "";
  return `<section class="osl-whatsapp-channel-controls" data-osl-whatsapp-channel-controls data-osl-whatsapp-place-id="${place.stableId}" aria-label="WhatsApp channel protection"><label><input type="checkbox" data-osl-whatsapp-channel-toggle="${place.stableId}" checked/> Allow OSL in ${place.placeName}</label></section>`;
}

/**
 * The reader is the authority for the current place. A channel gets only its
 * channel controls; every other kind is deliberately silent on this path.
 */
export function inspectWhatsAppAllowedPlace(readPlace: WhatsAppPlaceReader): WhatsAppPlaceInspection | null {
  const place = readPlace();
  if (!place) return null;
  return { kind: place.kind, controls: whatsappChannelControlsMarkup(place) };
}

export async function loadWhatsAppDirectMessageWhitelistState(
  firstAccount: string,
  secondAccount: string,
): Promise<WhatsAppDirectMessageWhitelistState | null> {
  if (!isTauriRuntime() || !nonEmptyString(firstAccount) || !nonEmptyString(secondAccount)) return null;
  const raw = await invoke<unknown>("compare_allowed_place_direction_state", {
    app: "whatsapp",
    kind: WHATSAPP_DIRECT_MESSAGE_KIND,
    firstAccount,
    secondAccount,
  });
  return parseWhatsAppDirectMessageWhitelistState(raw);
}
