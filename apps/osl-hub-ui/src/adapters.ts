import { invoke } from "./dev-preview";
import { isTauriRuntime } from "./preferences";

export interface FriendProfile { friendCode: string; oslUserId: string; safetyNumber: string; }
export interface AppNotification { id: string; title: string; detail: string; createdAt: string; }
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
}
export interface OpenedPeerProseText {
  plaintext: string;
  contextVerified: true;
  personToPersonE2ee: true;
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
export interface HubIdentitySlot { slotId: string; label: string; oslUserId: string; safetyNumber: string; active: boolean; }
export interface HubIdentityCreation { identity: HubIdentitySlot; identityRecoveryPhrase: string | null; storageMethod: string; }
export interface ScopeSecurity { storageKey: string; ttlSeconds: number; decryptDisplayEnabled: boolean; }
export interface HubPersonWhitelistScope { kind: "dm" | "group" | "channel" | "space"; contextId: string | null; }
export interface HubPerson { personId: string; oslUserId: string; alias: string | null; safetyNumber: string; safetyNumberVerified: boolean; whitelistCount: number; whitelistedScopes: HubPersonWhitelistScope[]; whitelistedScopesTruncated: boolean; pendingKeyChange: boolean; }
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
}
export interface LocalPrivacyFinding extends Omit<LocalMessageCandidate, "text"> {
  category: PrivacyRiskCategory;
  confidence: number;
  reason: string;
  localPreview: string;
  canRequestDelete: boolean;
}
export interface LocalPrivacyScanResult {
  findings: LocalPrivacyFinding[];
  messagesScanned: number;
  messagesRejected: number;
  truncated: boolean;
  analysisLocation: "this_device_only";
  persisted: false;
}
export interface PersistedLocalPrivacyScanResult extends Omit<LocalPrivacyScanResult, "persisted"> {
  persisted: true;
}

export const LOCAL_PROTECTED_TEXT_MAX_BYTES = 1_000;
export const HUB_PLAINTEXT_MAX_BYTES = 1_000;
export const HUB_CAPSULE_MAX_BYTES = 256 * 1024;
export const HUB_ATTACHMENT_B64_MAX_CHARACTERS = 32 * 1024 * 1024;
export const HUB_ATTACHMENT_FILENAME_MAX_BYTES = 1_024;
const HUB_PREPARED_MESSAGE_MAX_ITEMS = 64;
const HUB_CONTROL_MESSAGE_MAX_ITEMS = 512;
const HUB_PREPARED_TOTAL_MAX_BYTES = 4 * 1024 * 1024;

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
    return parsed?.serviceId === serviceId
      && parsed.accountId === accountId
      && parsed.conversationId === conversationId
      ? parsed
      : null;
  } catch { return null; }
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
    return parsed?.serviceId === serviceId
      && parsed.accountId === accountId
      && parsed.personId === personId
      ? parsed
      : null;
  } catch { return null; }
}

export async function preparePeerProseText(
  contextToken: string,
  plaintext: string,
): Promise<PreparedPeerProseText | null> {
  if (!isTauriRuntime() || !safeContextToken(contextToken) || !isHubPlaintext(plaintext)) return null;
  try {
    return parsePreparedPeerProseText(await invoke<unknown>("prepare_peer_prose_text", {
      contextToken,
      plaintext,
    }));
  } catch { return null; }
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
    return parseOpenedPeerProseText(await invoke<unknown>("open_peer_prose_text", {
      contextToken,
      senderPersonId,
      coverText,
    }));
  } catch { return null; }
}

/** Reserve or release an OSL-owned side-sheet region beside the remote child. */
export async function setLocalProtectedSheetOpen(open: boolean): Promise<boolean> {
  if (!isTauriRuntime() || typeof open !== "boolean") return false;
  try { return await invoke<boolean>("set_local_protected_sheet_open", { open }) === true; }
  catch { return false; }
}

