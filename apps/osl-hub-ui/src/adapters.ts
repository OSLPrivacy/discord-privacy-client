import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./preferences";
// Every adapter below still fails closed. The backend's own refusal message is
// no longer thrown away with the exception: it is recorded, sanitized and
// unchanged, so a developer can see why a command was refused instead of
// guessing from a generic sentence. See ./backend-failure.ts.
import { checkedBackendResponse, recordBackendFailure, recordInvalidBackendResponse } from "./backend-failure";
import {
  parseNativeDiscordOverlayOpenedBatch,
  type NativeDiscordOverlayOpenedBatch,
} from "./overlay-state";
import {
  REVOCATION_STATUS_ACKNOWLEDGED,
  REVOCATION_STATUS_NOT_ACKNOWLEDGED,
  REVOCATION_STATUS_SENT_REQUEST,
  type HubRevocationStatus,
  type HubScopeBurnOutcome,
} from "./burn-revocation-receipt";

export type { HubRevocationStatus, HubScopeBurnOutcome };

export interface FriendProfile { friendCode: string; oslUserId: string; safetyNumber: string; }
export interface AppNotification { id: string; title: string; detail: string; createdAt: string; }
export type SupportMatrixPublicStatus = "available" | "beta" | "coming_soon" | "externally_blocked" | "unsupported";
export type SupportMatrixInputEvidenceStatus = "qualified_profile" | "runtime_proven" | "qa_foundations_only" | "separate_qa_required" | "externally_blocked" | "unsupported" | "unknown";
export interface SupportMatrixPresentationInput {
  publicStatus: SupportMatrixPublicStatus | "comingSoon" | "externallyBlocked";
  evidenceStatus?: SupportMatrixInputEvidenceStatus | string;
  claimAllowed?: boolean;
}
export interface SupportMatrixPublicPresentation {
  state: "available" | "limited" | "comingSoon" | "blocked" | "unsupported";
  label: string;
  detail: string;
  action: string;
}
export interface LocalProtectedText {
  capsule: string;
  localMessageId: string;
  protection: "local_protected_loopback";
  personToPersonE2ee: false;
  statePersisted: true;
  viewOnce: boolean;
}
export interface DecryptedLocalProtectedText {
  plaintext: string;
  localMessageId: string;
  protection: "local_protected_loopback";
  personToPersonE2ee: false;
  contextVerified: true;
  viewOnceConsumed: boolean;
}
export interface LocalLoopbackContext {
  contextToken: string;
  serviceId: string;
  accountId: string;
  conversationId: string;
}
export interface ManualPeerContext {
  contextToken: string;
  serviceId: string;
  accountId: string;
  personId: string;
  peerOslUserId: string;
  scopeApproved: boolean;
}
export interface PreparedPeerProseText {
  coverText: string;
  expiresAt: number;
  personToPersonE2ee: true;
  viewOnce: boolean;
}
export interface OpenedPeerProseText {
  plaintext: string;
  contextVerified: true;
  personToPersonE2ee: true;
  viewOnceConsumed: boolean;
  requireCaptureProtection: boolean;
}
export interface PreparedOslChatText {
  messageId: string;
  expiresAt: number;
  personToPersonE2ee: true;
  viewOnce: boolean;
  deliveredToOslInbox: true;
}
export interface OslChatHistoryRow {
  messageId: string;
  senderOslUserId: string;
  plaintext: string;
  decryptedAt: number;
}
export interface PreparedHubAttachment {
  sealedB64: string;
  transportFilename: string;
  transportMimeType: "video/mp4";
  originalMimeType: string;
  ciphertextPrepared: true;
  automaticServiceUpload: false;
}
export interface OpenedHubAttachment {
  plaintextB64: string;
  originalFilename: string;
  mimeType: string;
  contextVerified: true;
}
export interface PreparedEncryptedText {
  messages: string[];
  controlMessages: string[];
  sessionId: number | null;
}
export interface HubIdentitySlot { slotId: string; label: string; oslUserId: string; active: boolean; }
export interface HubIdentityCreation { identity: HubIdentitySlot; identityRecoveryPhrase: string | null; storageMethod: string; }
export interface ScopeSecurity { storageKey: string; ttlSeconds: number; decryptDisplayEnabled: boolean; }
export interface HubPersonWhitelistScope { kind: "dm" | "group" | "channel" | "space"; contextId: string | null; storageKey: string; userSpecific: boolean; }
export interface HubPerson { personId: string; oslUserId: string; alias: string | null; safetyNumber: string; safetyNumberVerified: boolean; whitelistCount: number; whitelistedScopes: HubPersonWhitelistScope[]; whitelistedScopesTruncated: boolean; pendingKeyChange: boolean; reachBroadened: boolean; reachBroadenedAt: string | null; reachNarrowedScopes: string[]; }
export interface HubUsernameClaim { username: string; oslUserId: string; }
export interface HubUsernameStatus { username: string; ownedByActiveIdentity: boolean; }
export interface HubAddFriendResult {
  disposition: "added" | "already_present" | "key_change_requires_verification";
  personId: string;
  oslUserId: string;
  safetyNumber: string;
  codeSignatureValid: true;
  safetyNumberVerified: boolean;
}
export type OslProfileFrame = "none" | "thin" | "double" | "glow";
export type OslProfileEffect = "none" | "gradient" | "pulse" | "shimmer";
export interface OslProfile {
  displayName: string;
  usernameCandidate: string;
  avatar: string | null;
  accentColor: string;
  bannerColor: string;
  frame: OslProfileFrame;
  effect: OslProfileEffect;
  status: string;
}
export interface HubFullCleanupResult {
  localCleanupComplete: boolean;
  removedTargets: string[];
  failedTargets: string[];
  remoteUnregister: { identitiesFound: number; succeeded: number; failed: number; unavailable: number };
  restartRequired: boolean;
  originalDiscordDataUntouched: true;
}
export interface HubServiceBurnReadiness {
  burnId: string;
  manifestDigest: string;
  indexedScopes: number;
  coverageComplete: boolean;
  loginProfileUntouched: true;
  nativeHistoryUntouched: true;
}
export interface HubServiceBurnResult {
  burnId: string;
  scopesBurned: number;
  rowsDestroyed: number;
  whitelistEntriesRemoved: number;
  remoteBlobsDeleted: number;
  remoteBlobDeletionsFailed: number;
  localCleanupComplete: boolean;
  remoteCleanupComplete: boolean;
  loginProfileUntouched: true;
  nativeHistoryUntouched: true;
}
export type PrivacyRiskCategory = "credential" | "recovery_material" | "payment_card" | "government_identity" | "precise_location" | "profanity" | "sexual_content" | "sensitive_health" | "controlled_substances" | "potentially_unlawful_conduct" | "work_sensitive_information";
export interface LocalMessageCandidate {
  serviceId: string;
  accountId: string;
  conversationId: string;
  messageLocator: string;
  authoredBySelf: boolean;
  createdAtUnixMs: number | null;
  text: string;
  attachments?: LocalAttachmentCandidate[];
}
export interface LocalAttachmentCandidate {
  attachmentId: string;
  displayName: string;
  contentBase64: string;
}
export interface LocalPrivacyFinding extends Omit<LocalMessageCandidate, "text"> {
  category: PrivacyRiskCategory;
  confidence: number;
  reason: string;
  localPreview: string;
  canRequestDelete: boolean;
  attachmentPath?: string | null;
}
export type UninspectedAttachmentReason = "unsupported" | "encrypted" | "malformed" | "limit_exceeded" | "unsafe_archive_entry" | "model_not_installed" | "dependency_not_installed" | "image_only_pdf_needs_ocr";
export interface UninspectedAttachment {
  attachmentId: string;
  path: string;
  detectedType: string;
  reason: UninspectedAttachmentReason;
  detail: string;
}
export interface LocalPrivacyScanResult {
  findings: LocalPrivacyFinding[];
  messagesScanned: number;
  messagesRejected: number;
  truncated: boolean;
  analysisLocation: "this_device_only";
  persisted: false;
  attachmentsScanned: number;
  imagesChecked: boolean;
  videosChecked: boolean;
  attachmentTypesScanned: string[];
  uninspectedAttachments: UninspectedAttachment[];
}
export interface PersistedLocalPrivacyScanResult extends Omit<LocalPrivacyScanResult, "persisted"> {
  persisted: true;
}
export interface ReviewedItemIdentity {
  reviewId: string;
  serviceId: string;
  accountId: string;
  conversationId: string;
  messageLocator: string;
}
export interface ReviewedItemIdentityBinding extends ReviewedItemIdentity {
  selected: true;
}
export type SupportMatrixRowStatus =
  | "supported"
  | "verified_live"
  | "runtime_proven"
  | "test_proven_only"
  | "implemented_unwired"
  | "designed_only"
  | "externally_blocked"
  | "unsupported";
export type SupportMatrixPresentationTone = "available" | "beta" | "planned" | "blocked" | "unavailable";
export interface SupportMatrixRowPresentation {
  service: string;
  label: string;
  detail: string;
  tone: SupportMatrixPresentationTone;
  publicClaimAllowed: boolean;
}
export type SupportMatrixPresentation = SupportMatrixPublicPresentation | SupportMatrixRowPresentation;

export const LOCAL_PROTECTED_TEXT_MAX_BYTES = 1_000;
export const HUB_PLAINTEXT_MAX_BYTES = 1_000;
export const HUB_CAPSULE_MAX_BYTES = 256 * 1024;
export const HUB_ATTACHMENT_B64_MAX_CHARACTERS = 32 * 1024 * 1024;
export const HUB_ATTACHMENT_FILENAME_MAX_BYTES = 1_024;
const HUB_PREPARED_MESSAGE_MAX_ITEMS = 64;
const HUB_CONTROL_MESSAGE_MAX_ITEMS = 512;
const HUB_PREPARED_TOTAL_MAX_BYTES = 4 * 1024 * 1024;

export function supportMatrixPresentation(input: SupportMatrixPresentationInput): SupportMatrixPublicPresentation;
export function supportMatrixPresentation(row: unknown): SupportMatrixRowPresentation | null;
export function supportMatrixPresentation(input: unknown): SupportMatrixPublicPresentation | SupportMatrixRowPresentation | null {
  if (isRecord(input) && typeof input.publicStatus === "string") {
    return supportMatrixPublicPresentation(input as unknown as SupportMatrixPresentationInput);
  }
  return supportMatrixRowPresentation(input);
}

function supportMatrixPublicPresentation(input: SupportMatrixPresentationInput): SupportMatrixPublicPresentation {
  const publicStatus = normalizeSupportPublicStatus(input.publicStatus);
  const evidenceStatus = typeof input.evidenceStatus === "string" ? input.evidenceStatus : "unknown";
  const claimAllowed = input.claimAllowed === true;

  if (publicStatus === "available" && claimAllowed) {
    return {
      state: "available",
      label: "Available",
      detail: "Protected use is available after OSL verifies the app, account and conversation.",
      action: "Open",
    };
  }

  if (publicStatus === "beta" && claimAllowed) {
    return {
      state: "limited",
      label: "Beta",
      detail: "Protected use is available with extra checks and visible limits.",
      action: "Open",
    };
  }

  if (publicStatus === "externally_blocked" || evidenceStatus === "externally_blocked") {
    return {
      state: "blocked",
      label: "Blocked",
      detail: "This app cannot be protected until its visible message area can be verified reliably.",
      action: "Use OSL Chat",
    };
  }

  if (publicStatus === "unsupported" || evidenceStatus === "unsupported") {
    return {
      state: "unsupported",
      label: "Unsupported",
      detail: "Protected use is not available for this app.",
      action: "Use OSL Chat",
    };
  }

  return {
    state: "comingSoon",
    label: "Coming soon",
    detail: "Not ready for protected use yet. OSL will not send or claim support here.",
    action: "Use OSL Chat",
  };
}

