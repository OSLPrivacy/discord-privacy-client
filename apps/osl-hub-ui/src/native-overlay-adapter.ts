import { invoke } from "@tauri-apps/api/core";
// The overlay's adapters fail closed exactly as before; the backend's refusal
// message is now kept beside the null instead of being discarded with the
// exception. See ./backend-failure.ts.
import { checkedBackendResponse, recordBackendFailure } from "./backend-failure";
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
import {
  parseNativeDiscordCarrierRowBinding,
  type NativeDiscordCarrierRowBinding,
} from "./discord-carrier-row-binding";

async function invokeNativeDiscordOverlayStateValue(): Promise<unknown> {
  return invoke<unknown>("get_native_discord_overlay_state");
}

async function invokeNativeDiscordOverlayState(): Promise<NativeDiscordOverlayState | null> {
  return parseNativeDiscordOverlayState(await invokeNativeDiscordOverlayStateValue());
}

export async function getNativeDiscordOverlayState(): Promise<NativeDiscordOverlayState | null> {
  try { return await invokeNativeDiscordOverlayState(); }
  catch (error) { recordBackendFailure("get_native_discord_overlay_state", error); return null; }
}

/**
 * Disposable two-VM QA only. The native command is compile-feature-gated and
 * accepts no plaintext or routing input; production binaries do not register
 * it.
 */
export async function sendNativeDiscordQaProbe(): Promise<NativeDiscordOverlayPrepared | null> {
  if (import.meta.env.VITE_OSL_DISCORD_QA_SHELL !== "1") return null;
  try {
    return parseNativeDiscordOverlayPrepared(await invoke<unknown>("send_native_discord_qa_probe"));
  } catch (error) {
    recordBackendFailure("send_native_discord_qa_probe", error);
    return null;
  }
}

export interface NativeDiscordOverlayQaDiagnostic {
  state: NativeDiscordOverlayState | null;
  rejection: {
    command: "get_native_discord_overlay_state";
    message: string;
  } | null;
}

const MAX_QA_REJECTION_CHARS = 320;

function boundedQaRejection(error: unknown): string {
  const raw = typeof error === "string"
    ? error
    : error instanceof Error
      ? error.message
      : typeof error === "object" && error !== null && typeof (error as { message?: unknown }).message === "string"
        ? String((error as { message: string }).message)
        : "Backend rejected the overlay state request.";
  const sanitized = raw
    .replace(/[\u0000-\u001f\u007f]+/gu, " ")
    .replace(/\b(Bearer|token|password|secret|private[_ -]?key)\s*[:=]?\s*\S+/giu, "$1 [redacted]")
    .replace(/\s+/gu, " ")
    .trim();
  return Array.from(sanitized || "Backend rejected the overlay state request.")
    .slice(0, MAX_QA_REJECTION_CHARS)
    .join("");
}

const QA_OVERLAY_STATE_FIELDS = [
  "active",
  "friendLabel",
  "scopeApproved",
  "ttlSeconds",
  "decryptDisplayEnabled",
  "viewOnceEnabled",
  "attachmentsEnabled",
  "discordMarkerAvailable",
  "covertextEnabled",
] as const;
const QA_OVERLAY_OPTIONAL_STATE_FIELDS = [
  "visualRecipe",
  "nativeSurface",
  "visibleCarrierRows",
] as const;

function qaOverlayStateStructuralRejection(value: unknown): string {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return "Backend response is not an overlay-state object.";
  }
  const record = value as Record<string, unknown>;
  const actualKeys = Object.keys(record);
  const missing = QA_OVERLAY_STATE_FIELDS.filter((key) => !Object.hasOwn(record, key));
  const allowedKeys = [
    ...QA_OVERLAY_STATE_FIELDS,
    ...QA_OVERLAY_OPTIONAL_STATE_FIELDS,
  ] as readonly string[];
  const unexpectedCount = actualKeys.filter((key) => !allowedKeys.includes(key)).length;
  if (missing.length > 0 || unexpectedCount > 0) {
    return `Overlay-state fields do not match (missing: ${missing.join(", ") || "none"}; unexpected: ${unexpectedCount}).`;
  }
  if (record.active !== true) return "Overlay-state field active must be true.";
  if (record.scopeApproved !== true) return "Overlay-state field scopeApproved must be true.";
  if (typeof record.friendLabel !== "string" || record.friendLabel.length === 0
    || utf8Length(record.friendLabel) > 80 || /[\u0000\u007f]/u.test(record.friendLabel)) {
    return "Overlay-state field friendLabel is invalid.";
  }
  if (!NATIVE_OVERLAY_TTL_OPTIONS.includes(record.ttlSeconds as NativeOverlayTtlSeconds)) {
    return "Overlay-state field ttlSeconds is not an allowed TTL.";
  }
  for (const key of ["decryptDisplayEnabled", "viewOnceEnabled", "attachmentsEnabled", "discordMarkerAvailable", "covertextEnabled"] as const) {
    if (typeof record[key] !== "boolean") return `Overlay-state field ${key} must be boolean.`;
  }
  return "Backend returned an invalid overlay state.";
}

