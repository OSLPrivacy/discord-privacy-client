import {
  parseDiscordVisualRecipe,
  type DiscordVisualRecipe,
} from "./discord-visual-recipe";
import {
  parseNativeDiscordCarrierRowBindings,
  type NativeDiscordCarrierRowBinding,
} from "./discord-carrier-row-binding";

export const MAX_PROTECTED_DRAFT_BYTES = 1024 * 1024;
export const PROTECTED_DRAFT_WARNING_BYTES = 900 * 1024;
export const NATIVE_OVERLAY_TTL_OPTIONS = [3_600, 86_400, 259_200, 604_800] as const;
export type NativeOverlayTtlSeconds = typeof NATIVE_OVERLAY_TTL_OPTIONS[number];

export interface NativeSurfaceCapture {
  version: "osl-native-surface-capture-v1";
  imageDataUrl: string;
  widthPx: number;
  heightPx: number;
  inputLeftPx: number;
  inputTopPx: number;
  inputWidthPx: number;
  inputHeightPx: number;
  inputBackground: string;
  textLeftPx: number;
  textTopPx: number;
  textWidthPx: number;
  textHeightPx: number;
  fontFamily: string | null;
  fontSizePx: number | null;
  fontWeight: number | null;
  lineHeightPx: number | null;
}

export interface NativeDiscordOverlayState {
  active: true;
  friendLabel: string;
  scopeApproved: true;
  ttlSeconds: NativeOverlayTtlSeconds;
  decryptDisplayEnabled: boolean;
  viewOnceEnabled: boolean;
  attachmentsEnabled: boolean;
  discordMarkerAvailable: boolean;
  covertextEnabled: boolean;
  /**
   * Whether the operator's typing is being encrypted, and therefore whether OSL
   * owns a composer over Discord's real message box.
   *
   * Optional, and absent means engaged: it is never a display question, so a
   * backend that does not report it must not be read as "display nothing".
   */
  lockEngaged?: boolean;
  visualRecipe?: DiscordVisualRecipe;
  nativeSurface?: NativeSurfaceCapture;
  visibleCarrierRows?: NativeDiscordCarrierRowBinding[];
}

export interface NativeDiscordOverlayPrepared {
  messageId: string;
  expiresAt: number;
  personToPersonE2ee: true;
  viewOnce: boolean;
  deliveredToOslInbox: true;
  /**
   * The wordbank cover prose that this exact message puts on Discord's wire.
   *
   * Public by construction: it is the row Discord itself will display, so it is
   * neither secret nor sensitive, and it is what the overlay shows in place of
   * the plaintext while the protected display is off. Absent or `null` whenever
   * the backend did not produce one -- notably the deliberate multi-chunk case --
   * and the overlay then renders an explicit unknown-cover notice rather than
   * approximating a cover string it does not have.
   */
  flagtext?: string | null;
}

/** Longest cover prose accepted from the backend, in UTF-8 bytes. */
export const MAX_PROTECTED_FLAGTEXT_BYTES = 2_000;

/**
 * The cover prose is public, but it is still untrusted input that ends up in the
 * DOM, so it is bounded and refused whole rather than repaired: it must be one
 * printable line, because a payload-bearing cover is accepted or refused whole.
 */
export function boundedProtectedFlagtext(value: unknown): value is string {
  return typeof value === "string"
    && value.length > 0
    && utf8Length(value) <= MAX_PROTECTED_FLAGTEXT_BYTES
    && !/[\u0000-\u001f\u007f]/u.test(value);
}

export interface NativeDiscordOverlayOpened {
  /**
   * Correlation handle for this received message. Routing metadata, never
   * content: it is what lets a received message be matched to the Discord row it
   * belongs over instead of being appended in key-server inbox order.
   */
  messageId: string;
  /**
   * The public cover prose of that Discord row. Absent for a reassembled
   * multi-row message, which deliberately has no single carrier row, and absent
   * when the backend's cover fell outside MAX_PROTECTED_FLAGTEXT_BYTES.
   */
  coverPointer?: string;
  plaintext: string;
  contextVerified: true;
  personToPersonE2ee: true;
  viewOnceConsumed: boolean;
  expiresAt: number;
}