function normalizeSupportPublicStatus(
  status: SupportMatrixPresentationInput["publicStatus"],
): SupportMatrixPublicStatus {
  if (status === "comingSoon") return "coming_soon";
  if (status === "externallyBlocked") return "externally_blocked";
  return status;
}

function supportMatrixRowPresentation(row: unknown): SupportMatrixRowPresentation | null {
  if (!isRecord(row) || typeof row.service !== "string" || !safePlaintext(row.service, 80)) return null;
  const status = normalizeSupportMatrixStatus(row.status);
  if (!status) return null;
  const service = row.service;
  const claimScope = typeof row.claim_scope === "string" ? row.claim_scope : "";

  if (service === "Outlook" && claimScope === "osl_mail") {
    return {
      service: "OSL Mail",
      label: "Coming later",
      detail: "Protected Outlook replies are unavailable until mailbox, recipient, draft, and send checks are complete.",
      tone: "planned",
      publicClaimAllowed: false,
    };
  }

  switch (status) {
    case "supported":
    case "verified_live":
      return {
        service,
        label: "Supported",
        detail: `${service} can be shown as supported for the current release.`,
        tone: "available",
        publicClaimAllowed: true,
      };
    case "runtime_proven":
    case "test_proven_only":
      return {
        service,
        label: "Beta",
        detail: `${service} has testing proof, but not enough current-release evidence for a full support claim.`,
        tone: "beta",
        publicClaimAllowed: false,
      };
    case "implemented_unwired":
    case "designed_only":
      return {
        service,
        label: "Coming later",
        detail: `${service} is planned, but protected use is not available in this release.`,
        tone: "planned",
        publicClaimAllowed: false,
      };
    case "externally_blocked":
      return {
        service,
        label: "Externally blocked",
        detail: `${service} cannot be offered until OSL can confirm a reliable conversation view.`,
        tone: "blocked",
        publicClaimAllowed: false,
      };
    case "unsupported":
      return {
        service,
        label: "Unavailable",
        detail: `${service} is not available for protected use in this release.`,
        tone: "unavailable",
        publicClaimAllowed: false,
      };
  }
}

export function isLocalProtectedPlaintext(value: unknown): value is string {
  return typeof value === "string"
    && value.length > 0
    && new TextEncoder().encode(value).length <= LOCAL_PROTECTED_TEXT_MAX_BYTES
    && !/[\u0000\u007f]/.test(value);
}

export function isHubPlaintext(value: unknown): value is string {
  return boundedUtf8Text(value, HUB_PLAINTEXT_MAX_BYTES);
}

/**
 * Bind a manually named, single-device context to the exact hosted profile.
 * The human label is deliberately never passed to native code; only a random
 * opaque conversation id crosses IPC.
 */
export async function activateLocalLoopbackContext(
  serviceId: string,
  accountId: string,
  conversationId: string,
): Promise<LocalLoopbackContext | null> {
  if (!isTauriRuntime()
    || !safeId(serviceId, 32)
    || !isContextId(accountId)
    || !isContextId(conversationId)) return null;
  try {
    const parsed = parseLocalLoopbackContext(await invoke<unknown>("activate_local_loopback_context", {
      serviceId,
      accountId,
      conversationId,
    }));
    return checkedBackendResponse("activate_local_loopback_context", parsed?.serviceId === serviceId
      && parsed.accountId === accountId
      && parsed.conversationId === conversationId
      ? parsed
      : null, "the activated context did not match the requested one");
  } catch (error) { recordBackendFailure("activate_local_loopback_context", error); return null; }
}

export async function activateManualPeerContext(
  serviceId: string,
  accountId: string,
  personId: string,
): Promise<ManualPeerContext | null> {
  if (!isTauriRuntime() || !safeId(serviceId, 32) || !isContextId(accountId) || !safe(personId, 180)) return null;
  try {
    const parsed = parseManualPeerContext(await invoke<unknown>("activate_manual_peer_context", {
      serviceId,
      accountId,
      personId,
    }));
    return checkedBackendResponse("activate_manual_peer_context", parsed?.serviceId === serviceId
      && parsed.accountId === accountId
      && parsed.personId === personId
      ? parsed
      : null, "the activated context did not match the requested one");
  } catch (error) { recordBackendFailure("activate_manual_peer_context", error); return null; }
}

/** Bind one verified friend to the currently claimed native Discord host. */
export async function activateNativeManualPeerContext(personId: string): Promise<ManualPeerContext | null> {
  if (!isTauriRuntime() || !safe(personId, 180)) return null;
  try {
    const parsed = parseManualPeerContext(await invoke<unknown>("activate_native_manual_peer_context", { personId }));
    return checkedBackendResponse("activate_native_manual_peer_context",
      parsed?.serviceId === "discord" && parsed.personId === personId ? parsed : null,
      "the activated context did not match the requested one");
  } catch (error) { recordBackendFailure("activate_native_manual_peer_context", error); return null; }
}

/** Bind one verified friend to the fixed first-party OSL Chat context. */
export async function activateOslChatContext(personId: string): Promise<ManualPeerContext | null> {
  if (!isTauriRuntime() || !safe(personId, 180)) return null;
  const person = await verifiedStableOslChatPerson(personId);
  if (!person) return null;
  try {
    const parsed = parseManualPeerContext(await invoke<unknown>("activate_osl_chat_context", { personId }));
    return checkedBackendResponse("activate_osl_chat_context", parsed?.serviceId === "osl-chat"
      && parsed.accountId === "osl-main"
      && parsed.personId === personId
      && parsed.peerOslUserId === person.oslUserId ? parsed : null,
      "the activated context did not match the requested one");
  } catch (error) { recordBackendFailure("activate_osl_chat_context", error); return null; }
}

async function verifiedStableOslChatPerson(personId: string): Promise<HubPerson | null> {
  const people = await listHubPeople();
  if (!people) return null;
  const person = people.find((candidate) => candidate.personId === personId) ?? null;
  return person?.safetyNumberVerified && !person.pendingKeyChange ? person : null;
}

export async function closeOslChatContext(): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  try { await invoke("close_osl_chat_context"); return true; }
  catch (error) { recordBackendFailure("close_osl_chat_context", error); return false; }
}

export async function prepareOslChatText(plaintext: string, viewOnce = false): Promise<PreparedOslChatText | null> {
  if (!isTauriRuntime() || !isHubPlaintext(plaintext) || typeof viewOnce !== "boolean") return null;
  try {
    return checkedBackendResponse("prepare_osl_chat_text",
      parsePreparedOslChatText(await invoke<unknown>("prepare_osl_chat_text", { plaintext, viewOnce })),
      "the prepared message did not match the expected shape");
  } catch (error) { recordBackendFailure("prepare_osl_chat_text", error, [plaintext]); return null; }
}

export async function openOslChatText(): Promise<NativeDiscordOverlayOpenedBatch | null> {
  if (!isTauriRuntime()) return null;
  try {
    return checkedBackendResponse("open_osl_chat_text",
      parseNativeDiscordOverlayOpenedBatch(await invoke<unknown>("open_osl_chat_text")),
      "the opened batch did not match the expected shape");
  }
  catch (error) { recordBackendFailure("open_osl_chat_text", error); return null; }
}

export async function listOslChatHistory(): Promise<OslChatHistoryRow[] | null> {
  if (!isTauriRuntime()) return null;
  try {
    const value = await invoke<unknown>("list_osl_chat_history");
    if (!Array.isArray(value) || value.length > 200) return null;
    const rows = value.map((entry) => {
      if (!isRecord(entry)
        || !exact(entry, ["discord_message_id", "channel_id", "sender_discord_id", "sender_osl_user_id", "plaintext", "decrypted_at", "burned"])
        || !safe(entry.discord_message_id, 96)
        || !isContextId(entry.channel_id)
        || !isContextId(entry.sender_osl_user_id)
        || !isHubPlaintext(entry.plaintext)
        || !Number.isSafeInteger(entry.decrypted_at)
        || Number(entry.decrypted_at) <= 0
        || typeof entry.burned !== "boolean") return null;
      return {
        messageId: entry.discord_message_id as string,
        senderOslUserId: entry.sender_osl_user_id as string,
        plaintext: entry.plaintext as string,
        decryptedAt: entry.decrypted_at as number,
      };
    });
    return checkedBackendResponse("list_osl_chat_history",
      rows.some((row) => row === null) ? null : rows as OslChatHistoryRow[],
      "a history row did not match the expected shape");
  } catch (error) { recordBackendFailure("list_osl_chat_history", error); return null; }
}

export async function preparePeerProseText(
  contextToken: string,
  plaintext: string,
  viewOnce: boolean,
): Promise<PreparedPeerProseText | null> {
  if (!isTauriRuntime() || !safeContextToken(contextToken) || !isHubPlaintext(plaintext) || typeof viewOnce !== "boolean") return null;
  try {
    return checkedBackendResponse("prepare_peer_prose_text",
      parsePreparedPeerProseText(await invoke<unknown>("prepare_peer_prose_text", {
        contextToken,
        plaintext,
        viewOnce,
      })),
      "the prepared message did not match the expected shape");
  } catch (error) { recordBackendFailure("prepare_peer_prose_text", error, [plaintext]); return null; }
}

export async function openPeerProseText(
  contextToken: string,
  senderPersonId: string,
  coverText: string,
): Promise<OpenedPeerProseText | null> {
  if (!isTauriRuntime()
    || !safeContextToken(contextToken)
    || !safe(senderPersonId, 180)
    || !boundedUtf8Text(coverText, HUB_CAPSULE_MAX_BYTES)) return null;
  try {
    return checkedBackendResponse("open_peer_prose_text",
      parseOpenedPeerProseText(await invoke<unknown>("open_peer_prose_text", {
        contextToken,
        senderPersonId,
        coverText,
      })),
      "the opened message did not match the expected shape");
  } catch (error) { recordBackendFailure("open_peer_prose_text", error, [coverText]); return null; }
}

/** Reserve or release an OSL-owned side-sheet region beside the remote child. */
export async function setLocalProtectedSheetOpen(open: boolean): Promise<boolean> {
  if (!isTauriRuntime() || typeof open !== "boolean") return false;
  try { return await invoke<boolean>("set_local_protected_sheet_open", { open }) === true; }
  catch (error) { recordBackendFailure("set_local_protected_sheet_open", error); return false; }
}

/** Show or hide only the OSL-owned native Discord protection overlay. */
export async function setNativeDiscordProtectedOverlayOpen(contextToken: string, open: boolean): Promise<boolean> {
  if (!isTauriRuntime() || !safeContextToken(contextToken) || typeof open !== "boolean") return false;
  try {
    const confirmed = await invoke<boolean>("set_native_discord_protected_overlay_open", { contextToken, open }) === true;
    if (!confirmed) {
      recordInvalidBackendResponse("set_native_discord_protected_overlay_open",
        "the overlay did not confirm the requested change");
    }
    return confirmed;
  } catch (error) { recordBackendFailure("set_native_discord_protected_overlay_open", error); return false; }
}