// This diagnostic is deliberately consumed only by the stripped Discord QA
// shell. Production callers keep the fail-closed, detail-free adapter above.
export async function getNativeDiscordOverlayQaDiagnostic(): Promise<NativeDiscordOverlayQaDiagnostic> {
  try {
    const value = await invokeNativeDiscordOverlayStateValue();
    const state = parseNativeDiscordOverlayState(value);
    return {
      state,
      rejection: state === null
        ? {
          command: "get_native_discord_overlay_state",
          message: qaOverlayStateStructuralRejection(value),
        }
        : null,
    };
  } catch (error: unknown) {
    // Recorded as well as returned: the QA shell shows this, and the journal
    // keeps it for the surfaces that only get a null from the adapter above.
    recordBackendFailure("get_native_discord_overlay_state", error);
    return {
      state: null,
      rejection: {
        command: "get_native_discord_overlay_state",
        message: boundedQaRejection(error),
      },
    };
  }
}

export async function prepareNativeDiscordOverlayText(plaintext: string, viewOnce: boolean): Promise<NativeDiscordOverlayPrepared | null> {
  if (typeof plaintext !== "string" || !plaintext || utf8Length(plaintext) > MAX_PROTECTED_DRAFT_BYTES
    || boundedProtectedDraft(plaintext) !== plaintext || typeof viewOnce !== "boolean") return null;
  try {
    return checkedBackendResponse("prepare_native_discord_overlay_text",
      parseNativeDiscordOverlayPrepared(await invoke<unknown>("prepare_native_discord_overlay_text", { plaintext, viewOnce })),
      "the prepared message did not match the expected shape");
  } catch (error) { recordBackendFailure("prepare_native_discord_overlay_text", error, [plaintext]); return null; }
}

export type NativeDiscordCarrierMode = "atomic" | "compatibility";
export type NativeDiscordCarrierSendOutcome = "sent" | "notSent" | "deliveryUncertain";
export interface NativeDiscordCarrierReceipt {
  placed: boolean;
  enterSent: boolean;
  status: "sent" | "calibrationRequired" | "contextChanged" | "composerUnavailable" | "composerNotEmpty" | "placementRejected" | "enterRejected" | "carrierUnconfirmed" | "platformUnsupported";
  mode: NativeDiscordCarrierMode;
  compatibilityDelayMs: number;
  sendOutcome: NativeDiscordCarrierSendOutcome;
  automaticRetryAfterUncertain: false;
}

export const C4_NATIVE_RECEIPT_SCHEMA = "osl-c4-native-receipt-v2" as const;

export interface C4NativeReceiptSurface {
  value: string;
}

export type C4NativeReceiptEvidence =
  | {
    schema: typeof C4_NATIVE_RECEIPT_SCHEMA;
    attempt: number;
    state: "pending";
  }
  | {
    schema: typeof C4_NATIVE_RECEIPT_SCHEMA;
    attempt: number;
    state: "returned";
    receipt: NativeDiscordCarrierReceipt;
  };

function requireC4Attempt(attempt: number): void {
  if (!Number.isSafeInteger(attempt) || attempt < 1) {
    throw new RangeError("C4 native receipt attempt must be a positive safe integer");
  }
}

export function nativeDiscordCarrierSendOutcome(
  receipt: Pick<NativeDiscordCarrierReceipt, "enterSent" | "status">,
): NativeDiscordCarrierSendOutcome {
  if (receipt.status === "sent") return "sent";
  return receipt.enterSent ? "deliveryUncertain" : "notSent";
}