export interface NativeDiscordOverlayOpenedBatch {
  messages: NativeDiscordOverlayOpened[];
  pendingViewOnce: NativeDiscordOverlayPendingViewOnce[];
  acknowledgments: NativeDiscordOverlayAcknowledgment[];
  fetched: number;
  /**
   * `false` when decrypted display is off for this conversation. An empty batch
   * used to mean both "nothing arrived" and "opening is switched off"; these are
   * a state and an absence, and the renderer must not report the first as the
   * second.
   */
  decryptDisplayEnabled: boolean;
  /**
   * Rows the backend left in the inbox because resolving them failed in a way a
   * later poll can fix (a cipher-store outage). Non-zero means "incomplete, retry
   * soon", which is exactly what an empty batch used to hide.
   */
  deferredRows: number;
  /**
   * Rows retained because their encrypted wire format needs a newer compatible
   * app. This is not retryable network debt: the receiver must update OSL.
   */
  unrecognizedWireRows: number;
}

export interface NativeDiscordOverlayPendingViewOnce {
  messageId: string;
  expiresAt: number;
  personToPersonE2ee: true;
}

export interface NativeDiscordOverlayAcknowledgment {
  messageId: string;
  status: "received" | "opened";
  acknowledgedAt: number;
}

export interface NativeOverlayPreparedAttachment {
  attachmentId: string;
  originalFilename: string;
  plaintextSize: number;
  expiresAt: number;
  viewOnce: boolean;
  deliveredToOslInbox: true;
}

export interface NativeOverlayPendingAttachment {
  attachmentId: string;
  originalFilename: string;
  mimeType: string;
  plaintextSize: number;
  expiresAt: number;
  viewOnce: boolean;
}

export interface NativeOverlayOpenedAttachment {
  attachmentId: string;
  originalFilename: string;
  mimeType: string;
  plaintextSize: number;
  viewOnceConsumed: boolean;
  openedInNativeViewer: true;
}

const encoder = new TextEncoder();

export function utf8Length(value: string): number {
  return encoder.encode(value).length;
}

export function boundedProtectedDraft(value: string): string {
  return value;
}

function exactRecord(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const actual = Object.keys(value).sort();
  return actual.length === keys.length && [...keys].sort().every((key, index) => key === actual[index]);
}

function boundedVisible(value: unknown, maximum: number): value is string {
  return typeof value === "string" && value.length > 0 && utf8Length(value) <= maximum
    && !value.includes("\0") && !value.includes("\u007f");
}

const NATIVE_SURFACE_KEYS = [
  "version",
  "imageDataUrl",
  "widthPx",
  "heightPx",
  "inputLeftPx",
  "inputTopPx",
  "inputWidthPx",
  "inputHeightPx",
  "inputBackground",
  "textLeftPx",
  "textTopPx",
  "textWidthPx",
  "textHeightPx",
  "fontFamily",
  "fontSizePx",
  "fontWeight",
  "lineHeightPx",
] as const;
const BMP_HEADER_BYTES = 54;
const MAX_NATIVE_SURFACE_WIDTH = 8_192;
const MAX_NATIVE_SURFACE_HEIGHT = 1_024;

function littleEndianU16(bytes: string, offset: number): number {
  return bytes.charCodeAt(offset) | (bytes.charCodeAt(offset + 1) << 8);
}

function littleEndianU32(bytes: string, offset: number): number {
  return (bytes.charCodeAt(offset)
    | (bytes.charCodeAt(offset + 1) << 8)
    | (bytes.charCodeAt(offset + 2) << 16)
    | (bytes.charCodeAt(offset + 3) << 24)) >>> 0;
}

