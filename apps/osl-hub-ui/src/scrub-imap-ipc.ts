import { invoke } from "@tauri-apps/api/core";

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

export interface ScrubImapDeleteReceipt {
  accountId: string;
  mailbox: string;
  messageId: string;
  deletedUid: number;
}

export interface ScrubImapIpcPort {
  invoke(command: string, args: Record<string, unknown>): Promise<unknown>;
}

const productionPort: ScrubImapIpcPort = {
  invoke: (command, args) => invoke(command, args),
};

export async function scrubImapPrepareDelete(
  request: ScrubImapPrepareDeleteRequest,
  port: ScrubImapIpcPort = productionPort,
): Promise<ScrubImapPreparedDelete> {
  validatePrepareRequest(request);
  return parsePreparedDelete(
    await port.invoke("scrub_imap_prepare_delete", { request }),
    request,
  );
}

export async function scrubImapDelete(
  prepared: ScrubImapPreparedDelete,
  port: ScrubImapIpcPort = productionPort,
): Promise<ScrubImapDeleteReceipt> {
  validatePreparedDelete(prepared);
  return parseDeleteReceipt(
    await port.invoke("scrub_imap_delete", { prepared }),
    prepared,
  );
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
  ])
    || raw.ownerOslUserId !== request.ownerOslUserId
    || raw.accountId !== request.accountId
    || raw.mailbox !== request.mailbox
    || raw.messageId !== request.messageId
    || !positiveU32(raw.preparedUid)
    || !byteArray(raw.fingerprint, 32)
    || !byteArray(raw.batchDigest, 32)) {
    throw new Error("invalid scrub IMAP prepared delete response");
  }
  return {
    ownerOslUserId: raw.ownerOslUserId,
    accountId: raw.accountId,
    mailbox: raw.mailbox,
    messageId: raw.messageId,
    preparedUid: raw.preparedUid,
    fingerprint: [...raw.fingerprint],
    batchDigest: [...raw.batchDigest],
  };
}

function parseDeleteReceipt(
  raw: unknown,
  prepared: ScrubImapPreparedDelete,
): ScrubImapDeleteReceipt {
  if (!exactRecord(raw, ["accountId", "mailbox", "messageId", "deletedUid"])
    || raw.accountId !== prepared.accountId
    || raw.mailbox !== prepared.mailbox
    || raw.messageId !== prepared.messageId
    || raw.deletedUid !== prepared.preparedUid) {
    throw new Error("invalid scrub IMAP delete receipt");
  }
  return {
    accountId: raw.accountId,
    mailbox: raw.mailbox,
    messageId: raw.messageId,
    deletedUid: raw.deletedUid,
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
  return Number.isSafeInteger(value) && (value as number) > 0 && (value as number) <= 0xffff_ffff;
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
    && /^[A-Za-z0-9_.@<>-]+$/u.test(value);
}