export type NativeDiscordOverlayQaResult =
  | { opened: true; error: null }
  | { opened: false; error: string };

/** Preserve the native rejection only for the compile-time Discord QA shell. */
export async function setNativeDiscordProtectedOverlayOpenForQa(
  contextToken: string,
): Promise<NativeDiscordOverlayQaResult> {
  if (!isTauriRuntime() || !safeContextToken(contextToken)) {
    return { opened: false, error: "The native Discord overlay request was invalid" };
  }
  try {
    const opened = await invoke<boolean>("set_native_discord_protected_overlay_open", {
      contextToken,
      open: true,
    });
    if (opened === true) return { opened: true, error: null };
    recordInvalidBackendResponse("set_native_discord_protected_overlay_open",
      "the overlay did not confirm that it opened");
    return { opened: false, error: "The native Discord overlay did not confirm that it opened" };
  } catch (failure) {
    recordBackendFailure("set_native_discord_protected_overlay_open", failure);
    const raw = typeof failure === "string" ? failure : failure instanceof Error ? failure.message : "";
    const error = raw.replace(/[\u0000-\u001f\u007f]/gu, " ").replace(/\s+/gu, " ").trim();
    return {
      opened: false,
      error: error && error.length <= 240
        ? error
        : "The native Discord overlay operation failed without a usable error",
    };
  }
}

export async function prepareEncryptedText(contextToken: string, plaintext: string): Promise<PreparedEncryptedText | null> {
  if (!isTauriRuntime() || !safe(contextToken, 180) || !isHubPlaintext(plaintext)) return null;
  try {
    return checkedBackendResponse("prepare_encrypted_text",
      parsePreparedEncryptedText(await invoke<unknown>("prepare_encrypted_text", { contextToken, plaintext })),
      "the prepared message did not match the expected shape");
  } catch (error) { recordBackendFailure("prepare_encrypted_text", error, [plaintext]); return null; }
}

export async function decryptHubCapsule(
  contextToken: string,
  senderOslId: string,
  serviceMessageId: string | null,
  capsule: string,
): Promise<string | null> {
  if (!isTauriRuntime()
    || !safe(contextToken, 180)
    || !boundedUtf8Text(senderOslId, 160)
    || !(serviceMessageId === null || boundedUtf8Text(serviceMessageId, 180))
    || !boundedUtf8Text(capsule, HUB_CAPSULE_MAX_BYTES)) return null;
  try {
    return checkedBackendResponse("decrypt_hub_capsule",
      parseDecryptedHubPlaintext(await invoke<unknown>("decrypt_hub_capsule", {
        contextToken,
        senderOslId,
        serviceMessageId,
        capsule,
      })),
      "the decrypted result did not match the expected shape");
  } catch (error) { recordBackendFailure("decrypt_hub_capsule", error, [capsule]); return null; }
}

/**
 * Prepare a single-device, context-bound message. `viewOnce` is enforced by
 * the local ledger; this does not claim peer E2EE, remote deletion, or
 * screenshot prevention.
 */
export async function prepareLocalProtectedText(
  contextToken: string,
  plaintext: string,
  viewOnce = false,
): Promise<LocalProtectedText | null> {
  if (!isTauriRuntime()
    || !safeContextToken(contextToken)
    || !isLocalProtectedPlaintext(plaintext)
    || typeof viewOnce !== "boolean") return null;
  try {
    return checkedBackendResponse("prepare_local_protected_text_with_policy",
      parseLocalProtectedText(await invoke<unknown>("prepare_local_protected_text_with_policy", {
        contextToken,
        plaintext,
        viewOnce,
      })),
      "the prepared message did not match the expected shape");
  } catch (error) { recordBackendFailure("prepare_local_protected_text_with_policy", error, [plaintext]); return null; }
}

export async function decryptLocalProtectedText(
  contextToken: string,
  capsule: string,
): Promise<DecryptedLocalProtectedText | null> {
  if (!isTauriRuntime() || !safeContextToken(contextToken) || !boundedUtf8Text(capsule, HUB_CAPSULE_MAX_BYTES)) return null;
  try {
    return checkedBackendResponse("decrypt_local_protected_capsule",
      parseDecryptedLocalProtectedText(await invoke<unknown>("decrypt_local_protected_capsule", {
        contextToken,
        capsule,
      })),
      "the decrypted result did not match the expected shape");
  } catch (error) { recordBackendFailure("decrypt_local_protected_capsule", error, [capsule]); return null; }
}

/** Seal one bounded attachment for the exact active trusted context. */
export async function prepareHubAttachment(
  contextToken: string,
  originalBytesB64: string,
  originalFilename: string,
): Promise<PreparedHubAttachment | null> {
  if (!isTauriRuntime()
    || !safeContextToken(contextToken)
    || !isBoundedBase64(originalBytesB64)
    || !isAttachmentFilename(originalFilename)) return null;
  try {
    return checkedBackendResponse("prepare_hub_attachment",
      parsePreparedHubAttachment(await invoke<unknown>("prepare_hub_attachment", {
        contextToken,
        originalBytesB64,
        originalFilename,
      })),
      "the sealed attachment did not match the expected shape");
  } catch (error) { recordBackendFailure("prepare_hub_attachment", error, [originalBytesB64, originalFilename]); return null; }
}

/** Open one attachment only through the current context-bound broker lease. */
export async function openHubAttachment(
  contextToken: string,
  senderOslId: string,
  serviceMessageId: string | null,
  sealedB64: string,
): Promise<OpenedHubAttachment | null> {
  if (!isTauriRuntime()
    || !safeContextToken(contextToken)
    || !isContextId(senderOslId)
    || !(serviceMessageId === null || isContextId(serviceMessageId))
    || !isBoundedBase64(sealedB64)) return null;
  try {
    return checkedBackendResponse("open_hub_attachment",
      parseOpenedHubAttachment(await invoke<unknown>("open_hub_attachment", {
        contextToken,
        senderOslId,
        serviceMessageId,
        sealedB64,
      })),
      "the opened attachment did not match the expected shape");
  } catch (error) { recordBackendFailure("open_hub_attachment", error, [sealedB64]); return null; }
}

export async function loadFriendProfile(): Promise<FriendProfile | null> {
  if (!isTauriRuntime()) return null;
  try {
    const raw = await invoke<unknown>("export_hub_friend_code");
    return checkedBackendResponse("export_hub_friend_code", parseFriendProfile(raw),
      "the friend profile did not match the expected shape");
  } catch (error) { recordBackendFailure("export_hub_friend_code", error); return null; }
}

/**
 * In the native app Rust creates and copies the signed invite without
 * accepting renderer-controlled clipboard content. Browser development uses
 * the already-validated profile string as its narrow fallback.
 */
export async function copyHubFriendInvite(friendCode: string): Promise<boolean> {
  if (!/^OSLFR1\.[A-Za-z0-9_-]{16,8192}$/.test(friendCode)) return false;
  if (isTauriRuntime()) {
    try {
      await invoke("copy_hub_friend_invite");
      return true;
    } catch (error) { recordBackendFailure("copy_hub_friend_invite", error); return false; }
  }
  try {
    if (!navigator.clipboard?.writeText) return false;
    await navigator.clipboard.writeText(friendCode);
    return true;
  } catch { return false; }
}

export async function addOslFriend(code: string, nickname = ""): Promise<boolean> {
  const trimmed = nickname.trim();
  if (!isTauriRuntime() || !/^OSLFR1\.[A-Za-z0-9_-]{16,8192}$/.test(code) || !validFriendNickname(trimmed)) return false;
  try { await invoke("add_hub_friend", { friendCode: code, alias: trimmed || null }); return true; }
  catch (error) { recordBackendFailure("add_hub_friend", error, [code, trimmed]); return false; }
}

export async function listHubPeople(): Promise<HubPerson[] | null> {
  if (!isTauriRuntime()) return null;
  try {
    const raw = await invoke<unknown>("list_hub_people");
    if (!Array.isArray(raw) || raw.length > 1_024) return null;
    const people = raw.map(parseHubPerson);
    return checkedBackendResponse("list_hub_people",
      people.every((person): person is HubPerson => person !== null) ? people : null,
      "a person row did not match the expected shape");
  } catch (error) { recordBackendFailure("list_hub_people", error); return null; }
}

export async function setHubFriendNickname(personId: string, nickname: string): Promise<HubPerson | null> {
  const trimmed = nickname.trim();
  if (!isTauriRuntime() || !safe(personId, 180) || !validFriendNickname(trimmed)) return null;
  try {
    return checkedBackendResponse("set_hub_friend_nickname",
      parseHubPerson(await invoke<unknown>("set_hub_friend_nickname", {
        personId,
        nickname: trimmed || null,
      })),
      "the person row did not match the expected shape");
  } catch (error) { recordBackendFailure("set_hub_friend_nickname", error, [trimmed]); return null; }
}

export async function verifyHubPerson(personId: string, safetyNumber: string): Promise<boolean> {
  if (!isTauriRuntime() || !safe(personId, 180) || !safe(safetyNumber, 180)) return false;
  try { await invoke("verify_hub_friend_safety_number", { personId, safetyNumber }); return true; }
  catch (error) { recordBackendFailure("verify_hub_friend_safety_number", error, [safetyNumber]); return false; }
}

export async function setActiveHubFriendPermission(contextToken: string, personId: string, enabled: boolean, broadened = false): Promise<boolean> {
  if (!isTauriRuntime() || !safe(contextToken, 180) || !safe(personId, 180) || typeof broadened !== "boolean") return false;
  try { await invoke("set_active_hub_friend_permission", { contextToken, personId, enabled, broadened }); return true; }
  catch (error) { recordBackendFailure("set_active_hub_friend_permission", error); return false; }
}

// Widen or withdraw one verified friend's reach across the scopes shared with
// them. A separate command from the ordinary approval above, so reach is only
// ever changed by this deliberate action.
export async function setActiveHubFriendReach(contextToken: string, personId: string, broadened: boolean): Promise<HubPerson | null> {
  if (!isTauriRuntime() || !safe(contextToken, 180) || !safe(personId, 180) || typeof broadened !== "boolean") return null;
  try {
    return checkedBackendResponse("set_active_hub_friend_reach",
      parseHubPerson(await invoke<unknown>("set_active_hub_friend_reach", { contextToken, personId, broadened })),
      "the person row did not match the expected shape");
  }
  catch (error) { recordBackendFailure("set_active_hub_friend_reach", error); return null; }
}

// Revoke exactly one approval OSL recorded for this friend. The key is one OSL
// itself reported in whitelistedScopes; the backend accepts nothing else.
export async function revokeActiveHubFriendScope(contextToken: string, personId: string, storageKey: string): Promise<HubPerson | null> {
  if (!isTauriRuntime() || !safe(contextToken, 180) || !safe(personId, 180) || !safe(storageKey, 512)) return null;
  try {
    return checkedBackendResponse("revoke_active_hub_friend_scope",
      parseHubPerson(await invoke<unknown>("revoke_active_hub_friend_scope", { contextToken, personId, storageKey })),
      "the person row did not match the expected shape");
  }
  catch (error) { recordBackendFailure("revoke_active_hub_friend_scope", error); return null; }
}