function completeNativeDiscordCarrierReceipt(
  receipt: Pick<NativeDiscordCarrierReceipt, "placed" | "enterSent" | "status" | "mode" | "compatibilityDelayMs">,
): NativeDiscordCarrierReceipt {
  return {
    placed: receipt.placed,
    enterSent: receipt.enterSent,
    status: receipt.status,
    mode: receipt.mode,
    compatibilityDelayMs: receipt.compatibilityDelayMs,
    sendOutcome: nativeDiscordCarrierSendOutcome(receipt),
    automaticRetryAfterUncertain: false,
  };
}

export function serializeC4NativeReceiptEvidence(
  attempt: number,
  receipt?: NativeDiscordCarrierReceipt,
): string {
  requireC4Attempt(attempt);
  const evidence: C4NativeReceiptEvidence = receipt === undefined
    ? {
      schema: C4_NATIVE_RECEIPT_SCHEMA,
      attempt,
      state: "pending",
    }
    : {
      schema: C4_NATIVE_RECEIPT_SCHEMA,
      attempt,
      state: "returned",
      // Copy the Rust receipt fields plus deterministic C4 policy fields. This
      // surface must never grow UI-derived status text or fields guessed by the
      // renderer. The send outcome below is derived only from the native status
      // and Enter edge, and uncertainty explicitly forbids automatic retry so a
      // proof failure cannot duplicate a real host send.
      receipt: completeNativeDiscordCarrierReceipt(receipt),
    };
  return JSON.stringify(evidence);
}

export function clearC4NativeReceiptEvidence(surface: C4NativeReceiptSurface): void {
  surface.value = "";
}

export async function captureC4NativeReceipt(
  surface: C4NativeReceiptSurface,
  attempt: number,
  invokeReceipt: () => Promise<NativeDiscordCarrierReceipt | null>,
): Promise<NativeDiscordCarrierReceipt | null> {
  requireC4Attempt(attempt);
  // Publish before calling the native command. This both removes a prior
  // success and makes an interrupted/in-flight attempt distinguishable from a
  // returned native receipt.
  surface.value = serializeC4NativeReceiptEvidence(attempt);
  try {
    const receipt = await invokeReceipt();
    if (receipt === null) {
      clearC4NativeReceiptEvidence(surface);
      return null;
    }
    surface.value = serializeC4NativeReceiptEvidence(attempt, receipt);
    return receipt;
  } catch (error) {
    clearC4NativeReceiptEvidence(surface);
    throw error;
  }
}

export interface NativeDiscordCarrierLayout {
  contentWidthPx: number;
  averageGraphemeWidthPx: number;
  lineHeightPx: number;
  zoom: number;
  density: number;
  padding: "shapeMatched" | "fixedCompact" | "fixedStandard" | "fixedTall";
  rowKind: "plainText" | "markdown" | "media" | "reply";
}

function validCarrierLayout(layout: NativeDiscordCarrierLayout): boolean {
  const numbers = [layout.contentWidthPx, layout.averageGraphemeWidthPx, layout.lineHeightPx, layout.zoom, layout.density];
  return numbers.every((value) => Number.isFinite(value) && value > 0 && value <= 10_000)
    && ["shapeMatched", "fixedCompact", "fixedStandard", "fixedTall"].includes(layout.padding)
    && ["plainText", "markdown", "media", "reply"].includes(layout.rowKind);
}

function parseNativeDiscordCarrierReceipt(
  value: unknown,
  mode: NativeDiscordCarrierMode,
): NativeDiscordCarrierReceipt | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  const statuses = ["sent", "calibrationRequired", "contextChanged", "composerUnavailable", "composerNotEmpty", "placementRejected", "enterRejected", "carrierUnconfirmed", "platformUnsupported"];
  if (typeof record.placed !== "boolean" || typeof record.enterSent !== "boolean"
    || !statuses.includes(String(record.status)) || record.mode !== mode
    || !Number.isInteger(record.compatibilityDelayMs)
    || Number(record.compatibilityDelayMs) < 63 || Number(record.compatibilityDelayMs) > 500
    || (record.status === "sent" && (record.placed !== true || record.enterSent !== true))
    || (record.enterSent === true && record.placed !== true)) return null;
  return completeNativeDiscordCarrierReceipt({
    placed: record.placed,
    enterSent: record.enterSent,
    status: record.status as NativeDiscordCarrierReceipt["status"],
    mode: record.mode as NativeDiscordCarrierMode,
    compatibilityDelayMs: Number(record.compatibilityDelayMs),
  });
}

