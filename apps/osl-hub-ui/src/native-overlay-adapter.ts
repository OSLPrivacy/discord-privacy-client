import { invoke } from "@tauri-apps/api/core";
import {
  boundedProtectedDraft,
  MAX_PROTECTED_DRAFT_BYTES,
  NATIVE_OVERLAY_TTL_OPTIONS,
  parseNativeDiscordOverlayOpenedBatch,
  parseNativeDiscordOverlayOpened,
  parseNativeDiscordOverlayPrepared,
  parseNativeDiscordOverlayState,
  parseNativeOverlayOpenedAttachment,
  parseNativeOverlayPendingAttachment,
  parseNativeOverlayPreparedAttachment,
  type NativeDiscordOverlayOpenedBatch,
  type NativeDiscordOverlayOpened,
  type NativeDiscordOverlayPrepared,
  type NativeDiscordOverlayState,
  type NativeOverlayTtlSeconds,
  type NativeOverlayOpenedAttachment,
  type NativeOverlayPendingAttachment,
  type NativeOverlayPreparedAttachment,
  utf8Length,
} from "./overlay-state";

export async function getNativeDiscordOverlayState(): Promise<NativeDiscordOverlayState | null> {
  try { return parseNativeDiscordOverlayState(await invoke<unknown>("get_native_discord_overlay_state")); }
  catch { return null; }
}

export async function prepareNativeDiscordOverlayText(plaintext: string, viewOnce: boolean): Promise<NativeDiscordOverlayPrepared | null> {
  if (typeof plaintext !== "string" || !plaintext || utf8Length(plaintext) > MAX_PROTECTED_DRAFT_BYTES
    || boundedProtectedDraft(plaintext) !== plaintext || typeof viewOnce !== "boolean") return null;
  try {
    return parseNativeDiscordOverlayPrepared(await invoke<unknown>("prepare_native_discord_overlay_text", { plaintext, viewOnce }));
  } catch { return null; }
}

export type NativeDiscordCarrierMode = "atomic" | "compatibility";
export interface NativeDiscordCarrierReceipt {
  placed: boolean;
  enterSent: boolean;
  status: "sent" | "calibrationRequired" | "contextChanged" | "composerUnavailable" | "composerNotEmpty" | "placementRejected" | "enterRejected" | "platformUnsupported";
  mode: NativeDiscordCarrierMode;
  compatibilityDelayMs: number;
}

export async function sendNativeDiscordOverlayCarrier(
  mode: NativeDiscordCarrierMode,
  charsPerSecond: number,
): Promise<NativeDiscordCarrierReceipt | null> {
  if ((mode !== "atomic" && mode !== "compatibility")
    || !Number.isInteger(charsPerSecond) || charsPerSecond < 0 || charsPerSecond > 120) return null;
  try {
    const value = await invoke<unknown>("send_native_discord_overlay_carrier", { mode, charsPerSecond });
    if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
    const record = value as Record<string, unknown>;
    const statuses = ["sent", "calibrationRequired", "contextChanged", "composerUnavailable", "composerNotEmpty", "placementRejected", "enterRejected", "platformUnsupported"];
    if (typeof record.placed !== "boolean" || typeof record.enterSent !== "boolean"
      || !statuses.includes(String(record.status)) || record.mode !== mode
      || !Number.isInteger(record.compatibilityDelayMs)
      || Number(record.compatibilityDelayMs) < 63 || Number(record.compatibilityDelayMs) > 500
      || (record.status === "sent" && (record.placed !== true || record.enterSent !== true))
      || (record.enterSent === true && record.placed !== true)) return null;
    return record as unknown as NativeDiscordCarrierReceipt;
  } catch { return null; }
}

export async function openNativeDiscordOverlayText(): Promise<NativeDiscordOverlayOpenedBatch | null> {
  try { return parseNativeDiscordOverlayOpenedBatch(await invoke<unknown>("open_native_discord_overlay_text")); }
  catch { return null; }
}

export async function revealNativeDiscordOverlayViewOnce(messageId: string): Promise<NativeDiscordOverlayOpened | null> {
  if (!/^peer-[0-9a-f]{32}$/u.test(messageId)) return null;
  try {
    return parseNativeDiscordOverlayOpened(await invoke<unknown>("reveal_native_discord_overlay_view_once", { messageId }));
  } catch { return null; }
}