export function parseHubPerson(raw: unknown): HubPerson | null {
  if (!isRecord(raw) || !exact(raw, ["personId", "oslUserId", "alias", "safetyNumber", "safetyNumberVerified", "whitelistCount", "whitelistedScopes", "whitelistedScopesTruncated", "pendingKeyChange", "reachBroadened", "reachBroadenedAt", "reachNarrowedScopes"])) return null;
  if (typeof raw.reachBroadened !== "boolean" || !(raw.reachBroadenedAt === null || safe(raw.reachBroadenedAt, 64))) return null;
  if (!Array.isArray(raw.reachNarrowedScopes) || raw.reachNarrowedScopes.length > 512 || !raw.reachNarrowedScopes.every((key) => safePlaintext(key, 512))) return null;
  if (!safe(raw.personId, 180) || !safe(raw.oslUserId, 180) || !(raw.alias === null || safe(raw.alias, 80)) || !safe(raw.safetyNumber, 180)) return null;
  if (typeof raw.safetyNumberVerified !== "boolean" || !Number.isSafeInteger(raw.whitelistCount) || Number(raw.whitelistCount) < 0 || Number(raw.whitelistCount) > 1_000_000 || typeof raw.whitelistedScopesTruncated !== "boolean" || typeof raw.pendingKeyChange !== "boolean") return null;
  if (!Array.isArray(raw.whitelistedScopes) || raw.whitelistedScopes.length > 512) return null;
  const whitelistedScopes = raw.whitelistedScopes.map(parseHubPersonWhitelistScope);
  if (!whitelistedScopes.every((scope): scope is HubPersonWhitelistScope => scope !== null)) return null;
  return { ...raw, whitelistedScopes } as unknown as HubPerson;
}

export function isNormalizedOslUsername(value: unknown): value is string {
  return typeof value === "string"
    && value.length >= 3
    && value.length <= 30
    && /^[a-z0-9](?:[a-z0-9_]{1,28}[a-z0-9])?$/u.test(value);
}

export function parseHubUsernameClaim(raw: unknown): HubUsernameClaim | null {
  if (!isRecord(raw) || !exact(raw, ["username", "oslUserId"])) return null;
  if (!isNormalizedOslUsername(raw.username) || !safePlaintext(raw.oslUserId, 180)) return null;
  return raw as unknown as HubUsernameClaim;
}

export function parseHubAddFriendResult(raw: unknown): HubAddFriendResult | null {
  if (!isRecord(raw) || !exact(raw, ["disposition", "personId", "oslUserId", "safetyNumber", "codeSignatureValid", "safetyNumberVerified"])) return null;
  if (!["added", "already_present", "key_change_requires_verification"].includes(String(raw.disposition))
    || !safePlaintext(raw.personId, 180)
    || !safePlaintext(raw.oslUserId, 180)
    || !safePlaintext(raw.safetyNumber, 180)
    || raw.codeSignatureValid !== true
    || typeof raw.safetyNumberVerified !== "boolean") return null;
  if (raw.disposition === "added" && raw.safetyNumberVerified) return null;
  return raw as unknown as HubAddFriendResult;
}

export async function claimOslUsername(username: string): Promise<HubUsernameClaim | null> {
  if (!isTauriRuntime() || !isNormalizedOslUsername(username)) return null;
  try {
    const parsed = parseHubUsernameClaim(await invoke<unknown>("claim_hub_username", { username }));
    return checkedBackendResponse("claim_hub_username",
      parsed?.username === username ? parsed : null,
      "the username claim did not match the expected shape");
  } catch (error) { recordBackendFailure("claim_hub_username", error); return null; }
}

export async function getOslUsernameStatus(username: string): Promise<HubUsernameStatus | null> {
  if (!isTauriRuntime() || !isNormalizedOslUsername(username)) return null;
  try {
    const raw = await invoke<unknown>("get_hub_username_status", { username });
    const parsed = isRecord(raw)
      && exact(raw, ["username", "ownedByActiveIdentity"])
      && raw.username === username
      && typeof raw.ownedByActiveIdentity === "boolean"
      ? raw as unknown as HubUsernameStatus
      : null;
    return checkedBackendResponse("get_hub_username_status", parsed,
      "the username status did not match the expected shape");
  } catch (error) { recordBackendFailure("get_hub_username_status", error); return null; }
}

export async function addOslFriendByUsername(username: string, alias = ""): Promise<HubAddFriendResult | null> {
  const trimmed = alias.trim();
  if (!isTauriRuntime() || !isNormalizedOslUsername(username) || !validFriendNickname(trimmed)) return null;
  try {
    return checkedBackendResponse("add_hub_friend_by_username",
      parseHubAddFriendResult(await invoke<unknown>("add_hub_friend_by_username", { username, alias: trimmed || null })),
      "the username friend result did not match the expected shape");
  } catch (error) { recordBackendFailure("add_hub_friend_by_username", error, [trimmed]); return null; }
}

export function parseOslProfile(raw: unknown): OslProfile | null {
  if (!isRecord(raw) || !exact(raw, ["displayName", "usernameCandidate", "avatar", "accentColor", "bannerColor", "frame", "effect", "status"])) return null;
  if (!boundedProfileText(raw.displayName, 64, 192, false)
    || !isNormalizedOslUsername(raw.usernameCandidate)
    || !(raw.avatar === null || validProfileAvatar(raw.avatar))
    || !validHexColor(raw.accentColor)
    || !validHexColor(raw.bannerColor)
    || !["none", "thin", "double", "glow"].includes(String(raw.frame))
    || !["none", "gradient", "pulse", "shimmer"].includes(String(raw.effect))
    || !boundedProfileText(raw.status, 160, 512, true)) return null;
  return raw as unknown as OslProfile;
}

function parseHubPersonWhitelistScope(raw: unknown): HubPersonWhitelistScope | null {
  if (!isRecord(raw) || !exact(raw, ["kind", "contextId", "storageKey", "userSpecific"])) return null;
  if (!["dm", "group", "channel", "space"].includes(String(raw.kind))) return null;
  if (!(raw.contextId === null || safePlaintext(raw.contextId, 512))) return null;
  if (!safePlaintext(raw.storageKey, 512) || typeof raw.userSpecific !== "boolean") return null;
  return raw as unknown as HubPersonWhitelistScope;
}

function validFriendNickname(value: string): boolean {
  if (!value) return true;
  return new TextEncoder().encode(value).length <= 80
    && Array.from(value).length <= 48
    && !/[<>\u0000-\u001f\u007f\u200b-\u200d\u202a-\u202e\u2060\u2066-\u2069]/u.test(value);
}

function boundedProfileText(value: unknown, maxChars: number, maxBytes: number, allowEmpty: boolean): value is string {
  return typeof value === "string"
    && (allowEmpty || value.trim().length > 0)
    && Array.from(value).length <= maxChars
    && new TextEncoder().encode(value).length <= maxBytes
    && !/[\u0000-\u001f\u007f]/u.test(value);
}

function validHexColor(value: unknown): value is string {
  return typeof value === "string" && /^#[0-9a-f]{6}$/u.test(value);
}

function validProfileAvatar(value: unknown): value is string {
  if (typeof value !== "string" || value.length > 2_800_000 || /\s/u.test(value)) return false;
  if (value.startsWith("data:")) {
    const match = /^data:(image\/(?:png|jpeg|webp|gif));base64,([A-Za-z0-9+/=]+)$/u.exec(value);
    if (!match) return false;
    return match[2].length > 0 && match[2].length <= 2_800_000;
  }
  try {
    const url = new URL(value);
    return url.protocol === "https:"
      && url.hostname.length > 0
      && url.username === ""
      && url.password === ""
      && url.hash === ""
      && value.length <= 2_048;
  } catch {
    return false;
  }
}

export async function loadAppNotifications(): Promise<AppNotification[] | null> {
  if (!isTauriRuntime()) return null;
  try {
    const raw = await invoke<unknown>("list_hub_app_notifications");
    return checkedBackendResponse("list_hub_app_notifications", parseNotifications(raw),
      "a notification did not match the expected shape");
  } catch (error) { recordBackendFailure("list_hub_app_notifications", error); return null; }
}

export async function setNotificationsEnabled(enabled: boolean): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  try { await invoke("set_hub_notifications_enabled", { enabled }); return true; }
  catch (error) { recordBackendFailure("set_hub_notifications_enabled", error); return false; }
}

export async function setScreenshotProtection(enabled: boolean): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  try { await invoke("set_hub_screenshot_protection", { enabled }); return true; }
  catch (error) { recordBackendFailure("set_hub_screenshot_protection", error); return false; }
}

export async function listHubIdentities(): Promise<HubIdentitySlot[] | null> {
  if (!isTauriRuntime()) return null;
  try {
    const raw = await invoke<unknown>("list_hub_identities");
    if (!Array.isArray(raw) || raw.length > 16) return null;
    const parsed = raw.map(parseIdentitySlot);
    return checkedBackendResponse("list_hub_identities",
      parsed.every((item): item is HubIdentitySlot => item !== null) ? parsed : null,
      "an identity slot did not match the expected shape");
  } catch (error) { recordBackendFailure("list_hub_identities", error); return null; }
}

export async function createHubIdentitySlot(label: string): Promise<HubIdentityCreation | null> {
  if (!isTauriRuntime() || !safe(label, 80)) return null;
  try {
    return checkedBackendResponse("create_hub_identity_slot",
      parseIdentityCreation(await invoke<unknown>("create_hub_identity_slot", { label })),
      "the created identity did not match the expected shape");
  }
  catch (error) { recordBackendFailure("create_hub_identity_slot", error, [label]); return null; }
}

export async function recoverHubIdentitySlot(label: string, identityRecoveryPhrase: string): Promise<HubIdentityCreation | null> {
  if (!isTauriRuntime() || !safe(label, 80) || !safePlaintext(identityRecoveryPhrase, 512)) return null;
  try {
    return checkedBackendResponse("recover_hub_identity_slot",
      parseIdentityCreation(await invoke<unknown>("recover_hub_identity_slot", { label, identityRecoveryPhrase })),
      "the recovered identity did not match the expected shape");
  }
  catch (error) { recordBackendFailure("recover_hub_identity_slot", error, [identityRecoveryPhrase, label]); return null; }
}

export async function switchHubIdentity(slotId: string): Promise<boolean> {
  if (!isTauriRuntime() || !/^[A-Za-z0-9_-]{8,80}$/.test(slotId)) return false;
  try { await invoke("switch_hub_identity", { slotId }); return true; }
  catch (error) { recordBackendFailure("switch_hub_identity", error); return false; }
}

export async function burnActiveHubIdentity(): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  try { await invoke("burn_active_hub_identity"); return true; }
  catch (error) { recordBackendFailure("burn_active_hub_identity", error); return false; }
}

export async function executeHubFullCleanup(): Promise<HubFullCleanupResult | null> {
  if (!isTauriRuntime()) return null;
  try {
    return checkedBackendResponse("execute_hub_full_cleanup",
      parseFullCleanup(await invoke<unknown>("execute_hub_full_cleanup")),
      "the cleanup result did not match the expected shape");
  }
  catch (error) { recordBackendFailure("execute_hub_full_cleanup", error); return null; }
}