export function parseNativeSurfaceCapture(value: unknown): NativeSurfaceCapture | null {
  const typographyAbsent = value !== null
    && typeof value === "object"
    && (value as Record<string, unknown>).fontFamily === null
    && (value as Record<string, unknown>).fontSizePx === null
    && (value as Record<string, unknown>).fontWeight === null
    && (value as Record<string, unknown>).lineHeightPx === null;
  const typographyValid = value !== null
    && typeof value === "object"
    && typeof (value as Record<string, unknown>).fontFamily === "string"
    && /^[\p{L}\p{N} ._-]{1,64}$/u.test(String((value as Record<string, unknown>).fontFamily))
    && typeof (value as Record<string, unknown>).fontSizePx === "number"
    && Number.isFinite((value as Record<string, unknown>).fontSizePx)
    && Number((value as Record<string, unknown>).fontSizePx) >= 8
    && Number((value as Record<string, unknown>).fontSizePx) <= 128
    && Number.isInteger((value as Record<string, unknown>).fontWeight)
    && Number((value as Record<string, unknown>).fontWeight) >= 100
    && Number((value as Record<string, unknown>).fontWeight) <= 1_000
    && typeof (value as Record<string, unknown>).lineHeightPx === "number"
    && Number.isFinite((value as Record<string, unknown>).lineHeightPx)
    && Number((value as Record<string, unknown>).lineHeightPx) >= 8
    && Number((value as Record<string, unknown>).lineHeightPx) <= 128;
  if (!exactRecord(value, NATIVE_SURFACE_KEYS)
    || value.version !== "osl-native-surface-capture-v1"
    || !Number.isSafeInteger(value.widthPx) || Number(value.widthPx) < 160
    || Number(value.widthPx) > MAX_NATIVE_SURFACE_WIDTH
    || !Number.isSafeInteger(value.heightPx) || Number(value.heightPx) < 24
    || Number(value.heightPx) > MAX_NATIVE_SURFACE_HEIGHT
    || !Number.isSafeInteger(value.inputLeftPx) || Number(value.inputLeftPx) < 0
    || !Number.isSafeInteger(value.inputTopPx) || Number(value.inputTopPx) < 0
    || !Number.isSafeInteger(value.inputWidthPx) || Number(value.inputWidthPx) <= 0
    || !Number.isSafeInteger(value.inputHeightPx) || Number(value.inputHeightPx) <= 0
    || Number(value.inputLeftPx) + Number(value.inputWidthPx) > Number(value.widthPx)
    || Number(value.inputTopPx) + Number(value.inputHeightPx) > Number(value.heightPx)
    || !Number.isSafeInteger(value.textLeftPx)
    || !Number.isSafeInteger(value.textTopPx)
    || !Number.isSafeInteger(value.textWidthPx) || Number(value.textWidthPx) <= 0
    || !Number.isSafeInteger(value.textHeightPx) || Number(value.textHeightPx) < 8
    || Number(value.textHeightPx) > 128
    || Number(value.textLeftPx) < Number(value.inputLeftPx)
    || Number(value.textTopPx) < Number(value.inputTopPx)
    || Number(value.textLeftPx) + Number(value.textWidthPx)
      > Number(value.inputLeftPx) + Number(value.inputWidthPx)
    || Number(value.textTopPx) + Number(value.textHeightPx)
      > Number(value.inputTopPx) + Number(value.inputHeightPx)
    || (!typographyAbsent && !typographyValid)
    || typeof value.inputBackground !== "string"
    || !/^#[0-9a-f]{6}$/u.test(value.inputBackground)
    || typeof value.imageDataUrl !== "string"
    || !value.imageDataUrl.startsWith("data:image/bmp;base64,")) return null;

  const encoded = value.imageDataUrl.slice("data:image/bmp;base64,".length);
  const expectedBytes = BMP_HEADER_BYTES + Number(value.widthPx) * Number(value.heightPx) * 4;
  const expectedEncodedLength = 4 * Math.ceil(expectedBytes / 3);
  const paddingBytes = encoded.endsWith("==") ? 2 : encoded.endsWith("=") ? 1 : 0;
  if (encoded.length !== expectedEncodedLength || encoded.length < 72 || encoded.length % 4 !== 0
    || !/^[A-Za-z0-9+/]+={0,2}$/u.test(encoded)
    || /=/u.test(encoded.slice(0, -2))
    || encoded.length / 4 * 3 - paddingBytes !== expectedBytes) return null;
  let header: string;
  try {
    header = atob(encoded.slice(0, 72));
  } catch {
    return null;
  }
  if (header.length !== BMP_HEADER_BYTES || header.slice(0, 2) !== "BM"
    || littleEndianU32(header, 2) !== expectedBytes
    || littleEndianU32(header, 10) !== BMP_HEADER_BYTES
    || littleEndianU32(header, 14) !== 40
    || littleEndianU32(header, 18) !== Number(value.widthPx)
    || littleEndianU32(header, 22) !== Number(value.heightPx)
    || littleEndianU16(header, 26) !== 1
    || littleEndianU16(header, 28) !== 32
    || littleEndianU32(header, 30) !== 0
    || littleEndianU32(header, 34) !== expectedBytes - BMP_HEADER_BYTES) return null;
  return value as unknown as NativeSurfaceCapture;
}

