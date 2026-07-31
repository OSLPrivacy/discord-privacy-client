export type WhatsAppVerificationStatus = "verified" | "platformUnsupported" | "hostUnavailable" | "selectorContractUnverified" | "contextIncomplete" | "recipientSetAmbiguous" | "geometryRejected" | "contextChanged";
export type WhatsAppProtectedAction = "text" | "multilineUtf8" | "covertext" | "burn" | "expiry" | "replayRejection" | "malformedPayloadRejection" | "media" | "caption" | "openedReceipt";
export type WhatsAppProtectedActionStatus = "readyForExplicitPlacement" | "localLifecycleApplied" | "rejectedSafely" | "receiptReady" | "receiptPendingOffline" | "receiptUnavailable" | "contextUnverified" | "evidenceMismatch";

export interface WhatsAppVerificationReceipt {
  status: WhatsAppVerificationStatus;
  provider: "whatsapp";
  selectorRevision: string | null;
  accountVerified: boolean;
  chatVerified: boolean;
  recipientSetVerified: boolean;
  composerVerified: boolean;
  transcriptVerified: boolean;
  contextBindingSha256: string | null;
  recipientSetSha256: string | null;
  windowGeneration: number | null;
  composerRect: [number, number, number, number] | null;
  transcriptRect: [number, number, number, number] | null;
  protectedControlsAvailable: boolean;
}

export interface WhatsAppProtectedActionReceipt {
  action: WhatsAppProtectedAction;
  status: WhatsAppProtectedActionStatus;
  provider: "whatsapp";
  contextBindingSha256: string | null;
  messageCommitmentSha256: string | null;
  composerRect: [number, number, number, number] | null;
  transcriptRect: [number, number, number, number] | null;
  realMessageSent: false;
  providerHistoryChanged: false;
  providerStorageRead: false;
}

const verificationStatuses: readonly WhatsAppVerificationStatus[] = ["verified", "platformUnsupported", "hostUnavailable", "selectorContractUnverified", "contextIncomplete", "recipientSetAmbiguous", "geometryRejected", "contextChanged"];
const actions: readonly WhatsAppProtectedAction[] = ["text", "multilineUtf8", "covertext", "burn", "expiry", "replayRejection", "malformedPayloadRejection", "media", "caption", "openedReceipt"];
const actionStatuses: readonly WhatsAppProtectedActionStatus[] = ["readyForExplicitPlacement", "localLifecycleApplied", "rejectedSafely", "receiptReady", "receiptPendingOffline", "receiptUnavailable", "contextUnverified", "evidenceMismatch"];
const sha256 = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const exact = (value: unknown, keys: readonly string[]): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value) && Object.keys(value).sort().join(",") === [...keys].sort().join(",");
const rect = (value: unknown): value is [number, number, number, number] => Array.isArray(value) && value.length === 4 && value.every(Number.isSafeInteger) && value[2] > value[0] && value[3] > value[1];

export function parseWhatsAppVerificationReceipt(raw: unknown): WhatsAppVerificationReceipt {
  const keys = ["status", "provider", "selectorRevision", "accountVerified", "chatVerified", "recipientSetVerified", "composerVerified", "transcriptVerified", "contextBindingSha256", "recipientSetSha256", "windowGeneration", "composerRect", "transcriptRect", "protectedControlsAvailable"];
  if (!exact(raw, keys) || raw.provider !== "whatsapp" || !verificationStatuses.includes(raw.status as WhatsAppVerificationStatus)) throw new Error("invalid WhatsApp verification receipt");
  const verified = raw.status === "verified";
  const flags = [raw.accountVerified, raw.chatVerified, raw.recipientSetVerified, raw.composerVerified, raw.transcriptVerified, raw.protectedControlsAvailable];
  if (verified) {
    if (!flags.every((flag) => flag === true) || typeof raw.selectorRevision !== "string" || !sha256(raw.contextBindingSha256) || !sha256(raw.recipientSetSha256) || !Number.isSafeInteger(raw.windowGeneration) || Number(raw.windowGeneration) <= 0 || !rect(raw.composerRect) || !rect(raw.transcriptRect)) throw new Error("invalid WhatsApp verification receipt");
  } else if (!flags.every((flag) => flag === false) || raw.selectorRevision !== null || raw.contextBindingSha256 !== null || raw.recipientSetSha256 !== null || raw.windowGeneration !== null || raw.composerRect !== null || raw.transcriptRect !== null) throw new Error("invalid WhatsApp verification receipt");
  return raw as unknown as WhatsAppVerificationReceipt;
}

export function parseWhatsAppProtectedActionReceipt(raw: unknown): WhatsAppProtectedActionReceipt {
  const keys = ["action", "status", "provider", "contextBindingSha256", "messageCommitmentSha256", "composerRect", "transcriptRect", "realMessageSent", "providerHistoryChanged", "providerStorageRead"];
  if (!exact(raw, keys) || raw.provider !== "whatsapp" || !actions.includes(raw.action as WhatsAppProtectedAction) || !actionStatuses.includes(raw.status as WhatsAppProtectedActionStatus) || raw.realMessageSent !== false || raw.providerHistoryChanged !== false || raw.providerStorageRead !== false) throw new Error("invalid WhatsApp protected-control receipt");
  const bound = raw.status !== "contextUnverified";
  if (bound ? (!sha256(raw.contextBindingSha256) || (raw.messageCommitmentSha256 !== null && !sha256(raw.messageCommitmentSha256)) || !rect(raw.composerRect) || !rect(raw.transcriptRect)) : (raw.contextBindingSha256 !== null || raw.messageCommitmentSha256 !== null || raw.composerRect !== null || raw.transcriptRect !== null)) throw new Error("invalid WhatsApp protected-control receipt");
  return raw as unknown as WhatsAppProtectedActionReceipt;
}

export function createWhatsAppProtectedControls() {
  let verification: WhatsAppVerificationReceipt | null = null;
  return {
    bind(raw: unknown): boolean {
      const parsed = parseWhatsAppVerificationReceipt(raw);
      verification = parsed.status === "verified" ? parsed : null;
      return verification !== null;
    },
    clear(): void { verification = null; },
    available(): boolean { return verification !== null; },
    accept(raw: unknown): WhatsAppProtectedActionReceipt | null {
      if (!verification) return null;
      const receipt = parseWhatsAppProtectedActionReceipt(raw);
      return receipt.contextBindingSha256 === verification.contextBindingSha256 ? receipt : null;
    },
  };
}