export async function scanLocalPrivacy(messages: LocalMessageCandidate[]): Promise<LocalPrivacyScanResult | null> {
  if (!isTauriRuntime() || messages.length > 2_000 || !messages.every(validLocalCandidate)) return null;
  try {
    return checkedBackendResponse("scan_local_privacy",
      parseLocalPrivacyScan(await invoke<unknown>("scan_local_privacy", { messages })),
      "the scan result did not match the expected shape");
  }
  catch (error) { recordBackendFailure("scan_local_privacy", error); return null; }
}

export function bindReviewedItemIdentities(
  reviewedItems: readonly ReviewedItemIdentity[],
  selectedReviewIds: readonly string[],
): ReviewedItemIdentityBinding[] | null {
  if (!Array.isArray(reviewedItems)
    || !Array.isArray(selectedReviewIds)
    || reviewedItems.length > 2_000
    || selectedReviewIds.length > reviewedItems.length) return null;
  const byReviewId = new Map<string, ReviewedItemIdentity>();
  for (const item of reviewedItems) {
    if (!validReviewedItemIdentity(item) || byReviewId.has(item.reviewId)) return null;
    byReviewId.set(item.reviewId, item);
  }
  const seen = new Set<string>();
  const bound: ReviewedItemIdentityBinding[] = [];
  for (const reviewId of selectedReviewIds) {
    if (!safePlaintext(reviewId, 128) || seen.has(reviewId)) return null;
    const item = byReviewId.get(reviewId);
    if (!item) return null;
    bound.push({ ...item, selected: true });
    seen.add(reviewId);
  }
  return bound;
}

/**
 * Burn the active context and keep the part of the result the operator has to
 * be told about.
 *
 * This used to throw the whole `HubScopeBurnResult` away and return `true`,
 * which is how a burn whose peer revocations were only QUEUED still rendered as
 * "Finished": the caller had no `storageKey` to read the acknowledgement state
 * back with, and no `revocationQueueComplete` to notice a peer had been dropped.
 * `null` still reads falsy, so every existing failure branch is unchanged.
 */
export async function burnActiveHubContext(contextToken: string): Promise<HubScopeBurnOutcome | null> {
  if (!isTauriRuntime() || !safe(contextToken, 180)) return null;
  try {
    return parseHubScopeBurnOutcome(await invoke<unknown>("burn_active_hub_context", { contextToken }));
  }
  catch (error) { recordBackendFailure("burn_active_hub_context", error); return null; }
}

/**
 * Lenient on purpose. A strict `exact()` here would turn a burn that really
 * happened into "the chat burn failed closed", which is a different lie. Only
 * the three fields the acknowledgement read-back needs are validated.
 */
export function parseHubScopeBurnOutcome(raw: unknown): HubScopeBurnOutcome | null {
  if (!isRecord(raw)
    || !safePlaintext(raw.storageKey, 512)
    || !boundedCount(raw.revocationsQueued)
    || typeof raw.revocationQueueComplete !== "boolean") return null;
  return {
    storageKey: String(raw.storageKey),
    revocationsQueued: Number(raw.revocationsQueued),
    revocationQueueComplete: raw.revocationQueueComplete,
  };
}

/**
 * Whether the burn notices already queued for one conversation have been
 * acknowledged. `null` means OSL could not tell, which the burn dialog reports
 * as not acknowledged rather than as done.
 */
export async function getHubRevocationStatus(storageKey: string): Promise<HubRevocationStatus | null> {
  if (!isTauriRuntime() || !safePlaintext(storageKey, 512)) return null;
  try {
    return checkedBackendResponse("get_hub_revocation_status",
      parseHubRevocationStatus(await invoke<unknown>("get_hub_revocation_status", { storageKey })),
      "the revocation status did not match the expected shape");
  }
  catch (error) { recordBackendFailure("get_hub_revocation_status", error); return null; }
}

export function parseHubRevocationStatus(raw: unknown): HubRevocationStatus | null {
  if (!isRecord(raw) || !exact(raw, ["storageKey", "status", "peersPending", "peersAcknowledged", "claims"])) return null;
  if (!safePlaintext(raw.storageKey, 512)
    || !boundedCount(raw.peersPending)
    || !boundedCount(raw.peersAcknowledged)) return null;
  if (![REVOCATION_STATUS_SENT_REQUEST, REVOCATION_STATUS_ACKNOWLEDGED, REVOCATION_STATUS_NOT_ACKNOWLEDGED].includes(String(raw.status))) return null;
  if (!Array.isArray(raw.claims) || raw.claims.length > 8 || !raw.claims.every((claim) => safePlaintext(claim, 512))) return null;
  return raw as unknown as HubRevocationStatus;
}

export async function getHubServiceBurnReadiness(serviceId: string, accountId: string): Promise<HubServiceBurnReadiness | null> {
  if (!isTauriRuntime() || !safeId(serviceId, 32) || !safePlaintext(accountId, 128)) return null;
  try {
    return checkedBackendResponse("get_hub_service_burn_readiness",
      parseHubServiceBurnReadiness(await invoke<unknown>("get_hub_service_burn_readiness", { serviceId, accountId })),
      "the burn readiness did not match the expected shape");
  }
  catch (error) { recordBackendFailure("get_hub_service_burn_readiness", error); return null; }
}

export async function burnHubServiceAccount(serviceId: string, accountId: string, confirmedBurnId: string): Promise<HubServiceBurnResult | null> {
  if (!isTauriRuntime() || !safeId(serviceId, 32) || !safePlaintext(accountId, 128) || !/^[a-f0-9]{64}$/.test(confirmedBurnId)) return null;
  try {
    return checkedBackendResponse("burn_hub_service_account",
      parseHubServiceBurnResult(await invoke<unknown>("burn_hub_service_account", { serviceId, accountId, confirmedBurnId })),
      "the burn result did not match the expected shape");
  }
  catch (error) { recordBackendFailure("burn_hub_service_account", error); return null; }
}

export function parseHubServiceBurnReadiness(raw: unknown): HubServiceBurnReadiness | null {
  if (!isRecord(raw) || !exact(raw, ["burnId", "manifestDigest", "indexedScopes", "coverageComplete", "loginProfileUntouched", "nativeHistoryUntouched"])) return null;
  if (!/^[a-f0-9]{64}$/.test(String(raw.burnId)) || !/^[a-f0-9]{64}$/.test(String(raw.manifestDigest)) || !boundedCount(raw.indexedScopes) || typeof raw.coverageComplete !== "boolean" || raw.loginProfileUntouched !== true || raw.nativeHistoryUntouched !== true) return null;
  return raw as unknown as HubServiceBurnReadiness;
}

export function parseHubServiceBurnResult(raw: unknown): HubServiceBurnResult | null {
  if (!isRecord(raw) || !exact(raw, ["burnId", "scopesBurned", "rowsDestroyed", "whitelistEntriesRemoved", "remoteBlobsDeleted", "remoteBlobDeletionsFailed", "localCleanupComplete", "remoteCleanupComplete", "loginProfileUntouched", "nativeHistoryUntouched"])) return null;
  if (!/^[a-f0-9]{64}$/.test(String(raw.burnId)) || ![raw.scopesBurned, raw.rowsDestroyed, raw.whitelistEntriesRemoved, raw.remoteBlobsDeleted, raw.remoteBlobDeletionsFailed].every(boundedCount) || typeof raw.localCleanupComplete !== "boolean" || typeof raw.remoteCleanupComplete !== "boolean" || raw.loginProfileUntouched !== true || raw.nativeHistoryUntouched !== true) return null;
  return raw as unknown as HubServiceBurnResult;
}

// ---------------------------------------------------------------------------
// Guided deletion of the operator's own Discord messages.
//
// This is a DISTINCT action from Burn, and the two parsers directly above are
// why it had to be one. `nativeHistoryUntouched !== true` there is not a
// placeholder to be relaxed: `docs/design/burn-contract.md:14` says burn does
// not delete carrier messages from a native service, so a burn result that
// claimed otherwise would be false whatever else was true, and both burn
// parsers therefore keep rejecting it outright and unchanged.
//
// What was actually missing was a SECOND channel: a receipt that may claim
// platform removal, and can only do so with the evidence for it attached.
// `parseGuidedDeletionReceipt` below is that channel, and the claim is gated
// three ways rather than merely allowed:
//
//   1. per row -- `state: "verified"` requires `rewalkProvedAbsent === true`
//      AND `requestPosted === true`, and every other state requires
//      `rewalkProvedAbsent === false`. A row that says it was deleted without a
//      proven re-walk rejects the whole receipt;
//   2. in aggregate -- `platformRemovalVerified` must equal
//      `rowsRequested > 0 && rowsVerified === rowsRequested`, and every count
//      must equal the actual tally of rows in that state;
//   3. across guarantees -- this channel may never claim the OTHER two
//      guarantees from `osl-gui-final-plan.md:496-500`, so
//      `oslContentExpiryApplied`, `localRemovalApplied` and `burnPerformed` must
//      all be exactly `false`.
//
// So an unverified claim is still rejected, from either channel; only a claim
// carrying its own proof is accepted, and only through this one.
// ---------------------------------------------------------------------------

export const GUIDED_DELETION_CONTRACT = "discord_guided_deletion_v1";

/** The six activity states from `osl-gui-final-plan.md:494`. */
export type GuidedDeletionState = "scheduled" | "running" | "verified" | "failed" | "unsupported" | "held";

const GUIDED_DELETION_STATES: GuidedDeletionState[] = ["scheduled", "running", "verified", "failed", "unsupported", "held"];

export interface GuidedDeletionRowShape { heightPx: number; children: number }

export interface GuidedDeletionCandidate {
  scanOrdinal: number;
  shapeOrdinal: number;
  shape: GuidedDeletionRowShape;
  /** A length, never the message. */
  textLen: number;
  authoredByOperator: true;
}

export interface GuidedDeletionScan {
  scopeBindingHash: string;
  generation: number;
  rowsSeen: number;
  rowsUnreadable: number;
  walk: "complete" | "truncated";
  candidates: GuidedDeletionCandidate[];
}

export interface GuidedDeletionPreview {
  scopeBindingHash: string;
  generation: number;
  planDigest: string;
  rows: GuidedDeletionCandidate[];
  platformSteps: string[];
  irreversible: true;
  guarantee: "platform_removal";
  expiresOslContent: false;
  removesLocalCopies: false;
}

export interface GuidedDeletionRunAuthority {
  runId: string;
  attendedActionId: string;
  scopeBindingHash: string;
  generation: number;
  planDigest: string;
  mode: "attended_delete_run_v1";
}

export interface GuidedDeletionRowOutcome {
  scanOrdinal: number;
  textLen: number;
  state: GuidedDeletionState;
  /** Fixed native label. Never a message, a draft or a conversation name. */
  stage: string;
  requestPosted: boolean;
  rewalkProvedAbsent: boolean;
}

export interface GuidedDeletionReceipt {
  contract: string;
  planDigest: string;
  scopeBindingHash: string;
  generation: number;
  rowsRequested: number;
  rowsVerified: number;
  rowsFailed: number;
  rowsUnsupported: number;
  rowsHeld: number;
  /** Guarantee 1, and the only one this action can establish. */
  platformRemovalVerified: boolean;
  /** Guarantee 2. Never this action's to claim. */
  oslContentExpiryApplied: false;
  /** Guarantee 3. Never this action's to claim. */
  localRemovalApplied: false;
  /** Never a burn. `burn-contract.md:14` stays true. */
  burnPerformed: false;
  rows: GuidedDeletionRowOutcome[];
}