export async function prepareEncryptedText(contextToken: string, plaintext: string): Promise<PreparedEncryptedText | null> {
  if (!isTauriRuntime() || !safe(contextToken, 180) || !isHubPlaintext(plaintext)) return null;
  try {
    return parsePreparedEncryptedText(await invoke<unknown>("prepare_encrypted_text", { contextToken, plaintext }));
  } catch { return null; }
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
    return parseDecryptedHubPlaintext(await invoke<unknown>("decrypt_hub_capsule", {
      contextToken,
      senderOslId,
      serviceMessageId,
      capsule,
    }));
  } catch { return null; }
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
    return parseLocalProtectedText(await invoke<unknown>("prepare_local_protected_text_with_policy", {
      contextToken,
      plaintext,
      viewOnce,
    }));
  } catch { return null; }
}

export async function decryptLocalProtectedText(
  contextToken: string,
  capsule: string,
): Promise<DecryptedLocalProtectedText | null> {
  if (!isTauriRuntime() || !safeContextToken(contextToken) || !boundedUtf8Text(capsule, HUB_CAPSULE_MAX_BYTES)) return null;
  try {
    return parseDecryptedLocalProtectedText(await invoke<unknown>("decrypt_local_protected_capsule", {
      contextToken,
      capsule,
    }));
  } catch { return null; }
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
    return parsePreparedHubAttachment(await invoke<unknown>("prepare_hub_attachment", {
      contextToken,
      originalBytesB64,
      originalFilename,
    }));
  } catch { return null; }
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
    return parseOpenedHubAttachment(await invoke<unknown>("open_hub_attachment", {
      contextToken,
      senderOslId,
      serviceMessageId,
      sealedB64,
    }));
  } catch { return null; }
}

export async function loadFriendProfile(): Promise<FriendProfile | null> {
  if (!isTauriRuntime()) return null;
  try {
    const raw = await invoke<unknown>("export_hub_friend_code");
    return parseFriendProfile(raw);
  } catch { return null; }
}

export async function addOslFriend(code: string, nickname = ""): Promise<boolean> {
  const trimmed = nickname.trim();
  if (!isTauriRuntime() || !/^OSLFR1\.[A-Za-z0-9_-]{16,8192}$/.test(code) || !validFriendNickname(trimmed)) return false;
  try { await invoke("add_hub_friend", { friendCode: code, alias: trimmed || null }); return true; }
  catch { return false; }
}

export async function listHubPeople(): Promise<HubPerson[] | null> {
  if (!isTauriRuntime()) return null;
  try {
    const raw = await invoke<unknown>("list_hub_people");
    if (!Array.isArray(raw) || raw.length > 1_024) return null;
    const people = raw.map(parseHubPerson);
    return people.every((person): person is HubPerson => person !== null) ? people : null;
  } catch { return null; }
}

export async function setHubFriendNickname(personId: string, nickname: string): Promise<HubPerson | null> {
  const trimmed = nickname.trim();
  if (!isTauriRuntime() || !safe(personId, 180) || !validFriendNickname(trimmed)) return null;
  try {
    return parseHubPerson(await invoke<unknown>("set_hub_friend_nickname", {
      personId,
      nickname: trimmed || null,
    }));
  } catch { return null; }
}

export async function verifyHubPerson(personId: string, safetyNumber: string): Promise<boolean> {
  if (!isTauriRuntime() || !safe(personId, 180) || !safe(safetyNumber, 180)) return false;
  try { await invoke("verify_hub_friend_safety_number", { personId, safetyNumber }); return true; }
  catch { return false; }
}

export async function setActiveHubFriendPermission(contextToken: string, personId: string, enabled: boolean, broadened = false): Promise<boolean> {
  if (!isTauriRuntime() || !safe(contextToken, 180) || !safe(personId, 180) || typeof broadened !== "boolean") return false;
  try { await invoke("set_active_hub_friend_permission", { contextToken, personId, enabled, broadened }); return true; }
  catch { return false; }
}

