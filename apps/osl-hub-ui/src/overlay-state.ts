export const MAX_PROTECTED_DRAFT_BYTES = 1024 * 1024;
export const PROTECTED_DRAFT_WARNING_BYTES = 900 * 1024;
export const NATIVE_OVERLAY_TTL_OPTIONS = [3_600, 86_400, 259_200, 604_800] as const;
export type NativeOverlayTtlSeconds = typeof NATIVE_OVERLAY_TTL_OPTIONS[number];

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
}

export interface NativeDiscordOverlayPrepared {
  messageId: string;
  expiresAt: number;
  personToPersonE2ee: true;
  viewOnce: boolean;
  deliveredToOslInbox: true;
}

export interface NativeDiscordOverlayOpened {
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

export function parseNativeDiscordOverlayState(value: unknown): NativeDiscordOverlayState | null {
  if (!exactRecord(value, ["active", "friendLabel", "scopeApproved", "ttlSeconds", "decryptDisplayEnabled", "viewOnceEnabled", "attachmentsEnabled", "discordMarkerAvailable", "covertextEnabled"])) return null;
  if (value.active !== true || value.scopeApproved !== true || !boundedVisible(value.friendLabel, 80)
    || !NATIVE_OVERLAY_TTL_OPTIONS.includes(value.ttlSeconds as NativeOverlayTtlSeconds)
    || typeof value.decryptDisplayEnabled !== "boolean" || typeof value.viewOnceEnabled !== "boolean"
    || typeof value.attachmentsEnabled !== "boolean" || typeof value.discordMarkerAvailable !== "boolean"
    || typeof value.covertextEnabled !== "boolean") return null;
  return value as unknown as NativeDiscordOverlayState;
}

export function parseNativeDiscordOverlayPrepared(value: unknown): NativeDiscordOverlayPrepared | null {
  if (!exactRecord(value, ["messageId", "expiresAt", "personToPersonE2ee", "viewOnce", "deliveredToOslInbox"])) return null;
  if (!Number.isSafeInteger(value.expiresAt) || Number(value.expiresAt) <= 0
    || !boundedVisible(value.messageId, 96)
    || value.personToPersonE2ee !== true || typeof value.viewOnce !== "boolean" || value.deliveredToOslInbox !== true) return null;
  return value as unknown as NativeDiscordOverlayPrepared;
}

export function parseNativeDiscordOverlayAcknowledgment(value: unknown): NativeDiscordOverlayAcknowledgment | null {
  if (!exactRecord(value, ["messageId", "status", "acknowledgedAt"])) return null;
  if (!boundedVisible(value.messageId, 96) || (value.status !== "received" && value.status !== "opened")
    || !Number.isSafeInteger(value.acknowledgedAt) || Number(value.acknowledgedAt) <= 0) return null;
  return value as unknown as NativeDiscordOverlayAcknowledgment;
}

export function parseNativeDiscordOverlayOpened(value: unknown): NativeDiscordOverlayOpened | null {
  if (!exactRecord(value, ["plaintext", "contextVerified", "personToPersonE2ee", "viewOnceConsumed", "expiresAt"])) return null;
  if (!boundedVisible(value.plaintext, MAX_PROTECTED_DRAFT_BYTES) || utf8Length(value.plaintext) > MAX_PROTECTED_DRAFT_BYTES
    || value.contextVerified !== true || value.personToPersonE2ee !== true || typeof value.viewOnceConsumed !== "boolean"
    || !Number.isSafeInteger(value.expiresAt) || Number(value.expiresAt) <= 0) return null;
  return value as unknown as NativeDiscordOverlayOpened;
}

/** Remaining in-memory display lifetime. Never persists plaintext or timing. */
export function overlayExpiryDelayMs(expiresAtSeconds: number, nowMs: number): number {
  if (!Number.isSafeInteger(expiresAtSeconds) || expiresAtSeconds <= 0 || !Number.isFinite(nowMs)) return 0;
  return Math.max(0, Math.min(expiresAtSeconds * 1_000 - nowMs, 604_800_000));
}

export function parseNativeDiscordOverlayOpenedBatch(value: unknown): NativeDiscordOverlayOpenedBatch | null {
  if (!exactRecord(value, ["messages", "pendingViewOnce", "acknowledgments", "fetched"]) || !Array.isArray(value.messages)
    || !Array.isArray(value.pendingViewOnce) || !Array.isArray(value.acknowledgments) || value.acknowledgments.length > 64
    || value.pendingViewOnce.length > 64
    || value.messages.length > 64 || !Number.isSafeInteger(value.fetched)
    || Number(value.fetched) < 0 || Number(value.fetched) > 64) return null;
  const messages = value.messages.map(parseNativeDiscordOverlayOpened);
  const pendingViewOnce = value.pendingViewOnce.map(parseNativeDiscordOverlayPendingViewOnce);
  const acknowledgments = value.acknowledgments.map(parseNativeDiscordOverlayAcknowledgment);
  if (messages.some((message) => message === null) || pendingViewOnce.some((message) => message === null)
    || acknowledgments.some((receipt) => receipt === null)) return null;
  return { messages: messages as NativeDiscordOverlayOpened[], pendingViewOnce: pendingViewOnce as NativeDiscordOverlayPendingViewOnce[], acknowledgments: acknowledgments as NativeDiscordOverlayAcknowledgment[], fetched: value.fetched as number };
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