/**
 * Scan the claimed native Discord window for the operator's own messages.
 * Read-only and non-destructive; the Pro gate is on the plan, not the scan.
 */
export async function scanDiscordOwnMessagesForDeletion(): Promise<GuidedDeletionScan | null> {
  if (!isTauriRuntime()) return null;
  try {
    return checkedBackendResponse("scan_discord_own_messages_for_deletion",
      parseGuidedDeletionScan(await invoke<unknown>("scan_discord_own_messages_for_deletion")),
      "the deletion scan did not match the expected shape");
  } catch (error) { recordBackendFailure("scan_discord_own_messages_for_deletion", error); return null; }
}

/**
 * Preview exactly which of the operator's own rows would be deleted. Pro-only
 * in native code (`osl-gui-final-plan.md:506-508`); this never asserts the
 * entitlement itself.
 */
export async function previewDiscordGuidedDeletion(scanOrdinals: number[]): Promise<GuidedDeletionPreview | null> {
  if (!isTauriRuntime() || !isBoundedOrdinalSelection(scanOrdinals)) return null;
  try {
    const preview = parseGuidedDeletionPreview(await invoke<unknown>("preview_discord_guided_deletion", { scanOrdinals }));
    return checkedBackendResponse("preview_discord_guided_deletion",
      preview && preview.rows.length === scanOrdinals.length
        && preview.rows.every((row) => scanOrdinals.includes(row.scanOrdinal))
        ? preview
        : null,
      "the preview did not describe exactly the requested rows");
  } catch (error) { recordBackendFailure("preview_discord_guided_deletion", error); return null; }
}

/**
 * Execute a previewed plan. `planDigest` is the digest the operator confirmed;
 * native code re-derives it and refuses anything else, so a preview the
 * operator never saw cannot be executed.
 */
export async function executeDiscordGuidedDeletion(planDigest: string, authority?: GuidedDeletionRunAuthority): Promise<GuidedDeletionReceipt | null> {
  if (!isTauriRuntime() || !/^[a-f0-9]{64}$/.test(planDigest) || !validGuidedDeletionRunAuthority(authority, planDigest)) return null;
  try {
    const receipt = parseGuidedDeletionReceipt(await invoke<unknown>("execute_discord_guided_deletion", { planDigest, authority }));
    return checkedBackendResponse("execute_discord_guided_deletion",
      receipt
        && receipt.planDigest === planDigest
        && receipt.scopeBindingHash === authority.scopeBindingHash
        && receipt.generation === authority.generation
        ? receipt
        : null,
      "the deletion receipt did not match the confirmed plan");
  } catch (error) { recordBackendFailure("execute_discord_guided_deletion", error); return null; }
}

export function parseGuidedDeletionScan(raw: unknown): GuidedDeletionScan | null {
  if (!isRecord(raw) || !exact(raw, ["scopeBindingHash", "generation", "rowsSeen", "rowsUnreadable", "walk", "candidates"])) return null;
  if (!/^[a-f0-9]{64}$/.test(String(raw.scopeBindingHash))
    || !boundedCount(raw.generation)
    || !boundedCount(raw.rowsSeen)
    || !boundedCount(raw.rowsUnreadable)
    || (raw.walk !== "complete" && raw.walk !== "truncated")
    || !Array.isArray(raw.candidates)
    || raw.candidates.length > GUIDED_DELETION_MAX_ROWS) return null;
  const candidates = raw.candidates.map(parseGuidedDeletionCandidate);
  if (!candidates.every((row): row is GuidedDeletionCandidate => row !== null)) return null;
  // Ordinals are positions in one bounded read, so they cannot repeat and cannot
  // exceed the rows that read saw.
  const ordinals = candidates.map((row) => row.scanOrdinal);
  if (new Set(ordinals).size !== ordinals.length || ordinals.some((ordinal) => ordinal >= Number(raw.rowsSeen))) return null;
  return { ...raw, candidates } as unknown as GuidedDeletionScan;
}

export function parseGuidedDeletionPreview(raw: unknown): GuidedDeletionPreview | null {
  if (!isRecord(raw) || !exact(raw, ["scopeBindingHash", "generation", "planDigest", "rows", "platformSteps", "irreversible", "guarantee", "expiresOslContent", "removesLocalCopies"])) return null;
  if (!/^[a-f0-9]{64}$/.test(String(raw.scopeBindingHash))
    || !/^[a-f0-9]{64}$/.test(String(raw.planDigest))
    || !boundedCount(raw.generation)
    || !Array.isArray(raw.rows)
    || raw.rows.length === 0
    || raw.rows.length > GUIDED_DELETION_MAX_ROWS
    || !Array.isArray(raw.platformSteps)
    || raw.platformSteps.length === 0
    || raw.platformSteps.length > 16
    || !raw.platformSteps.every((step) => safeId(step, 80))
    // An irreversible action must say so, must claim only platform removal, and
    // must never claim either of the other two guarantees.
    || raw.irreversible !== true
    || raw.guarantee !== "platform_removal"
    || raw.expiresOslContent !== false
    || raw.removesLocalCopies !== false) return null;
  const rows = raw.rows.map(parseGuidedDeletionCandidate);
  if (!rows.every((row): row is GuidedDeletionCandidate => row !== null)) return null;
  const ordinals = rows.map((row) => row.scanOrdinal);
  if (new Set(ordinals).size !== ordinals.length) return null;
  return { ...raw, rows } as unknown as GuidedDeletionPreview;
}

/**
 * The one parser that may accept a claim of platform removal.
 *
 * Fails closed on every axis: an exact key set, bounded counts, per-row evidence
 * for `verified`, aggregate agreement between the counts and the rows, and a
 * hard refusal of any claim about the other two guarantees or about burn.
 */
export function parseGuidedDeletionReceipt(raw: unknown): GuidedDeletionReceipt | null {
  if (!isRecord(raw) || !exact(raw, ["contract", "planDigest", "scopeBindingHash", "generation", "rowsRequested", "rowsVerified", "rowsFailed", "rowsUnsupported", "rowsHeld", "platformRemovalVerified", "oslContentExpiryApplied", "localRemovalApplied", "burnPerformed", "rows"])) return null;
  if (raw.contract !== GUIDED_DELETION_CONTRACT
    || !/^[a-f0-9]{64}$/.test(String(raw.planDigest))
    || !/^[a-f0-9]{64}$/.test(String(raw.scopeBindingHash))
    || !boundedCount(raw.generation)
    || ![raw.rowsRequested, raw.rowsVerified, raw.rowsFailed, raw.rowsUnsupported, raw.rowsHeld].every(boundedCount)
    || typeof raw.platformRemovalVerified !== "boolean") return null;
  // Guarantee separation. This channel exists to report platform removal, and it
  // may report nothing else: an OSL content expiry, a local removal or a burn
  // asserted here is rejected exactly as it is on the burn parsers above.
  if (raw.oslContentExpiryApplied !== false || raw.localRemovalApplied !== false || raw.burnPerformed !== false) return null;
  if (!Array.isArray(raw.rows) || raw.rows.length === 0 || raw.rows.length > GUIDED_DELETION_MAX_ROWS) return null;
  const rows = raw.rows.map(parseGuidedDeletionRowOutcome);
  if (!rows.every((row): row is GuidedDeletionRowOutcome => row !== null)) return null;
  if (rows.length !== Number(raw.rowsRequested)) return null;
  const ordinals = rows.map((row) => row.scanOrdinal);
  if (new Set(ordinals).size !== ordinals.length) return null;
  const tally = (state: GuidedDeletionState) => rows.filter((row) => row.state === state).length;
  if (tally("verified") !== Number(raw.rowsVerified)
    || tally("failed") !== Number(raw.rowsFailed)
    || tally("unsupported") !== Number(raw.rowsUnsupported)
    || tally("held") !== Number(raw.rowsHeld)) return null;
  // The aggregate claim is derived, never asserted: it holds only when every
  // requested row was individually proven gone.
  if (raw.platformRemovalVerified !== (Number(raw.rowsRequested) > 0 && Number(raw.rowsVerified) === Number(raw.rowsRequested))) return null;
  return { ...raw, rows } as unknown as GuidedDeletionReceipt;
}

/**
 * The label the UI may show for one row.
 *
 * `Sent request` is never displayed as `Deleted` (`osl-gui-final-plan.md:494`),
 * so exactly one branch here says the message is gone from Discord and it is
 * reachable only from `verified` -- which `parseGuidedDeletionReceipt` will not
 * accept without a proven re-walk.
 */
export function guidedDeletionRowLabel(row: GuidedDeletionRowOutcome): string {
  switch (row.state) {
    case "verified": return "Deleted from Discord";
    case "running": return row.requestPosted ? "Sent request" : "Running";
    case "scheduled": return "Scheduled";
    case "failed": return row.requestPosted ? "Sent request - not verified" : "Failed";
    case "unsupported": return "Discord offers no delete for this message";
    case "held": return "Held - nothing was deleted";
  }
}

const GUIDED_DELETION_MAX_ROWS = 32;

function parseGuidedDeletionCandidate(raw: unknown): GuidedDeletionCandidate | null {
  if (!isRecord(raw) || !exact(raw, ["scanOrdinal", "shapeOrdinal", "shape", "textLen", "authoredByOperator"])) return null;
  if (!boundedCount(raw.scanOrdinal) || !boundedCount(raw.shapeOrdinal) || !boundedCount(raw.textLen)) return null;
  // Only the operator's own rows are ever deletable, and a candidate that says
  // otherwise is refused rather than filtered out silently.
  if (raw.authoredByOperator !== true) return null;
  if (!isRecord(raw.shape) || !exact(raw.shape, ["heightPx", "children"])) return null;
  if (!Number.isSafeInteger(raw.shape.heightPx) || Number(raw.shape.heightPx) <= 0 || Number(raw.shape.heightPx) > 4_096) return null;
  if (!boundedCount(raw.shape.children)) return null;
  return raw as unknown as GuidedDeletionCandidate;
}

function parseGuidedDeletionRowOutcome(raw: unknown): GuidedDeletionRowOutcome | null {
  if (!isRecord(raw) || !exact(raw, ["scanOrdinal", "textLen", "state", "stage", "requestPosted", "rewalkProvedAbsent"])) return null;
  if (!boundedCount(raw.scanOrdinal)
    || !boundedCount(raw.textLen)
    || !GUIDED_DELETION_STATES.includes(raw.state as GuidedDeletionState)
    || !safeId(raw.stage, 120)
    || typeof raw.requestPosted !== "boolean"
    || typeof raw.rewalkProvedAbsent !== "boolean") return null;
  // THE narrowing. A verified row must carry both pieces of evidence, and no
  // other state may claim the re-walk proof at all.
  if (raw.state === "verified") {
    if (raw.rewalkProvedAbsent !== true || raw.requestPosted !== true) return null;
  } else if (raw.rewalkProvedAbsent !== false) return null;
  return raw as unknown as GuidedDeletionRowOutcome;
}

function isBoundedOrdinalSelection(value: unknown): value is number[] {
  return Array.isArray(value)
    && value.length > 0
    && value.length <= GUIDED_DELETION_MAX_ROWS
    && value.every((ordinal) => Number.isSafeInteger(ordinal) && ordinal >= 0 && ordinal < 4_096)
    && new Set(value).size === value.length;
}