export function parseNativeDiscordOverlayState(value: unknown): NativeDiscordOverlayState | null {
  const baseKeys = ["active", "friendLabel", "scopeApproved", "ttlSeconds", "decryptDisplayEnabled", "viewOnceEnabled", "attachmentsEnabled", "discordMarkerAvailable", "covertextEnabled"] as const;
  const hasVisualRecipe = typeof value === "object" && value !== null && !Array.isArray(value)
    && Object.hasOwn(value, "visualRecipe");
  const hasNativeSurface = typeof value === "object" && value !== null && !Array.isArray(value)
    && Object.hasOwn(value, "nativeSurface");
  const hasVisibleCarrierRows = typeof value === "object" && value !== null && !Array.isArray(value)
    && Object.hasOwn(value, "visibleCarrierRows");
  const hasLockEngaged = typeof value === "object" && value !== null && !Array.isArray(value)
    && Object.hasOwn(value, "lockEngaged");
  if (!exactRecord(value, [
    ...baseKeys,
    ...(hasLockEngaged ? ["lockEngaged"] : []),
    ...(hasVisualRecipe ? ["visualRecipe"] : []),
    ...(hasNativeSurface ? ["nativeSurface"] : []),
    ...(hasVisibleCarrierRows ? ["visibleCarrierRows"] : []),
  ])) return null;
  if (hasLockEngaged && typeof value.lockEngaged !== "boolean") return null;
  if (value.active !== true || value.scopeApproved !== true || !boundedVisible(value.friendLabel, 80)
    || !NATIVE_OVERLAY_TTL_OPTIONS.includes(value.ttlSeconds as NativeOverlayTtlSeconds)
    || typeof value.decryptDisplayEnabled !== "boolean" || typeof value.viewOnceEnabled !== "boolean"
    || typeof value.attachmentsEnabled !== "boolean" || typeof value.discordMarkerAvailable !== "boolean"
    || typeof value.covertextEnabled !== "boolean") return null;
  const visualRecipe = hasVisualRecipe ? parseDiscordVisualRecipe(value.visualRecipe) : null;
  if (hasVisualRecipe && visualRecipe === null) return null;
  const nativeSurface = hasNativeSurface && value.nativeSurface !== null
    ? parseNativeSurfaceCapture(value.nativeSurface)
    : null;
  if (hasNativeSurface && value.nativeSurface !== null && nativeSurface === null) return null;
  const visibleCarrierRows = hasVisibleCarrierRows
    ? parseNativeDiscordCarrierRowBindings(value.visibleCarrierRows)
    : null;
  if (hasVisibleCarrierRows && visibleCarrierRows === null) return null;
  const {
    visualRecipe: _visualRecipe,
    nativeSurface: _nativeSurface,
    visibleCarrierRows: _visibleCarrierRows,
    ...base
  } = value;
  return {
    ...(base as unknown as Omit<NativeDiscordOverlayState, "visualRecipe" | "nativeSurface" | "visibleCarrierRows">),
    ...(visualRecipe ? { visualRecipe } : {}),
    ...(nativeSurface ? { nativeSurface } : {}),
    ...(visibleCarrierRows ? { visibleCarrierRows } : {}),
  };
}

const PREPARED_KEYS = [
  "messageId",
  "expiresAt",
  "personToPersonE2ee",
  "viewOnce",
  "deliveredToOslInbox",
] as const;