export function parseHubPerson(raw: unknown): HubPerson | null {
  if (!isRecord(raw) || !exact(raw, ["personId", "oslUserId", "alias", "safetyNumber", "safetyNumberVerified", "whitelistCount", "whitelistedScopes", "whitelistedScopesTruncated", "pendingKeyChange"])) return null;
  if (!safe(raw.personId, 180) || !safe(raw.oslUserId, 180) || !(raw.alias === null || safe(raw.alias, 80)) || !safe(raw.safetyNumber, 180)) return null;
  if (typeof raw.safetyNumberVerified !== "boolean" || !Number.isSafeInteger(raw.whitelistCount) || Number(raw.whitelistCount) < 0 || Number(raw.whitelistCount) > 1_000_000 || typeof raw.whitelistedScopesTruncated !== "boolean" || typeof raw.pendingKeyChange !== "boolean") return null;
  if (!Array.isArray(raw.whitelistedScopes) || raw.whitelistedScopes.length > 512) return null;
  const whitelistedScopes = raw.whitelistedScopes.map(parseHubPersonWhitelistScope);
  if (!whitelistedScopes.every((scope): scope is HubPersonWhitelistScope => scope !== null)) return null;
  return { ...raw, whitelistedScopes } as unknown as HubPerson;
}

function parseHubPersonWhitelistScope(raw: unknown): HubPersonWhitelistScope | null {
  if (!isRecord(raw) || !exact(raw, ["kind", "contextId"])) return null;
  if (!["dm", "group", "channel", "space"].includes(String(raw.kind))) return null;
  if (!(raw.contextId === null || safePlaintext(raw.contextId, 512))) return null;
  return raw as unknown as HubPersonWhitelistScope;
}

function validFriendNickname(value: string): boolean {
  if (!value) return true;
  return new TextEncoder().encode(value).length <= 80
    && Array.from(value).length <= 48
    && !/[<>\u0000-\u001f\u007f\u200b-\u200d\u202a-\u202e\u2060\u2066-\u2069]/u.test(value);
}

export async function loadAppNotifications(): Promise<AppNotification[] | null> {
  if (!isTauriRuntime()) return null;
  try {
    const raw = await invoke<unknown>("list_hub_app_notifications");
    return parseNotifications(raw);
  } catch { return null; }
}

export async function setNotificationsEnabled(enabled: boolean): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  try { await invoke("set_hub_notifications_enabled", { enabled }); return true; }
  catch { return false; }
}

export async function setScreenshotProtection(enabled: boolean): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  try { await invoke("set_hub_screenshot_protection", { enabled }); return true; }
  catch { return false; }
}

export async function listHubIdentities(): Promise<HubIdentitySlot[] | null> {
  if (!isTauriRuntime()) return null;
  try {
    const raw = await invoke<unknown>("list_hub_identities");
    if (!Array.isArray(raw) || raw.length > 16) return null;
    const parsed = raw.map(parseIdentitySlot);
    return parsed.every((item): item is HubIdentitySlot => item !== null) ? parsed : null;
  } catch { return null; }
}

export async function createHubIdentitySlot(label: string): Promise<HubIdentityCreation | null> {
  if (!isTauriRuntime() || !safe(label, 80)) return null;
  try { return parseIdentityCreation(await invoke<unknown>("create_hub_identity_slot", { label })); }
  catch { return null; }
}

export async function recoverHubIdentitySlot(label: string, identityRecoveryPhrase: string): Promise<HubIdentityCreation | null> {
  if (!isTauriRuntime() || !safe(label, 80) || !safePlaintext(identityRecoveryPhrase, 512)) return null;
  try { return parseIdentityCreation(await invoke<unknown>("recover_hub_identity_slot", { label, identityRecoveryPhrase })); }
  catch { return null; }
}