function validGuidedDeletionRunAuthority(value: unknown, planDigest: string): value is GuidedDeletionRunAuthority {
  return isRecord(value)
    && exact(value, ["runId", "attendedActionId", "scopeBindingHash", "generation", "planDigest", "mode"])
    && safeOpaque(value.runId, 80)
    && safeOpaque(value.attendedActionId, 80)
    && /^[a-f0-9]{64}$/.test(String(value.scopeBindingHash))
    && boundedCount(value.generation)
    && Number(value.generation) > 0
    && value.planDigest === planDigest
    && value.mode === "attended_delete_run_v1";
}

export async function loadActiveContextSecurity(contextToken: string): Promise<ScopeSecurity | null> {
  if (!isTauriRuntime() || !safe(contextToken, 180)) return null;
  try {
    return checkedBackendResponse("get_active_hub_context_security",
      parseScopeSecurity(await invoke<unknown>("get_active_hub_context_security", { contextToken })),
      "the scope security did not match the expected shape");
  }
  catch (error) { recordBackendFailure("get_active_hub_context_security", error); return null; }
}

export async function saveActiveContextSecurity(contextToken: string, ttlSeconds: number, decryptDisplayEnabled: boolean): Promise<ScopeSecurity | null> {
  if (!isTauriRuntime() || !safe(contextToken, 180) || !Number.isSafeInteger(ttlSeconds) || ttlSeconds < 0 || ttlSeconds > 31_536_000) return null;
  try {
    return checkedBackendResponse("set_active_hub_context_security",
      parseScopeSecurity(await invoke<unknown>("set_active_hub_context_security", { contextToken, ttlSeconds, decryptDisplayEnabled })),
      "the scope security did not match the expected shape");
  }
  catch (error) { recordBackendFailure("set_active_hub_context_security", error); return null; }
}

function parseScopeSecurity(raw: unknown): ScopeSecurity | null {
  if (!isRecord(raw) || !exact(raw, ["storageKey", "ttlSeconds", "decryptDisplayEnabled"])) return null;
  if (!safe(raw.storageKey, 512) || !Number.isSafeInteger(raw.ttlSeconds) || Number(raw.ttlSeconds) < 0 || typeof raw.decryptDisplayEnabled !== "boolean") return null;
  return raw as unknown as ScopeSecurity;
}

export function parseFullCleanup(raw: unknown): HubFullCleanupResult | null {
  if (!isRecord(raw) || !exact(raw, ["localCleanupComplete", "removedTargets", "failedTargets", "remoteUnregister", "restartRequired", "originalDiscordDataUntouched"])) return null;
  if (typeof raw.localCleanupComplete !== "boolean" || typeof raw.restartRequired !== "boolean" || raw.originalDiscordDataUntouched !== true) return null;
  if (!Array.isArray(raw.removedTargets) || !Array.isArray(raw.failedTargets) || raw.removedTargets.length > 32 || raw.failedTargets.length > 32) return null;
  if (![...raw.removedTargets, ...raw.failedTargets].every((item) => safe(item, 80))) return null;
  if (!isRecord(raw.remoteUnregister) || !exact(raw.remoteUnregister, ["identitiesFound", "succeeded", "failed", "unavailable"])) return null;
  if (![raw.remoteUnregister.identitiesFound, raw.remoteUnregister.succeeded, raw.remoteUnregister.failed, raw.remoteUnregister.unavailable].every((item) => Number.isSafeInteger(item) && Number(item) >= 0)) return null;
  return raw as unknown as HubFullCleanupResult;
}

export function parseLocalPrivacyScan(raw: unknown): LocalPrivacyScanResult | null {
  return parsePrivacyScan(raw, false);
}

export function parsePersistedLocalPrivacyScan(raw: unknown): PersistedLocalPrivacyScanResult | null {
  return parsePrivacyScan(raw, true);
}

function parsePrivacyScan(raw: unknown, persisted: false): LocalPrivacyScanResult | null;
function parsePrivacyScan(raw: unknown, persisted: true): PersistedLocalPrivacyScanResult | null;
function parsePrivacyScan(raw: unknown, persisted: boolean): LocalPrivacyScanResult | PersistedLocalPrivacyScanResult | null {
  if (!isRecord(raw)) return null;
  const baseKeys = ["findings", "messagesScanned", "messagesRejected", "truncated", "analysisLocation", "persisted"];
  const attachmentKeys = ["attachmentsScanned", "imagesChecked", "videosChecked", "attachmentTypesScanned", "uninspectedAttachments"];
  const hasAttachmentKeys = attachmentKeys.some((key) => Object.prototype.hasOwnProperty.call(raw, key));
  if (!exact(raw, hasAttachmentKeys ? [...baseKeys, ...attachmentKeys] : baseKeys)) return null;
  const maxMessages = persisted ? 10_000_000 : 2_000;
  if (!Array.isArray(raw.findings) || raw.findings.length > 1_000 || !Number.isSafeInteger(raw.messagesScanned) || Number(raw.messagesScanned) < 0 || Number(raw.messagesScanned) > maxMessages || !Number.isSafeInteger(raw.messagesRejected) || Number(raw.messagesRejected) < 0 || typeof raw.truncated !== "boolean" || raw.analysisLocation !== "this_device_only" || raw.persisted !== persisted) return null;
  const attachmentsScanned = hasAttachmentKeys ? raw.attachmentsScanned : 0;
  const imagesChecked = hasAttachmentKeys ? raw.imagesChecked : false;
  const videosChecked = hasAttachmentKeys ? raw.videosChecked : false;
  const attachmentTypesScanned = hasAttachmentKeys ? raw.attachmentTypesScanned : [];
  const uninspectedAttachments = hasAttachmentKeys ? raw.uninspectedAttachments : [];
  if (!boundedCount(attachmentsScanned)
    || typeof imagesChecked !== "boolean"
    || typeof videosChecked !== "boolean"
    || !Array.isArray(attachmentTypesScanned)
    || attachmentTypesScanned.length > 64
    || !attachmentTypesScanned.every((value) => safePlaintext(value, 80))
    || new Set(attachmentTypesScanned).size !== attachmentTypesScanned.length
    || !Array.isArray(uninspectedAttachments)
    || uninspectedAttachments.length > 1_000
    || !uninspectedAttachments.every(validUninspectedAttachment)) return null;
  const findings = raw.findings.map(parsePrivacyFinding);
  if (!findings.every((finding): finding is LocalPrivacyFinding => finding !== null)) return null;
  return { ...raw, findings, attachmentsScanned, imagesChecked, videosChecked, attachmentTypesScanned, uninspectedAttachments } as LocalPrivacyScanResult | PersistedLocalPrivacyScanResult;
}

function parsePrivacyFinding(raw: unknown): LocalPrivacyFinding | null {
  if (!isRecord(raw)) return null;
  const hasAttachmentPath = Object.prototype.hasOwnProperty.call(raw, "attachmentPath");
  if (!exact(raw, hasAttachmentPath ? ["serviceId", "accountId", "conversationId", "messageLocator", "authoredBySelf", "createdAtUnixMs", "category", "confidence", "reason", "localPreview", "canRequestDelete", "attachmentPath"] : ["serviceId", "accountId", "conversationId", "messageLocator", "authoredBySelf", "createdAtUnixMs", "category", "confidence", "reason", "localPreview", "canRequestDelete"])) return null;
  const attachmentPath = hasAttachmentPath ? raw.attachmentPath : null;
  if (!safeId(raw.serviceId, 32) || !safePlaintext(raw.accountId, 128) || !safePlaintext(raw.conversationId, 256) || !safePlaintext(raw.messageLocator, 256) || typeof raw.authoredBySelf !== "boolean" || !(raw.createdAtUnixMs === null || Number.isSafeInteger(raw.createdAtUnixMs)) || !["credential", "recovery_material", "payment_card", "government_identity", "precise_location", "profanity", "sexual_content", "sensitive_health", "controlled_substances", "potentially_unlawful_conduct", "work_sensitive_information"].includes(String(raw.category)) || !Number.isSafeInteger(raw.confidence) || Number(raw.confidence) < 0 || Number(raw.confidence) > 100 || !safePlaintext(raw.reason, 240) || !safePlaintext(raw.localPreview, 256) || typeof raw.canRequestDelete !== "boolean" || !(attachmentPath === null || safePlaintext(attachmentPath, 1_024))) return null;
  if (raw.canRequestDelete && !raw.authoredBySelf) return null;
  return { ...raw, attachmentPath } as unknown as LocalPrivacyFinding;
}

function validLocalCandidate(candidate: LocalMessageCandidate): boolean {
  return safeId(candidate.serviceId, 32)
    && safePlaintext(candidate.accountId, 128)
    && safePlaintext(candidate.conversationId, 256)
    && safePlaintext(candidate.messageLocator, 256)
    && typeof candidate.authoredBySelf === "boolean"
    && (candidate.createdAtUnixMs === null || Number.isSafeInteger(candidate.createdAtUnixMs))
    && ((safePlaintext(candidate.text, 8 * 1024)) || (candidate.text === "" && Boolean(candidate.attachments?.length)))
    && (candidate.attachments === undefined || (Array.isArray(candidate.attachments)
      && candidate.attachments.length > 0
      && candidate.attachments.length <= 64
      && candidate.attachments.every(validLocalAttachment)));
}

function validLocalAttachment(attachment: LocalAttachmentCandidate): boolean {
  return isRecord(attachment)
    && exact(attachment, ["attachmentId", "displayName", "contentBase64"])
    && safePlaintext(attachment.attachmentId, 256)
    && safePlaintext(attachment.displayName, 1_024)
    && typeof attachment.contentBase64 === "string"
    && attachment.contentBase64.length <= 12 * 1024 * 1024
    && attachment.contentBase64.length % 4 === 0
    && /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/u.test(attachment.contentBase64);
}

function validUninspectedAttachment(value: unknown): boolean {
  if (!isRecord(value) || !exact(value, ["attachmentId", "path", "detectedType", "reason", "detail"])) return false;
  return safePlaintext(value.attachmentId, 256)
    && safePlaintext(value.path, 1_024)
    && safePlaintext(value.detectedType, 80)
    && ["unsupported", "encrypted", "malformed", "limit_exceeded", "unsafe_archive_entry", "model_not_installed", "dependency_not_installed", "image_only_pdf_needs_ocr"].includes(String(value.reason))
    && safePlaintext(value.detail, 240);
}

function validReviewedItemIdentity(item: ReviewedItemIdentity): boolean {
  return isRecord(item)
    && exact(item, ["reviewId", "serviceId", "accountId", "conversationId", "messageLocator"])
    && safePlaintext(item.reviewId, 128)
    && safeId(item.serviceId, 32)
    && isContextId(item.accountId)
    && isContextId(item.conversationId)
    && safePlaintext(item.messageLocator, 512);
}

function parseIdentityCreation(raw: unknown): HubIdentityCreation | null {
  if (!isRecord(raw) || !exact(raw, ["identity", "identityRecoveryPhrase", "storageMethod"])) return null;
  const identity = parseIdentitySlot(raw.identity);
  if (!identity || !(raw.identityRecoveryPhrase === null || safePlaintext(raw.identityRecoveryPhrase, 512)) || !safe(raw.storageMethod, 80)) return null;
  return { identity, identityRecoveryPhrase: raw.identityRecoveryPhrase as string | null, storageMethod: raw.storageMethod };
}

