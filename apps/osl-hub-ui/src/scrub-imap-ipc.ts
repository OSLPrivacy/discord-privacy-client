import { invoke } from "@tauri-apps/api/core";

export interface NativeImapMessageSnapshot {
  ownerOslUserId: string;
  accountId: string;
  mailbox: string;
  messageId: string;
  uid: number;
  fingerprint: number[];
  authoredBySelf: boolean;
}

export interface NativePreparedImapDelete {
  ownerOslUserId: string;
  accountId: string;
  mailbox: string;
  messageId: string;
  preparedUid: number;
  fingerprint: number[];
  batchDigest: number[];
}

export interface NativeImapDeleteReceipt {
  accountId: string;
  mailbox: string;
  messageId: string;
  deletedUid: number;
}

export interface ScrubImapPrepareDeleteRequest {
  ownerOslUserId: string;
  accountId: string;
  mailbox: string;
  messageId: string;
}

export interface ScrubImapDeletePort {
  prepareDelete(request: ScrubImapPrepareDeleteRequest): Promise<unknown>;
  delete(prepared: NativePreparedImapDelete): Promise<unknown>;
}

const productionPort: ScrubImapDeletePort = {
  prepareDelete: (request) => invoke("scrub_imap_prepare_delete", { request }),
  delete: (prepared) => invoke("scrub_imap_delete", { prepared }),
};

export async function scrubImapPrepareDelete(
  request: ScrubImapPrepareDeleteRequest,
  port: ScrubImapDeletePort = productionPort,
): Promise<NativePreparedImapDelete> {
  validatePrepareDeleteRequest(request);
  return parsePreparedImapDelete(await port.prepareDelete(request), request);
}

export async function scrubImapDelete(
  prepared: NativePreparedImapDelete,
  port: ScrubImapDeletePort = productionPort,
): Promise<NativeImapDeleteReceipt> {
  validatePreparedImapDelete(prepared);
  return parseImapDeleteReceipt(await port.delete(prepared), prepared);
}

function validatePrepareDeleteRequest(request: ScrubImapPrepareDeleteRequest): void {
  if (!exactRecord(request, ["ownerOslUserId", "accountId", "mailbox", "messageId"])
    || !safeBinding(request.ownerOslUserId, 128)
    || !safeBinding(request.accountId, 128)
    || !safeBinding(request.mailbox, 128)
    || !safeBinding(request.messageId, 256)) {
    throw new Error("invalid scrub IMAP prepare-delete request");
  }
}

function validatePreparedImapDelete(prepared: NativePreparedImapDelete): void {
  if (!isPreparedImapDelete(prepared)) {
    throw new Error("invalid scrub IMAP prepared delete");
  }
}

function parsePreparedImapDelete(
  raw: unknown,
  request: ScrubImapPrepareDeleteRequest,
): NativePreparedImapDelete {
  if (!isPreparedImapDelete(raw)
    || raw.ownerOslUserId !== request.ownerOslUserId
    || raw.accountId !== request.accountId
    || raw.mailbox !== request.mailbox
    || raw.messageId !== request.messageId) {
    throw new Error("invalid scrub IMAP prepared delete");
  }
  return raw;
}

function parseImapDeleteReceipt(
  raw: unknown,
  prepared: NativePreparedImapDelete,
): NativeImapDeleteReceipt {
  if (!isNativeImapDeleteReceipt(raw)
    || raw.accountId !== prepared.accountId
    || raw.mailbox !== prepared.mailbox
    || raw.messageId !== prepared.messageId
    || raw.deletedUid !== prepared.preparedUid) {
    throw new Error("invalid scrub IMAP delete receipt");
  }
  return raw;
}

function isNativeImapDeleteReceipt(value: unknown): value is NativeImapDeleteReceipt {
  return exactRecord(value, ["accountId", "mailbox", "messageId", "deletedUid"])
    && safeBinding(value.accountId, 128)
    && safeBinding(value.mailbox, 128)
    && safeBinding(value.messageId, 256)
    && safeCounter(value.deletedUid);
}

function isPreparedImapDelete(value: unknown): value is NativePreparedImapDelete {
  return exactRecord(value, [
    "ownerOslUserId",
    "accountId",
    "mailbox",
    "messageId",
    "preparedUid",
    "fingerprint",
    "batchDigest",
  ])
    && safeBinding(value.ownerOslUserId, 128)
    && safeBinding(value.accountId, 128)
    && safeBinding(value.mailbox, 128)
    && safeBinding(value.messageId, 256)
    && safeCounter(value.preparedUid)
    && byteArray(value.fingerprint, 32)
    && byteArray(value.batchDigest, 32);
}

function exactRecord(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}

function safeBinding(value: unknown, maxBytes: number): value is string {
  return typeof value === "string"
    && value.length > 0
    && new TextEncoder().encode(value).length <= maxBytes
    && !/[\u0000-\u001f\u007f]/u.test(value);
}

function safeCounter(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function byteArray(value: unknown, length: number): value is number[] {
  return Array.isArray(value)
    && value.length === length
    && value.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255);
}