export async function switchHubIdentity(slotId: string): Promise<boolean> {
  if (!isTauriRuntime() || !/^[A-Za-z0-9_-]{8,80}$/.test(slotId)) return false;
  try { await invoke("switch_hub_identity", { slotId }); return true; }
  catch { return false; }
}

export async function burnActiveHubIdentity(): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  try { await invoke("burn_active_hub_identity"); return true; }
  catch { return false; }
}

export async function executeHubFullCleanup(): Promise<HubFullCleanupResult | null> {
  if (!isTauriRuntime()) return null;
  try { return parseFullCleanup(await invoke<unknown>("execute_hub_full_cleanup")); }
  catch { return null; }
}

export async function scanLocalPrivacy(messages: LocalMessageCandidate[]): Promise<LocalPrivacyScanResult | null> {
  if (!isTauriRuntime() || messages.length > 2_000 || !messages.every(validLocalCandidate)) return null;
  try { return parseLocalPrivacyScan(await invoke<unknown>("scan_local_privacy", { messages })); }
  catch { return null; }
}

export async function burnActiveHubContext(contextToken: string): Promise<boolean> {
  if (!isTauriRuntime() || !safe(contextToken, 180)) return false;
  try { await invoke("burn_active_hub_context", { contextToken }); return true; }
  catch { return false; }
}

export async function getHubServiceBurnReadiness(serviceId: string, accountId: string): Promise<HubServiceBurnReadiness | null> {
  if (!isTauriRuntime() || !safeId(serviceId, 32) || !safePlaintext(accountId, 128)) return null;
  try { return parseHubServiceBurnReadiness(await invoke<unknown>("get_hub_service_burn_readiness", { serviceId, accountId })); }
  catch { return null; }
}