function parseIdentitySlot(raw: unknown): HubIdentitySlot | null {
  if (!isRecord(raw) || !exact(raw, ["slotId", "label", "oslUserId", "active"])) return null;
  if (!/^[A-Za-z0-9_-]{8,80}$/.test(String(raw.slotId)) || !safe(raw.label, 80) || !safe(raw.oslUserId, 180) || typeof raw.active !== "boolean") return null;
  return raw as unknown as HubIdentitySlot;
}

export function parseFriendProfile(raw: unknown): FriendProfile | null {
  if (!isRecord(raw) || !exact(raw, ["friendCode", "oslUserId", "safetyNumber"])) return null;
  if (typeof raw.friendCode !== "string" || !/^OSLFR1\.[A-Za-z0-9_-]{16,8192}$/.test(raw.friendCode)) return null;
  if (!safe(raw.oslUserId, 180) || !safe(raw.safetyNumber, 180)) return null;
  return { friendCode: raw.friendCode, oslUserId: raw.oslUserId, safetyNumber: raw.safetyNumber };
}

export function parseNotifications(raw: unknown): AppNotification[] | null {
  if (!Array.isArray(raw) || raw.length > 20) return null;
  const parsed: AppNotification[] = [];
  for (const item of raw) {
    if (!isRecord(item) || !exact(item, ["id", "title", "detail", "createdAt"]) || !safe(item.id, 64) || !safe(item.title, 100) || !safe(item.detail, 240) || !safe(item.createdAt, 40)) return null;
    parsed.push(item as unknown as AppNotification);
  }
  return parsed;
}

export function parseLocalProtectedText(raw: unknown): LocalProtectedText | null {
  if (!isRecord(raw) || !exact(raw, ["capsule", "localMessageId", "protection", "personToPersonE2ee", "statePersisted", "viewOnce"])) return null;
  if (!boundedUtf8Text(raw.capsule, HUB_CAPSULE_MAX_BYTES)
    || !isContextId(raw.localMessageId)
    || raw.protection !== "local_protected_loopback"
    || raw.personToPersonE2ee !== false
    || raw.statePersisted !== true
    || typeof raw.viewOnce !== "boolean") return null;
  return raw as unknown as LocalProtectedText;
}

export function parseLocalLoopbackContext(raw: unknown): LocalLoopbackContext | null {
  if (!isRecord(raw) || !exact(raw, ["contextToken", "serviceId", "accountId", "conversationId"])) return null;
  if (!safeContextToken(raw.contextToken)
    || !safeId(raw.serviceId, 32)
    || !isContextId(raw.accountId)
    || !isContextId(raw.conversationId)) return null;
  return raw as unknown as LocalLoopbackContext;
}

export function parseManualPeerContext(raw: unknown): ManualPeerContext | null {
  if (!isRecord(raw) || !exact(raw, ["contextToken", "serviceId", "accountId", "personId", "peerOslUserId", "scopeApproved"])) return null;
  if (!safeContextToken(raw.contextToken)
    || !safeId(raw.serviceId, 32)
    || !isContextId(raw.accountId)
    || !safe(raw.personId, 180)
    || !isContextId(raw.peerOslUserId)
    || typeof raw.scopeApproved !== "boolean") return null;
  return raw as unknown as ManualPeerContext;
}

export function parsePreparedPeerProseText(raw: unknown): PreparedPeerProseText | null {
  if (!isRecord(raw) || !exact(raw, ["coverText", "expiresAt", "personToPersonE2ee", "viewOnce"])) return null;
  if (!boundedUtf8Text(raw.coverText, HUB_CAPSULE_MAX_BYTES)
    || !Number.isSafeInteger(raw.expiresAt)
    || Number(raw.expiresAt) <= 0
    || raw.personToPersonE2ee !== true
    || typeof raw.viewOnce !== "boolean") return null;
  return raw as unknown as PreparedPeerProseText;
}

export function parsePreparedOslChatText(raw: unknown): PreparedOslChatText | null {
  if (!isRecord(raw) || !exact(raw, ["messageId", "expiresAt", "personToPersonE2ee", "viewOnce", "deliveredToOslInbox"])) return null;
  if (!safe(raw.messageId, 96)
    || !Number.isSafeInteger(raw.expiresAt)
    || Number(raw.expiresAt) <= 0
    || raw.personToPersonE2ee !== true
    || typeof raw.viewOnce !== "boolean"
    || raw.deliveredToOslInbox !== true) return null;
  return raw as unknown as PreparedOslChatText;
}

export function parseOpenedPeerProseText(raw: unknown): OpenedPeerProseText | null {
  if (!isRecord(raw) || !exact(raw, ["plaintext", "contextVerified", "personToPersonE2ee", "viewOnceConsumed", "requireCaptureProtection"])) return null;
  if (!isHubPlaintext(raw.plaintext)
    || raw.contextVerified !== true
    || raw.personToPersonE2ee !== true
    || typeof raw.viewOnceConsumed !== "boolean"
    || typeof raw.requireCaptureProtection !== "boolean") return null;
  return raw as unknown as OpenedPeerProseText;
}

export function parseDecryptedLocalProtectedText(raw: unknown): DecryptedLocalProtectedText | null {
  if (!isRecord(raw) || !exact(raw, ["plaintext", "localMessageId", "protection", "personToPersonE2ee", "contextVerified", "viewOnceConsumed"])) return null;
  if (!isLocalProtectedPlaintext(raw.plaintext)
    || !isContextId(raw.localMessageId)
    || raw.protection !== "local_protected_loopback"
    || raw.personToPersonE2ee !== false
    || raw.contextVerified !== true
    || typeof raw.viewOnceConsumed !== "boolean") return null;
  return raw as unknown as DecryptedLocalProtectedText;
}

export function parsePreparedHubAttachment(raw: unknown): PreparedHubAttachment | null {
  if (!isRecord(raw) || !exact(raw, ["sealedB64", "transportFilename", "transportMimeType", "originalMimeType", "ciphertextPrepared", "automaticServiceUpload"])) return null;
  if (!isBoundedBase64(raw.sealedB64)
    || typeof raw.transportFilename !== "string"
    || !/^osl-[a-f0-9]{32}\.mp4$/u.test(raw.transportFilename)
    || raw.transportMimeType !== "video/mp4"
    || !isMimeType(raw.originalMimeType)
    || raw.ciphertextPrepared !== true
    || raw.automaticServiceUpload !== false) return null;
  return raw as unknown as PreparedHubAttachment;
}

export function parseOpenedHubAttachment(raw: unknown): OpenedHubAttachment | null {
  if (!isRecord(raw) || !exact(raw, ["plaintextB64", "originalFilename", "mimeType", "contextVerified"])) return null;
  if (!isBoundedBase64(raw.plaintextB64)
    || !isAttachmentFilename(raw.originalFilename)
    || !isMimeType(raw.mimeType)
    || raw.contextVerified !== true) return null;
  return raw as unknown as OpenedHubAttachment;
}

export function parsePreparedEncryptedText(raw: unknown): PreparedEncryptedText | null {
  if (!isRecord(raw) || !exact(raw, ["messages", "controlMessages", "sessionId"])) return null;
  if (!Array.isArray(raw.messages)
    || raw.messages.length === 0
    || raw.messages.length > HUB_PREPARED_MESSAGE_MAX_ITEMS
    || !Array.isArray(raw.controlMessages)
    || raw.controlMessages.length > HUB_CONTROL_MESSAGE_MAX_ITEMS) return null;
  const wires = [...raw.messages, ...raw.controlMessages];
  if (!wires.every((wire) => boundedUtf8Text(wire, HUB_CAPSULE_MAX_BYTES))) return null;
  const totalBytes = wires.reduce((total, wire) => total + new TextEncoder().encode(wire).length, 0);
  if (totalBytes > HUB_PREPARED_TOTAL_MAX_BYTES) return null;
  if (!(raw.sessionId === null
    || (Number.isSafeInteger(raw.sessionId) && Number(raw.sessionId) >= 0 && Number(raw.sessionId) <= 0xffff_ffff))) return null;
  return {
    messages: raw.messages as string[],
    controlMessages: raw.controlMessages as string[],
    sessionId: raw.sessionId as number | null,
  };
}

export function parseDecryptedHubPlaintext(raw: unknown): string | null {
  return isHubPlaintext(raw) ? raw : null;
}

function isRecord(value: unknown): value is Record<string, unknown> { return typeof value === "object" && value !== null && !Array.isArray(value); }
function exact(value: Record<string, unknown>, keys: string[]): boolean { const actual = Object.keys(value); return actual.length === keys.length && actual.every((key) => keys.includes(key)); }
function safe(value: unknown, max: number): value is string { return typeof value === "string" && value.length > 0 && value.length <= max && !/[<>\u0000-\u001f\u007f]/.test(value); }
function safePlaintext(value: unknown, max: number): value is string { return typeof value === "string" && value.length > 0 && value.length <= max && !/[\u0000\u007f]/.test(value); }
function normalizeSupportMatrixStatus(value: unknown): SupportMatrixRowStatus | null {
  if (typeof value !== "string") return null;
  const normalized = value.replace(/-/gu, "_");
  return [
    "supported",
    "verified_live",
    "runtime_proven",
    "test_proven_only",
    "implemented_unwired",
    "designed_only",
    "externally_blocked",
    "unsupported",
  ].includes(normalized) ? normalized as SupportMatrixRowStatus : null;
}
function safeId(value: unknown, max: number): value is string { return typeof value === "string" && value.length > 0 && value.length <= max && /^[a-z0-9_-]+$/.test(value); }
function safeOpaque(value: unknown, max: number): value is string { return typeof value === "string" && value.length > 0 && value.length <= max && /^[A-Za-z0-9_-]+$/.test(value); }
function boundedCount(value: unknown): boolean { return Number.isSafeInteger(value) && Number(value) >= 0 && Number(value) <= 10_000_000; }
function boundedUtf8Text(value: unknown, maxBytes: number): value is string {
  return typeof value === "string"
    && value.length > 0
    && new TextEncoder().encode(value).length <= maxBytes
    && !/[\u0000\u007f]/.test(value);
}

function safeContextToken(value: unknown): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= 180 && /^[A-Za-z0-9._:-]+$/u.test(value);
}

function isContextId(value: unknown): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= 160 && /^[A-Za-z0-9._:-]+$/u.test(value);
}

function isAttachmentFilename(value: unknown): value is string {
  return typeof value === "string"
    && value.length > 0
    && new TextEncoder().encode(value).length <= HUB_ATTACHMENT_FILENAME_MAX_BYTES
    && !/[\u0000-\u001f\u007f]/u.test(value);
}

function isMimeType(value: unknown): value is string {
  return typeof value === "string"
    && value.length > 0
    && value.length <= 127
    && /^[a-z0-9][a-z0-9!#$&^_.+-]*\/[a-z0-9][a-z0-9!#$&^_.+-]*$/u.test(value);
}

function isBoundedBase64(value: unknown): value is string {
  if (typeof value !== "string" || value.length === 0 || value.length > HUB_ATTACHMENT_B64_MAX_CHARACTERS || value.length % 4 !== 0) return false;
  return /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/u.test(value);
}
