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

export type NativeImapAuthInput =
  | { password: { username: string; password: string } }
  | { oAuthBearer: { username: string; bearerToken: string } };

export interface NativeConfigureImapRequest {
  ownerOslUserId: string;
  accountId: string;
  host: string;
  port: number;
  tlsRequired: boolean;
  auth: NativeImapAuthInput;
}

export interface ScrubImapPrepareDeleteRequest {
  ownerOslUserId: string;
  accountId: string;
  mailbox: string;
  messageId: string;
}

export interface ScrubImapPreparedDelete {
  ownerOslUserId: string;
  accountId: string;
  mailbox: string;
  messageId: string;
  preparedUid: number;
  fingerprint: readonly number[];
  batchDigest: readonly number[];
}

export type NativePreparedImapDelete = ScrubImapPreparedDelete;

export interface ScrubImapDeleteReceipt {
  accountId: string;
  mailbox: string;
  messageId: string;
  deletedUid: number;
}

export type NativeImapDeleteReceipt = ScrubImapDeleteReceipt;

export interface ScrubImapIpcPort {
  invoke(command: string, args: Record<string, unknown>): Promise<unknown>;
}

export interface ScrubImapDeletePort {
  invoke?: (command: string, args: Record<string, unknown>) => Promise<unknown>;
  prepareDelete?: (request: ScrubImapPrepareDeleteRequest) => Promise<unknown>;
  delete?: (prepared: ScrubImapPreparedDelete) => Promise<unknown>;
}

const productionPort: ScrubImapIpcPort = {
  invoke: (command, args) => invoke(command, args),
};

export async function scrubImapPrepareDelete(
  request: ScrubImapPrepareDeleteRequest,
  port: ScrubImapDeletePort = productionPort,
): Promise<ScrubImapPreparedDelete> {
  validatePrepareRequest(request);
  return parsePreparedDelete(await invokePrepareDelete(port, request), request);
}

export async function scrubImapDelete(
  prepared: ScrubImapPreparedDelete,
  port: ScrubImapDeletePort = productionPort,
): Promise<ScrubImapDeleteReceipt> {
  validatePreparedDelete(prepared);
  return parseDeleteReceipt(await invokeDelete(port, prepared), prepared);
}

function invokePrepareDelete(
  port: ScrubImapDeletePort,
  request: ScrubImapPrepareDeleteRequest,
): Promise<unknown> {
  if (port.invoke) {
    return port.invoke("scrub_imap_prepare_delete", { request });
  }
  if (port.prepareDelete) {
    return port.prepareDelete(request);
  }
  throw new Error("invalid scrub IMAP IPC port");
}

function invokeDelete(
  port: ScrubImapDeletePort,
  prepared: ScrubImapPreparedDelete,
): Promise<unknown> {
  if (port.invoke) {
    return port.invoke("scrub_imap_delete", { prepared });
  }
  if (port.delete) {
    return port.delete(prepared);
  }
  throw new Error("invalid scrub IMAP IPC port");
}

function parsePreparedDelete(
  raw: unknown,
  request: ScrubImapPrepareDeleteRequest,
): ScrubImapPreparedDelete {
  if (!exactRecord(raw, [
    "ownerOslUserId",
    "accountId",
    "mailbox",
    "messageId",
    "preparedUid",
    "fingerprint",
    "batchDigest",
  ])) {
    throw new Error("invalid scrub IMAP prepared delete response");
  }
  const preparedUid = raw.preparedUid;
  const fingerprint = raw.fingerprint;
  const batchDigest = raw.batchDigest;
  if (raw.ownerOslUserId !== request.ownerOslUserId
    || raw.accountId !== request.accountId
    || raw.mailbox !== request.mailbox
    || raw.messageId !== request.messageId
    || !positiveU32(preparedUid)
    || !byteArray(fingerprint, 32)
    || !byteArray(batchDigest, 32)) {
    throw new Error("invalid scrub IMAP prepared delete response");
  }
  return {
    ownerOslUserId: request.ownerOslUserId,
    accountId: request.accountId,
    mailbox: request.mailbox,
    messageId: request.messageId,
    preparedUid,
    fingerprint: [...fingerprint],
    batchDigest: [...batchDigest],
  };
}

function parseDeleteReceipt(
  raw: unknown,
  prepared: ScrubImapPreparedDelete,
): ScrubImapDeleteReceipt {
  if (!exactRecord(raw, ["accountId", "mailbox", "messageId", "deletedUid"])) {
    throw new Error("invalid scrub IMAP delete receipt");
  }
  const deletedUid = raw.deletedUid;
  if (raw.accountId !== prepared.accountId
    || raw.mailbox !== prepared.mailbox
    || raw.messageId !== prepared.messageId
    || deletedUid !== prepared.preparedUid
    || !positiveU32(deletedUid)) {
    throw new Error("invalid scrub IMAP delete receipt");
  }
  return {
    accountId: prepared.accountId,
    mailbox: prepared.mailbox,
    messageId: prepared.messageId,
    deletedUid,
  };
}

function validatePrepareRequest(request: ScrubImapPrepareDeleteRequest): void {
  if (!exactRecord(request, ["ownerOslUserId", "accountId", "mailbox", "messageId"])
    || !binding(request.ownerOslUserId, 128)
    || !binding(request.accountId, 128)
    || !binding(request.mailbox, 128)
    || !binding(request.messageId, 256)) {
    throw new Error("invalid scrub IMAP prepare delete request");
  }
}

function validatePreparedDelete(prepared: ScrubImapPreparedDelete): void {
  if (!exactRecord(prepared, [
    "ownerOslUserId",
    "accountId",
    "mailbox",
    "messageId",
    "preparedUid",
    "fingerprint",
    "batchDigest",
  ])
    || !binding(prepared.ownerOslUserId, 128)
    || !binding(prepared.accountId, 128)
    || !binding(prepared.mailbox, 128)
    || !binding(prepared.messageId, 256)
    || !positiveU32(prepared.preparedUid)
    || !byteArray(prepared.fingerprint, 32)
    || !byteArray(prepared.batchDigest, 32)) {
    throw new Error("invalid scrub IMAP prepared delete");
  }
}

function exactRecord(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}

function positiveU32(value: unknown): value is number {
  return typeof value === "number"
    && Number.isSafeInteger(value)
    && value > 0
    && value <= 0xffff_ffff;
}

function byteArray(value: unknown, length: number): value is readonly number[] {
  return Array.isArray(value)
    && value.length === length
    && value.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255);
}

function binding(value: unknown, maxBytes: number): value is string {
  return typeof value === "string"
    && value.length > 0
    && new TextEncoder().encode(value).length <= maxBytes
    && !/[\u0000-\u001f\u007f]/u.test(value);
}