export async function sendNativeDiscordOverlayCarrier(
  mode: NativeDiscordCarrierMode,
  charsPerSecond: number,
  layout?: NativeDiscordCarrierLayout,
): Promise<NativeDiscordCarrierReceipt | null> {
  if ((mode !== "atomic" && mode !== "compatibility")
    || !Number.isInteger(charsPerSecond) || charsPerSecond < 0 || charsPerSecond > 120
    || (layout !== undefined && !validCarrierLayout(layout))) return null;
  try {
    const args: Record<string, unknown> = { mode, charsPerSecond };
    if (layout !== undefined) args.layout = layout;
    return checkedBackendResponse("send_native_discord_overlay_carrier",
      parseNativeDiscordCarrierReceipt(
        await invoke<unknown>("send_native_discord_overlay_carrier", args),
        mode,
      ),
      "the carrier receipt did not match the expected shape");
  } catch (error) { recordBackendFailure("send_native_discord_overlay_carrier", error); return null; }
}

export interface NativeDiscordQaAtomicText {
  prepared: NativeDiscordOverlayPrepared;
  carrier: NativeDiscordCarrierReceipt;
  visibleCarrierRow?: NativeDiscordCarrierRowBinding;
}

export async function sendNativeDiscordQaAtomicText(
  plaintext: string,
  viewOnce: boolean,
  mode: NativeDiscordCarrierMode,
  charsPerSecond: number,
  layout?: NativeDiscordCarrierLayout,
): Promise<NativeDiscordQaAtomicText | null> {
  if (import.meta.env.VITE_OSL_DISCORD_QA_SHELL !== "1"
    || typeof plaintext !== "string" || !plaintext
    || utf8Length(plaintext) > MAX_PROTECTED_DRAFT_BYTES
    || boundedProtectedDraft(plaintext) !== plaintext
    || typeof viewOnce !== "boolean"
    || (mode !== "atomic" && mode !== "compatibility")
    || !Number.isInteger(charsPerSecond) || charsPerSecond < 0 || charsPerSecond > 120
    || (layout !== undefined && !validCarrierLayout(layout))) return null;
  try {
    const args: Record<string, unknown> = {
      plaintext,
      viewOnce,
      mode,
      charsPerSecond,
    };
    if (layout !== undefined) args.layout = layout;
    const value = await invoke<unknown>("send_native_discord_qa_atomic_text", args);
    if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
    const record = value as Record<string, unknown>;
    const prepared = parseNativeDiscordOverlayPrepared(record.prepared);
    const carrier = parseNativeDiscordCarrierReceipt(record.carrier, mode);
    const visibleCarrierRow = record.visibleCarrierRow == null
      ? undefined
      : parseNativeDiscordCarrierRowBinding(record.visibleCarrierRow);
    if (!prepared || !prepared.personToPersonE2ee || !prepared.deliveredToOslInbox
      || !carrier || (record.visibleCarrierRow != null && !visibleCarrierRow)) return null;
    return { prepared, carrier, ...(visibleCarrierRow ? { visibleCarrierRow } : {}) };
  } catch (error) {
    recordBackendFailure("send_native_discord_qa_atomic_text", error, [plaintext]);
    return null;
  }
}

export async function openNativeDiscordOverlayText(): Promise<NativeDiscordOverlayOpenedBatch | null> {
  try {
    return checkedBackendResponse("open_native_discord_overlay_text",
      parseNativeDiscordOverlayOpenedBatch(await invoke<unknown>("open_native_discord_overlay_text")),
      "the opened batch did not match the expected shape");
  }
  catch (error) { recordBackendFailure("open_native_discord_overlay_text", error); return null; }
}

