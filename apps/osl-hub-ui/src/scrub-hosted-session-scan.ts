export type SessionKind = "browser_account" | "new_account";
export type OwnerId = string;
export type OwnerIdl = OwnerId;

export interface OwnSessionProofInput {
  ownerId: OwnerId;
  serviceId: string;
  accountId: string;
  sessionKind: SessionKind;
  authEpoch: number;
  proofNonce: string;
  issuedAtUnixMs: number;
  expiresAtUnixMs: number;
}

export interface DeleteCapabilityInput {
  ownerProof: OwnSessionProofInput;
  scopeFingerprint: string;
  findingFingerprint: string;
  mode: "manual_jump" | "documented_provider_api";
  attended: boolean;
  unattended: false;
}

export interface HostedSessionScanRequest {
  ownerProof: OwnSessionProofInput;
  scopeFingerprint: string;
  maxMessages: number;
}

export interface HostedSessionScanReceipt {
  ownerId: OwnerId;
  serviceId: string;
  accountId: string;
  sessionKind: SessionKind;
  authEpoch: number;
  scopeFingerprint: string;
  messagesScanned: number;
  deleteCapability: "none" | "attended_only";
}

export interface HostedSessionScanPort {
  scan(request: HostedSessionScanRequest): Promise<HostedSessionScanReceipt>;
  requestDeleteCapability(input: DeleteCapabilityInput): Promise<"attended_only" | null>;
}

export function validateOwnSessionProofInput(value: unknown): value is OwnSessionProofInput {
  return exactRecord(value, [
    "ownerId",
    "serviceId",
    "accountId",
    "sessionKind",
    "authEpoch",
    "proofNonce",
    "issuedAtUnixMs",
    "expiresAtUnixMs",
  ])
    && validOwnerId(value.ownerId)
    && validServiceId(value.serviceId)
    && validAccountId(value.accountId)
    && (value.sessionKind === "browser_account" || value.sessionKind === "new_account")
    && positiveInteger(value.authEpoch)
    && validHex(value.proofNonce, 64)
    && nonnegativeInteger(value.issuedAtUnixMs)
    && nonnegativeInteger(value.expiresAtUnixMs)
    && value.expiresAtUnixMs > value.issuedAtUnixMs;
}

export function validateDeleteCapabilityInput(value: unknown): value is DeleteCapabilityInput {
  return exactRecord(value, [
    "ownerProof",
    "scopeFingerprint",
    "findingFingerprint",
    "mode",
    "attended",
    "unattended",
  ])
    && validateOwnSessionProofInput(value.ownerProof)
    && validHex(value.scopeFingerprint, 64)
    && validHex(value.findingFingerprint, 64)
    && (value.mode === "manual_jump" || value.mode === "documented_provider_api")
    && value.attended === true
    && value.unattended === false;
}

export async function scanHostedSession(
  port: HostedSessionScanPort,
  request: HostedSessionScanRequest,
): Promise<HostedSessionScanReceipt> {
  if (!exactRecord(request, ["ownerProof", "scopeFingerprint", "maxMessages"])
    || !validateOwnSessionProofInput(request.ownerProof)
    || !validHex(request.scopeFingerprint, 64)
    || !positiveInteger(request.maxMessages)
    || request.maxMessages > 5_000) {
    throw new Error("invalid hosted session scan request");
  }
  const receipt = await port.scan(request);
  if (receipt.ownerId !== request.ownerProof.ownerId
    || receipt.serviceId !== request.ownerProof.serviceId
    || receipt.accountId !== request.ownerProof.accountId
    || receipt.sessionKind !== request.ownerProof.sessionKind
    || receipt.authEpoch !== request.ownerProof.authEpoch
    || receipt.scopeFingerprint !== request.scopeFingerprint
    || !nonnegativeInteger(receipt.messagesScanned)
    || receipt.messagesScanned > request.maxMessages
    || !(receipt.deleteCapability === "none" || receipt.deleteCapability === "attended_only")) {
    throw new Error("invalid hosted session scan receipt");
  }
  return receipt;
}

export async function requestHostedDeleteCapability(
  port: HostedSessionScanPort,
  input: DeleteCapabilityInput,
): Promise<"attended_only" | null> {
  if (!validateDeleteCapabilityInput(input)) {
    throw new Error("invalid hosted session delete capability request");
  }
  return port.requestDeleteCapability(input);
}

function exactRecord(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}

function positiveInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0;
}

function nonnegativeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function validHex(value: unknown, length: number): value is string {
  return typeof value === "string" && value.length === length && /^[a-f0-9]+$/u.test(value);
}

function validOwnerId(value: unknown): value is OwnerId {
  return typeof value === "string" && /^[a-z0-9][a-z0-9_-]{0,63}$/u.test(value);
}

function validServiceId(value: unknown): value is string {
  return typeof value === "string" && /^[a-z0-9_-]{1,32}$/u.test(value);
}

function validAccountId(value: unknown): value is string {
  return typeof value === "string" && /^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$/u.test(value);
}
