export const MESSAGE_READ_KEY_RECORD = "osl/message-read-key/v1";
export const DELETE_GRANT_RECORD = "osl/delete-grant/v1";

const HEX_128_RE = /^[0-9a-f]{32}$/;
const BINDING_RE = /^[A-Za-z0-9:._@<>-]{1,512}$/;

export interface MessageReadKeyRecord {
  type: typeof MESSAGE_READ_KEY_RECORD;
  message: string;
  key: string;
}

export interface SenderDeleteGrantRecord {
  type: typeof DELETE_GRANT_RECORD;
  message: string;
  owner: string;
  scope: string;
  grant: string;
export const DELETE_GRANT_RECORD = "osl/delete-grant/v1";
export const MESSAGE_READ_KEY_RECORD = "osl/message-read-key/v1";

const NAME_RE = /^[A-Za-z0-9._:@/-]{1,256}$/;
const READ_KEY_RE = /^[0-9a-f]{32}$/;

export interface DeleteGrantRecord {
  record: typeof DELETE_GRANT_RECORD;
  message: string;
const NONCE_RE = /^[0-9a-f]{24}$/;
const CIPHERTEXT_RE = /^[0-9a-f]+$/;

export interface DeleteGrantRecord {
  record: typeof DELETE_GRANT_RECORD;
  owner: string;
  scope: string;
}

export interface MessageReadKeyRecord {
  record: typeof MESSAGE_READ_KEY_RECORD;
  message: string;
  readKey: string;
}

export interface StoredProtectedMessageGrantInput {
  message: string;
  readKey: string;
  sender: string;
  recipient?: string;
  scope: string;
}

export interface StoredProtectedMessageGrants {
  message: string;
  owner: string;
  scope: string;
  readKeys: readonly MessageReadKeyRecord[];
  senderDeleteGrants: readonly SenderDeleteGrantRecord[];
}

export interface RecipientMessageData {
  message: string;
  ciphertext: string;
  nonce: string;
  readKeys: readonly MessageReadKeyRecord[];
}

export type DeleteGrantParseError =
  | "not_delete_grant"
  | "bad_delete_grant_encoding"
  | "bad_delete_grant_shape";

function validBinding(value: unknown): value is string {
  return typeof value === "string" && BINDING_RE.test(value);
}

function validCap(value: unknown): value is string {
  return typeof value === "string" && HEX_128_RE.test(value);
}

function b64u(bytes: Uint8Array): string {
  let out = "";
  for (const byte of bytes) out += String.fromCharCode(byte);
  return btoa(out).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function b64uDecode(value: string): string | null {
  if (!/^[A-Za-z0-9_-]+$/.test(value)) return null;
  const padded = value.replace(/-/g, "+").replace(/_/g, "/");
  try {
    return atob(padded + "=".repeat((4 - (padded.length % 4)) % 4));
  } catch {
    return null;
  }
}

async function deriveCap(label: string, parts: readonly string[]): Promise<string> {
  const material = new TextEncoder().encode([label, ...parts].join("\u001f"));
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", material));
  return [...digest.slice(0, 16)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

export async function createStoredProtectedMessageGrants(input: {
  message: string;
  owner: string;
  scope: string;
}): Promise<StoredProtectedMessageGrants> {
  if (!validBinding(input.message) || !validBinding(input.owner) || !validBinding(input.scope)) {
    throw new Error("invalid protected message binding");
  }
  const readKey = await deriveCap("read", [input.message, input.owner, input.scope]);
  const deleteGrant = await deriveCap("delete", [input.message, input.owner, input.scope]);
  return {
    message: input.message,
    owner: input.owner,
    scope: input.scope,
    readKeys: [{
      type: MESSAGE_READ_KEY_RECORD,
      message: input.message,
      key: readKey,
    }],
    senderDeleteGrants: [{
      type: DELETE_GRANT_RECORD,
      message: input.message,
      owner: input.owner,
      scope: input.scope,
      grant: deleteGrant,
    }],
  };
}

export function recipientMessageData(input: {
  grants: StoredProtectedMessageGrants;
  ciphertext: string;
  nonce: string;
}): RecipientMessageData {
  return {
    message: input.grants.message,
    ciphertext: input.ciphertext,
    nonce: input.nonce,
    readKeys: input.grants.readKeys,
  };
}

export function encodeDeleteGrant(grant: SenderDeleteGrantRecord): string {
  return b64u(new TextEncoder().encode(JSON.stringify(grant)));
}

export function parseDeleteGrant(value: unknown): SenderDeleteGrantRecord | DeleteGrantParseError {
  if (typeof value !== "string") return "not_delete_grant";
  const json = b64uDecode(value);
  if (json === null) return "bad_delete_grant_encoding";
  let parsed: unknown;
  try {
    parsed = JSON.parse(json);
  } catch {
    return "bad_delete_grant_encoding";
  }
  if (typeof parsed !== "object" || parsed === null) return "bad_delete_grant_shape";
  const record = parsed as Record<string, unknown>;
  if (record.type !== DELETE_GRANT_RECORD) return "not_delete_grant";
  if (
    !validBinding(record.message)
    || !validBinding(record.owner)
    || !validBinding(record.scope)
    || !validCap(record.grant)
  ) {
    return "bad_delete_grant_shape";
  }
  return {
    type: DELETE_GRANT_RECORD,
    message: record.message,
    owner: record.owner,
    scope: record.scope,
    grant: record.grant,
  };
}

export function countUsableSenderDeleteGrants(value: unknown): number {
  let count = 0;
  const visit = (candidate: unknown) => {
    if (typeof candidate === "string") {
      if (typeof parseDeleteGrant(candidate) !== "string") count += 1;
      return;
    }
    if (Array.isArray(candidate)) {
      for (const item of candidate) visit(item);
      return;
    }
    if (typeof candidate === "object" && candidate !== null) {
      const record = candidate as Record<string, unknown>;
      if (
        record.type === DELETE_GRANT_RECORD
        && validBinding(record.message)
        && validBinding(record.owner)
        && validBinding(record.scope)
        && validCap(record.grant)
      ) {
        count += 1;
      }
      for (const item of Object.values(record)) visit(item);
    }
  };
  visit(value);
  return count;
  readKeys: [MessageReadKeyRecord];
  senderDeleteGrants: [DeleteGrantRecord];
  readKeys: [MessageReadKeyRecord];
  senderDeleteGrants: [DeleteGrantRecord];
  recipientDeleteGrants: [DeleteGrantRecord];
  readKeys: [MessageReadKeyRecord];
  senderDeleteGrants: [DeleteGrantRecord];
}

export interface RecipientProtectedMessageData {
  message: string;
  readKeys: [MessageReadKeyRecord];
  nonce: string;
  ciphertext: string;
}

export interface RecipientProtectedMessageDataInput {
  message: string;
  readKey: string;
  plaintext: string;
  readKeys: [MessageReadKeyRecord];
  senderDeleteGrants: [DeleteGrantRecord];
  recipientDeleteGrants: [DeleteGrantRecord];
}

export type DeleteGrantParseResult =
  | { ok: true; grant: DeleteGrantRecord }
  | { ok: false; code: "not_delete_grant" | "malformed_delete_grant" };

type DeleteGrantParseFailureCode = Extract<DeleteGrantParseResult, { ok: false }>["code"];

export type MessageReadKeyParseResult =
  | { ok: true; key: MessageReadKeyRecord }
  | { ok: false; code: "not_message_read_key" | "malformed_message_read_key" };

export type StoredProtectedMessageGrantCreationResult =
  | { ok: true; grants: StoredProtectedMessageGrants }
  | {
    ok: false;
    code: "malformed_message_read_key" | "malformed_delete_grant";
  };

export interface DeleteGrantValidationInput {
  grant: string | unknown;
  message: string;
  owner: string;
  burnScope: string;
  allowedBurnScope: string;
}

export type DeleteGrantValidationResult =
  | { ok: true; grant: DeleteGrantRecord }
  | {
    ok: false;
    code:
      | DeleteGrantParseFailureCode
      | "malformed_delete_request"
      | "delete_grant_message_mismatch"
      | "delete_grant_owner_mismatch"
      | "delete_grant_scope_mismatch"
      | "delete_grant_scope_not_allowed";
export type RecipientProtectedMessageDataCreationResult =
  | { ok: true; data: RecipientProtectedMessageData }
  | { ok: false; code: "malformed_message_read_key" };

export type RecipientProtectedMessageDecryptResult =
  | { ok: true; plaintext: string }
  | {
    ok: false;
    code: "malformed_message_data" | "missing_message_read_key" | "decrypt_failed";
  };

function parseRecord(input: string | unknown): Record<string, unknown> | null {
  const value = typeof input === "string" ? JSON.parse(input) as unknown : input;
  if (value === null || typeof value !== "object" || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
}

function hasExactly(record: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(record).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length
    && actual.every((key, index) => key === expected[index]);
}

function validName(value: unknown): value is string {
  return typeof value === "string" && NAME_RE.test(value);
}

function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function bytesFromHex(value: string): Uint8Array {
  return Uint8Array.from(value.match(/../g) ?? [], (pair) => Number.parseInt(pair, 16));
}

async function aesKey(readKey: string): Promise<CryptoKey> {
  return crypto.subtle.importKey("raw", bytesFromHex(readKey), { name: "AES-GCM" }, false, [
    "decrypt",
    "encrypt",
  ]);
}

export function parseDeleteGrantRecord(input: string | unknown): DeleteGrantParseResult {
  let record: Record<string, unknown> | null;
  try {
    record = parseRecord(input);
  } catch {
    return { ok: false, code: "malformed_delete_grant" };
  }
  if (record === null) return { ok: false, code: "malformed_delete_grant" };
  if (record.record !== DELETE_GRANT_RECORD) return { ok: false, code: "not_delete_grant" };
  if (!hasExactly(record, ["record", "message", "owner", "scope"])) {
    return { ok: false, code: "malformed_delete_grant" };
  }
  if (!validName(record.message) || !validName(record.owner) || !validName(record.scope)) {
  if (!hasExactly(record, ["record", "owner", "scope"])) {
    return { ok: false, code: "malformed_delete_grant" };
  }
  if (!validName(record.owner) || !validName(record.scope)) {
    return { ok: false, code: "malformed_delete_grant" };
  }
  return {
    ok: true,
    grant: {
      record: DELETE_GRANT_RECORD,
      message: record.message,
      owner: record.owner,
      scope: record.scope,
    },
  };
}

export function parseMessageReadKeyRecord(input: string | unknown): MessageReadKeyParseResult {
  let record: Record<string, unknown> | null;
  try {
    record = parseRecord(input);
  } catch {
    return { ok: false, code: "malformed_message_read_key" };
  }
  if (record === null) return { ok: false, code: "malformed_message_read_key" };
  if (record.record !== MESSAGE_READ_KEY_RECORD) {
    return { ok: false, code: "not_message_read_key" };
  }
  if (!hasExactly(record, ["record", "message", "readKey"])) {
    return { ok: false, code: "malformed_message_read_key" };
  }
  if (!validName(record.message)) {
    return { ok: false, code: "malformed_message_read_key" };
  }
  if (typeof record.readKey !== "string" || !READ_KEY_RE.test(record.readKey)) {
    return { ok: false, code: "malformed_message_read_key" };
  }
  return {
    ok: true,
    key: {
      record: MESSAGE_READ_KEY_RECORD,
      message: record.message,
      readKey: record.readKey,
    },
  };
}

function deleteGrant(message: string, owner: string, scope: string): DeleteGrantParseResult {
  return parseDeleteGrantRecord({
    record: DELETE_GRANT_RECORD,
    message,
    owner,
    scope,
  });
}

export function createStoredProtectedMessageGrants(
  input: StoredProtectedMessageGrantInput,
): StoredProtectedMessageGrantCreationResult {
  const readKey: MessageReadKeyRecord = {
    record: MESSAGE_READ_KEY_RECORD,
    message: input.message,
    readKey: input.readKey,
  };
  const parsedReadKey = parseMessageReadKeyRecord(readKey);
  if (!parsedReadKey.ok) return { ok: false, code: "malformed_message_read_key" };

  const senderDeleteGrant: DeleteGrantRecord = {
    record: DELETE_GRANT_RECORD,
    message: input.message,
    owner: input.sender,
    scope: input.scope,
  };
  const parsedDeleteGrant = parseDeleteGrantRecord(senderDeleteGrant);
  if (!parsedDeleteGrant.ok) return { ok: false, code: "malformed_delete_grant" };
  const parsedSenderDeleteGrant = deleteGrant(input.message, input.sender, input.scope);
  if (!parsedSenderDeleteGrant.ok) return { ok: false, code: "malformed_delete_grant" };

  const parsedRecipientDeleteGrant = deleteGrant(
    input.message,
    input.recipient ?? input.sender,
    input.scope,
  );
  if (!parsedRecipientDeleteGrant.ok) return { ok: false, code: "malformed_delete_grant" };

  return {
    ok: true,
    grants: {
      message: input.message,
      readKeys: [parsedReadKey.key],
      senderDeleteGrants: [parsedDeleteGrant.grant],
      senderDeleteGrants: [parsedSenderDeleteGrant.grant],
      recipientDeleteGrants: [parsedRecipientDeleteGrant.grant],
    },
  };
}

export function validateDeleteGrant(
  input: DeleteGrantValidationInput,
): DeleteGrantValidationResult {
  const parsed = parseDeleteGrantRecord(input.grant);
  if (!parsed.ok) return parsed;

  if (
    !validName(input.message)
    || !validName(input.owner)
    || !validName(input.burnScope)
    || !validName(input.allowedBurnScope)
  ) {
    return { ok: false, code: "malformed_delete_request" };
  }
  if (input.burnScope !== input.allowedBurnScope) {
    return { ok: false, code: "delete_grant_scope_not_allowed" };
  }
  if (parsed.grant.message !== input.message) {
    return { ok: false, code: "delete_grant_message_mismatch" };
  }
  if (parsed.grant.owner !== input.owner) {
    return { ok: false, code: "delete_grant_owner_mismatch" };
  }
  if (parsed.grant.scope !== input.burnScope) {
    return { ok: false, code: "delete_grant_scope_mismatch" };
  }

  return parsed;
export async function createRecipientProtectedMessageData(
  input: RecipientProtectedMessageDataInput,
): Promise<RecipientProtectedMessageDataCreationResult> {
  const readKey = parseMessageReadKeyRecord({
    record: MESSAGE_READ_KEY_RECORD,
    message: input.message,
    readKey: input.readKey,
  });
  if (!readKey.ok) return { ok: false, code: "malformed_message_read_key" };

  const nonce = crypto.getRandomValues(new Uint8Array(12));
  const ciphertext = new Uint8Array(
    await crypto.subtle.encrypt(
      { name: "AES-GCM", iv: nonce },
      await aesKey(readKey.key.readKey),
      new TextEncoder().encode(input.plaintext),
    ),
  );
  return {
    ok: true,
    data: {
      message: input.message,
      readKeys: [readKey.key],
      nonce: hex(nonce),
      ciphertext: hex(ciphertext),
    },
  };
}

export async function decryptRecipientProtectedMessageData(
  data: RecipientProtectedMessageData,
): Promise<RecipientProtectedMessageDecryptResult> {
  if (
    !validName(data.message)
    || !Array.isArray(data.readKeys)
    || data.readKeys.length !== 1
    || typeof data.nonce !== "string"
    || !NONCE_RE.test(data.nonce)
    || typeof data.ciphertext !== "string"
    || data.ciphertext.length === 0
    || data.ciphertext.length % 2 !== 0
    || !CIPHERTEXT_RE.test(data.ciphertext)
  ) {
    return { ok: false, code: "malformed_message_data" };
  }
  const readKey = parseMessageReadKeyRecord(data.readKeys[0]);
  if (!readKey.ok || readKey.key.message !== data.message) {
    return { ok: false, code: "missing_message_read_key" };
  }
  try {
    const plaintext = await crypto.subtle.decrypt(
      { name: "AES-GCM", iv: bytesFromHex(data.nonce) },
      await aesKey(readKey.key.readKey),
      bytesFromHex(data.ciphertext),
    );
    return { ok: true, plaintext: new TextDecoder().decode(plaintext) };
  } catch {
    return { ok: false, code: "decrypt_failed" };
  }
}

export function usableSenderDeleteGrantCount(
  data: unknown,
  sender: string,
  scope: string,
): number {
  const seen = new Set<unknown>();
  const stack: unknown[] = [data];
  let count = 0;
  while (stack.length > 0) {
    const value = stack.pop();
    if (value === null || value === undefined || seen.has(value)) continue;
    if (typeof value === "object") seen.add(value);

    const parsed = parseDeleteGrantRecord(value);
    if (parsed.ok && parsed.grant.owner === sender && parsed.grant.scope === scope) {
      count += 1;
    }

    if (Array.isArray(value)) {
      stack.push(...value);
    } else if (typeof value === "object") {
      stack.push(...Object.values(value as Record<string, unknown>));
    }
  }
  return count;
}