export async function burnHubServiceAccount(serviceId: string, accountId: string, confirmedBurnId: string): Promise<HubServiceBurnResult | null> {
  if (!isTauriRuntime() || !safeId(serviceId, 32) || !safePlaintext(accountId, 128) || !/^[a-f0-9]{64}$/.test(confirmedBurnId)) return null;
  try { return parseHubServiceBurnResult(await invoke<unknown>("burn_hub_service_account", { serviceId, accountId, confirmedBurnId })); }
  catch { return null; }
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

export async function loadActiveContextSecurity(contextToken: string): Promise<ScopeSecurity | null> {
  if (!isTauriRuntime() || !safe(contextToken, 180)) return null;
  try { return parseScopeSecurity(await invoke<unknown>("get_active_hub_context_security", { contextToken })); }
  catch { return null; }
}

export async function saveActiveContextSecurity(contextToken: string, ttlSeconds: number, decryptDisplayEnabled: boolean): Promise<ScopeSecurity | null> {
  if (!isTauriRuntime() || !safe(contextToken, 180) || !Number.isSafeInteger(ttlSeconds) || ttlSeconds < 0 || ttlSeconds > 31_536_000) return null;
  try { return parseScopeSecurity(await invoke<unknown>("set_active_hub_context_security", { contextToken, ttlSeconds, decryptDisplayEnabled })); }
  catch { return null; }
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
  if (!isRecord(raw) || !exact(raw, ["findings", "messagesScanned", "messagesRejected", "truncated", "analysisLocation", "persisted"])) return null;
  if (!Array.isArray(raw.findings) || raw.findings.length > 1_000 || !Number.isSafeInteger(raw.messagesScanned) || Number(raw.messagesScanned) < 0 || Number(raw.messagesScanned) > 2_000 || !Number.isSafeInteger(raw.messagesRejected) || Number(raw.messagesRejected) < 0 || typeof raw.truncated !== "boolean" || raw.analysisLocation !== "this_device_only" || raw.persisted !== persisted) return null;
  const findings = raw.findings.map(parsePrivacyFinding);
  if (!findings.every((finding): finding is LocalPrivacyFinding => finding !== null)) return null;
  return { ...raw, findings } as LocalPrivacyScanResult | PersistedLocalPrivacyScanResult;
}

function parsePrivacyFinding(raw: unknown): LocalPrivacyFinding | null {
  if (!isRecord(raw) || !exact(raw, ["serviceId", "accountId", "conversationId", "messageLocator", "authoredBySelf", "createdAtUnixMs", "category", "confidence", "reason", "localPreview", "canRequestDelete"])) return null;
  if (!safeId(raw.serviceId, 32) || !safePlaintext(raw.accountId, 128) || !safePlaintext(raw.conversationId, 256) || !safePlaintext(raw.messageLocator, 256) || typeof raw.authoredBySelf !== "boolean" || !(raw.createdAtUnixMs === null || Number.isSafeInteger(raw.createdAtUnixMs)) || !["credential", "recovery_material", "payment_card", "government_identity", "precise_location", "profanity", "sexual_content", "sensitive_health", "controlled_substances", "potentially_unlawful_conduct", "work_sensitive_information"].includes(String(raw.category)) || !Number.isSafeInteger(raw.confidence) || Number(raw.confidence) < 0 || Number(raw.confidence) > 100 || !safePlaintext(raw.reason, 240) || !safePlaintext(raw.localPreview, 256) || typeof raw.canRequestDelete !== "boolean") return null;
  if (raw.canRequestDelete && !raw.authoredBySelf) return null;
  return raw as unknown as LocalPrivacyFinding;
}

function validLocalCandidate(candidate: LocalMessageCandidate): boolean {
  return safeId(candidate.serviceId, 32)
    && safePlaintext(candidate.accountId, 128)
    && safePlaintext(candidate.conversationId, 256)
    && safePlaintext(candidate.messageLocator, 256)
    && typeof candidate.authoredBySelf === "boolean"
    && (candidate.createdAtUnixMs === null || Number.isSafeInteger(candidate.createdAtUnixMs))
    && safePlaintext(candidate.text, 8 * 1024);
}

function parseIdentityCreation(raw: unknown): HubIdentityCreation | null {
  if (!isRecord(raw) || !exact(raw, ["identity", "identityRecoveryPhrase", "storageMethod"])) return null;
  const identity = parseIdentitySlot(raw.identity);
  if (!identity || !(raw.identityRecoveryPhrase === null || safePlaintext(raw.identityRecoveryPhrase, 512)) || !safe(raw.storageMethod, 80)) return null;
  return { identity, identityRecoveryPhrase: raw.identityRecoveryPhrase as string | null, storageMethod: raw.storageMethod };
}

function parseIdentitySlot(raw: unknown): HubIdentitySlot | null {
  if (!isRecord(raw) || !exact(raw, ["slotId", "label", "oslUserId", "safetyNumber", "active"])) return null;
  if (!/^[A-Za-z0-9_-]{8,80}$/.test(String(raw.slotId)) || !safe(raw.label, 80) || !safe(raw.oslUserId, 180) || !safe(raw.safetyNumber, 180) || typeof raw.active !== "boolean") return null;
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
  if (!isRecord(raw) || !exact(raw, ["coverText", "expiresAt", "personToPersonE2ee"])) return null;
  if (!boundedUtf8Text(raw.coverText, HUB_CAPSULE_MAX_BYTES)
    || !Number.isSafeInteger(raw.expiresAt)
    || Number(raw.expiresAt) <= 0
    || raw.personToPersonE2ee !== true) return null;
  return raw as unknown as PreparedPeerProseText;
}

export function parseOpenedPeerProseText(raw: unknown): OpenedPeerProseText | null {
  if (!isRecord(raw) || !exact(raw, ["plaintext", "contextVerified", "personToPersonE2ee"])) return null;
  if (!isHubPlaintext(raw.plaintext)
    || raw.contextVerified !== true
    || raw.personToPersonE2ee !== true) return null;
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
function safeId(value: unknown, max: number): value is string { return typeof value === "string" && value.length > 0 && value.length <= max && /^[a-z0-9_-]+$/.test(value); }
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