export function parseNativeDiscordOverlayPrepared(value: unknown): NativeDiscordOverlayPrepared | null {
  // The cover prose is an additive field: a backend that does not send it is
  // still a valid receipt, and the overlay then shows its unknown-cover notice.
  // Accepted as an exact key set either way, so an unexpected extra key is still
  // refused rather than ignored.
  const carriesFlagtext = typeof value === "object" && value !== null && !Array.isArray(value)
    && Object.hasOwn(value, "flagtext");
  if (!exactRecord(value, carriesFlagtext ? [...PREPARED_KEYS, "flagtext"] : PREPARED_KEYS)) return null;
  if (!Number.isSafeInteger(value.expiresAt) || Number(value.expiresAt) <= 0
    || !boundedVisible(value.messageId, 96)
    || value.personToPersonE2ee !== true || typeof value.viewOnce !== "boolean" || value.deliveredToOslInbox !== true) return null;
  // `null` is the backend saying "no cover was produced for this message", which
  // is a real and expected outcome. Anything else must be one printable line.
  if (carriesFlagtext && value.flagtext !== null && !boundedProtectedFlagtext(value.flagtext)) return null;
  return value as unknown as NativeDiscordOverlayPrepared;
}

export function parseNativeDiscordOverlayAcknowledgment(value: unknown): NativeDiscordOverlayAcknowledgment | null {
  if (!exactRecord(value, ["messageId", "status", "acknowledgedAt"])) return null;
  if (!boundedVisible(value.messageId, 96) || (value.status !== "received" && value.status !== "opened")
    || !Number.isSafeInteger(value.acknowledgedAt) || Number(value.acknowledgedAt) <= 0) return null;
  return value as unknown as NativeDiscordOverlayAcknowledgment;
}

const OPENED_KEYS = ["messageId", "coverPointer", "plaintext", "contextVerified", "personToPersonE2ee", "viewOnceConsumed", "expiresAt"] as const;
const OPENED_KEYS_WITHOUT_COVER = OPENED_KEYS.filter((key) => key !== "coverPointer");

export function parseNativeDiscordOverlayOpened(value: unknown): NativeDiscordOverlayOpened | null {
  // A message with no single Discord carrier row serialises with the cover key
  // *absent* rather than null, the same shape the prepared receipt already uses,
  // so both arities are accepted and nothing in between is.
  const carriesCover = typeof value === "object" && value !== null && Object.hasOwn(value, "coverPointer");
  if (!exactRecord(value, carriesCover ? OPENED_KEYS : OPENED_KEYS_WITHOUT_COVER)) return null;
  if (!boundedVisible(value.plaintext, MAX_PROTECTED_DRAFT_BYTES) || utf8Length(value.plaintext) > MAX_PROTECTED_DRAFT_BYTES
    || value.contextVerified !== true || value.personToPersonE2ee !== true || typeof value.viewOnceConsumed !== "boolean"
    || !Number.isSafeInteger(value.expiresAt) || Number(value.expiresAt) <= 0) return null;
  // The handle is routing metadata, so it is held to the same shape rules as the
  // outbound cover: one printable line, bounded, or not present at all.
  if (!validAttachmentId(value.messageId)) return null;
  if (carriesCover && !boundedProtectedFlagtext(value.coverPointer)) return null;
  return value as unknown as NativeDiscordOverlayOpened;
}

/** Remaining in-memory display lifetime. Never persists plaintext or timing. */
export function overlayExpiryDelayMs(expiresAtSeconds: number, nowMs: number): number {
  if (!Number.isSafeInteger(expiresAtSeconds) || expiresAtSeconds <= 0 || !Number.isFinite(nowMs)) return 0;
  return Math.max(0, Math.min(expiresAtSeconds * 1_000 - nowMs, 604_800_000));
}