export async function revealNativeDiscordOverlayViewOnce(messageId: string): Promise<NativeDiscordOverlayOpened | null> {
  if (!/^peer-[0-9a-f]{32}$/u.test(messageId)) return null;
  try {
    return checkedBackendResponse("reveal_native_discord_overlay_view_once",
      parseNativeDiscordOverlayOpened(await invoke<unknown>("reveal_native_discord_overlay_view_once", { messageId })),
      "the revealed message did not match the expected shape");
  } catch (error) { recordBackendFailure("reveal_native_discord_overlay_view_once", error); return null; }
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
  } catch (error) { recordBackendFailure("burn_native_discord_overlay_chat", error); return null; }
}

export async function setNativeDiscordOverlaySecurity(
  ttlSeconds: NativeOverlayTtlSeconds,
  decryptDisplayEnabled: boolean,
): Promise<NativeDiscordOverlayState | null> {
  if (!NATIVE_OVERLAY_TTL_OPTIONS.includes(ttlSeconds) || typeof decryptDisplayEnabled !== "boolean") return null;
  try {
    return checkedBackendResponse("set_native_discord_overlay_security",
      parseNativeDiscordOverlayState(await invoke<unknown>("set_native_discord_overlay_security", {
        ttlSeconds,
        decryptDisplayEnabled,
      })),
      "the overlay state did not match the expected shape");
  } catch (error) { recordBackendFailure("set_native_discord_overlay_security", error); return null; }
}

export async function selectNativeDiscordOverlayAttachment(viewOnce: boolean): Promise<NativeOverlayPreparedAttachment | "cancelled" | null> {
  if (typeof viewOnce !== "boolean") return null;
  try {
    const value = await invoke<unknown>("select_native_discord_overlay_attachment", { viewOnce });
    if (value === null) return "cancelled";
    return parseNativeOverlayPreparedAttachment(value);
  } catch (error) { recordBackendFailure("select_native_discord_overlay_attachment", error); return null; }
}

export async function listNativeDiscordOverlayAttachments(): Promise<NativeOverlayPendingAttachment[] | null> {
  try {
    const value = await invoke<unknown>("list_native_discord_overlay_attachments");
    if (!Array.isArray(value) || value.length > 64) return null;
    const parsed = value.map(parseNativeOverlayPendingAttachment);
    return parsed.some((entry) => entry === null) ? null : parsed as NativeOverlayPendingAttachment[];
  } catch (error) { recordBackendFailure("list_native_discord_overlay_attachments", error); return null; }
}

export async function openNativeDiscordOverlayAttachment(attachmentId: string): Promise<NativeOverlayOpenedAttachment | null> {
  if (!/^peer-[0-9a-f]{32}$/u.test(attachmentId)) return null;
  try {
    return parseNativeOverlayOpenedAttachment(await invoke<unknown>("open_native_discord_overlay_attachment", { attachmentId }));
  } catch (error) { recordBackendFailure("open_native_discord_overlay_attachment", error); return null; }
}

export async function selectOslChatAttachment(viewOnce: boolean): Promise<NativeOverlayPreparedAttachment | "cancelled" | null> {
  if (typeof viewOnce !== "boolean") return null;
  try {
    const value = await invoke<unknown>("select_osl_chat_attachment", { viewOnce });
    if (value === null) return "cancelled";
    return parseNativeOverlayPreparedAttachment(value);
  } catch (error) { recordBackendFailure("select_osl_chat_attachment", error); return null; }
}

export async function listOslChatAttachments(): Promise<NativeOverlayPendingAttachment[] | null> {
  try {
    const value = await invoke<unknown>("list_osl_chat_attachments");
    if (!Array.isArray(value) || value.length > 64) return null;
    const parsed = value.map(parseNativeOverlayPendingAttachment);
    return parsed.some((entry) => entry === null) ? null : parsed as NativeOverlayPendingAttachment[];
  } catch (error) { recordBackendFailure("list_osl_chat_attachments", error); return null; }
}

export async function openOslChatAttachment(attachmentId: string): Promise<NativeOverlayOpenedAttachment | null> {
  if (!/^[A-Za-z0-9_-]{8,128}$/u.test(attachmentId)) return null;
  try { return parseNativeOverlayOpenedAttachment(await invoke<unknown>("open_osl_chat_attachment", { attachmentId })); }
  catch (error) { recordBackendFailure("open_osl_chat_attachment", error); return null; }
}