export interface NativeDiscordOverlayBurnResult {
  rowsDestroyed: number;
  channelsDestroyed: number;
  whitelistEntriesRemoved: number;
  localProtectedRowsDestroyed: number;
  remoteBlobsDeleted: number;
  remoteBlobDeletionsFailed: number;
  localCleanupComplete: boolean;
  remoteCleanupComplete: boolean;
  discordHistoryDeleted: false;
  recipientCopiesDeleted: false;
}

export async function burnNativeDiscordOverlayChat(): Promise<NativeDiscordOverlayBurnResult | null> {
  try {
    const value = await invoke<unknown>("burn_native_discord_overlay_chat");
    if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
    const record = value as Record<string, unknown>;
    const keys = ["rowsDestroyed", "channelsDestroyed", "whitelistEntriesRemoved", "localProtectedRowsDestroyed", "remoteBlobsDeleted", "remoteBlobDeletionsFailed"];
    if (keys.some((key) => !Number.isSafeInteger(record[key]) || Number(record[key]) < 0)
      || typeof record.localCleanupComplete !== "boolean"
      || typeof record.remoteCleanupComplete !== "boolean"
      || record.discordHistoryDeleted !== false || record.recipientCopiesDeleted !== false) return null;
    return record as unknown as NativeDiscordOverlayBurnResult;
  } catch { return null; }
}

export async function setNativeDiscordOverlaySecurity(
  ttlSeconds: NativeOverlayTtlSeconds,
  decryptDisplayEnabled: boolean,
): Promise<NativeDiscordOverlayState | null> {
  if (!NATIVE_OVERLAY_TTL_OPTIONS.includes(ttlSeconds) || typeof decryptDisplayEnabled !== "boolean") return null;
  try {
    return parseNativeDiscordOverlayState(await invoke<unknown>("set_native_discord_overlay_security", {
      ttlSeconds,
      decryptDisplayEnabled,
    }));
  } catch { return null; }
}

export async function selectNativeDiscordOverlayAttachment(viewOnce: boolean): Promise<NativeOverlayPreparedAttachment | "cancelled" | null> {
  if (typeof viewOnce !== "boolean") return null;
  try {
    const value = await invoke<unknown>("select_native_discord_overlay_attachment", { viewOnce });
    if (value === null) return "cancelled";
    return parseNativeOverlayPreparedAttachment(value);
  } catch { return null; }
}

export async function listNativeDiscordOverlayAttachments(): Promise<NativeOverlayPendingAttachment[] | null> {
  try {
    const value = await invoke<unknown>("list_native_discord_overlay_attachments");
    if (!Array.isArray(value) || value.length > 64) return null;
    const parsed = value.map(parseNativeOverlayPendingAttachment);
    return parsed.some((entry) => entry === null) ? null : parsed as NativeOverlayPendingAttachment[];
  } catch { return null; }
}

export async function openNativeDiscordOverlayAttachment(attachmentId: string): Promise<NativeOverlayOpenedAttachment | null> {
  if (!/^peer-[0-9a-f]{32}$/u.test(attachmentId)) return null;
  try {
    return parseNativeOverlayOpenedAttachment(await invoke<unknown>("open_native_discord_overlay_attachment", { attachmentId }));
  } catch { return null; }
}

export async function selectOslChatAttachment(viewOnce: boolean): Promise<NativeOverlayPreparedAttachment | "cancelled" | null> {
  if (typeof viewOnce !== "boolean") return null;
  try {
    const value = await invoke<unknown>("select_osl_chat_attachment", { viewOnce });
    if (value === null) return "cancelled";
    return parseNativeOverlayPreparedAttachment(value);
  } catch { return null; }
}

export async function listOslChatAttachments(): Promise<NativeOverlayPendingAttachment[] | null> {
  try {
    const value = await invoke<unknown>("list_osl_chat_attachments");
    if (!Array.isArray(value) || value.length > 64) return null;
    const parsed = value.map(parseNativeOverlayPendingAttachment);
    return parsed.some((entry) => entry === null) ? null : parsed as NativeOverlayPendingAttachment[];
  } catch { return null; }
}

export async function openOslChatAttachment(attachmentId: string): Promise<NativeOverlayOpenedAttachment | null> {
  if (!/^[A-Za-z0-9_-]{8,128}$/u.test(attachmentId)) return null;
  try { return parseNativeOverlayOpenedAttachment(await invoke<unknown>("open_osl_chat_attachment", { attachmentId })); }
  catch { return null; }
}