export function parseNativeDiscordOverlayOpenedBatch(value: unknown): NativeDiscordOverlayOpenedBatch | null {
  if (!exactRecord(value, ["messages", "pendingViewOnce", "acknowledgments", "fetched", "decryptDisplayEnabled", "deferredRows", "unrecognizedWireRows"])
    || !Array.isArray(value.messages)
    || !Array.isArray(value.pendingViewOnce) || !Array.isArray(value.acknowledgments) || value.acknowledgments.length > 64
    || value.pendingViewOnce.length > 64
    || value.messages.length > 64 || !Number.isSafeInteger(value.fetched)
    || Number(value.fetched) < 0 || Number(value.fetched) > 64
    // Both new fields are required and exactly typed. A batch that cannot state
    // whether opening was on, or how many rows it deferred, is the ambiguous
    // shape this parser exists to stop accepting.
    //
    // `deferredRows` is deliberately not capped at the 64-row page size: it only
    // drives a status sentence and the poll interval, and the inbox page size is
    // owned elsewhere and is actively changing. A cap here would turn a larger
    // page into a rejected batch, which is a real regression for a field that
    // cannot affect what is displayed.
    || typeof value.decryptDisplayEnabled !== "boolean"
    || !Number.isSafeInteger(value.deferredRows)
    || Number(value.deferredRows) < 0
    || !Number.isSafeInteger(value.unrecognizedWireRows)
    || Number(value.unrecognizedWireRows) < 0) return null;
  const messages = value.messages.map(parseNativeDiscordOverlayOpened);
  const pendingViewOnce = value.pendingViewOnce.map(parseNativeDiscordOverlayPendingViewOnce);
  const acknowledgments = value.acknowledgments.map(parseNativeDiscordOverlayAcknowledgment);
  if (messages.some((message) => message === null) || pendingViewOnce.some((message) => message === null)
    || acknowledgments.some((receipt) => receipt === null)) return null;
  return { messages: messages as NativeDiscordOverlayOpened[], pendingViewOnce: pendingViewOnce as NativeDiscordOverlayPendingViewOnce[], acknowledgments: acknowledgments as NativeDiscordOverlayAcknowledgment[], fetched: value.fetched as number, decryptDisplayEnabled: value.decryptDisplayEnabled as boolean, deferredRows: value.deferredRows as number, unrecognizedWireRows: value.unrecognizedWireRows as number };
}

function validAttachmentId(value: unknown): value is string {
  return typeof value === "string" && /^peer-[0-9a-f]{32}$/u.test(value);
}

export function parseNativeDiscordOverlayPendingViewOnce(value: unknown): NativeDiscordOverlayPendingViewOnce | null {
  if (!exactRecord(value, ["messageId", "expiresAt", "personToPersonE2ee"])
    || !validAttachmentId(value.messageId) || !Number.isSafeInteger(value.expiresAt) || Number(value.expiresAt) <= 0
    || value.personToPersonE2ee !== true) return null;
  return value as unknown as NativeDiscordOverlayPendingViewOnce;
}

function validAttachmentMetadata(value: Record<string, unknown>): boolean {
  return validAttachmentId(value.attachmentId)
    && boundedVisible(value.originalFilename, 1_024)
    && Number.isSafeInteger(value.plaintextSize)
    && Number(value.plaintextSize) > 0
    && Number(value.plaintextSize) <= 512 * 1024 * 1024;
}

export function parseNativeOverlayPreparedAttachment(value: unknown): NativeOverlayPreparedAttachment | null {
  if (!exactRecord(value, ["attachmentId", "originalFilename", "plaintextSize", "expiresAt", "viewOnce", "deliveredToOslInbox"])
    || !validAttachmentMetadata(value) || !Number.isSafeInteger(value.expiresAt) || Number(value.expiresAt) <= 0
    || typeof value.viewOnce !== "boolean" || value.deliveredToOslInbox !== true) return null;
  return value as unknown as NativeOverlayPreparedAttachment;
}

export function parseNativeOverlayPendingAttachment(value: unknown): NativeOverlayPendingAttachment | null {
  if (!exactRecord(value, ["attachmentId", "originalFilename", "mimeType", "plaintextSize", "expiresAt", "viewOnce"])
    || !validAttachmentMetadata(value) || !boundedVisible(value.mimeType, 64)
    || !Number.isSafeInteger(value.expiresAt) || Number(value.expiresAt) <= 0
    || typeof value.viewOnce !== "boolean") return null;
  return value as unknown as NativeOverlayPendingAttachment;
}

export function parseNativeOverlayOpenedAttachment(value: unknown): NativeOverlayOpenedAttachment | null {
  if (!exactRecord(value, ["attachmentId", "originalFilename", "mimeType", "plaintextSize", "viewOnceConsumed", "openedInNativeViewer"])
    || !validAttachmentMetadata(value) || !boundedVisible(value.mimeType, 64)
    || typeof value.viewOnceConsumed !== "boolean" || value.openedInNativeViewer !== true) return null;
  return value as unknown as NativeOverlayOpenedAttachment;
}
