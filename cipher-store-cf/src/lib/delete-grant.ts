export const DELETE_GRANT_RECORD = "osl/delete-grant/v1";
export const MESSAGE_READ_KEY_RECORD = "osl/message-read-key/v1";

const NAME_RE = /^[A-Za-z0-9._:@/-]{1,256}$/;
const READ_KEY_RE = /^[0-9a-f]{32}$/;
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
  scope: string;
}

export interface StoredProtectedMessageGrants {
  message: string;
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
}

export type DeleteGrantParseResult =
  | { ok: true; grant: DeleteGrantRecord }
  | { ok: false; code: "not_delete_grant" | "malformed_delete_grant" };

export type MessageReadKeyParseResult =
  | { ok: true; key: MessageReadKeyRecord }
  | { ok: false; code: "not_message_read_key" | "malformed_message_read_key" };

export type StoredProtectedMessageGrantCreationResult =
  | { ok: true; grants: StoredProtectedMessageGrants }
  | {
    ok: false;
    code: "malformed_message_read_key" | "malformed_delete_grant";
  };

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
    owner: input.sender,
    scope: input.scope,
  };
  const parsedDeleteGrant = parseDeleteGrantRecord(senderDeleteGrant);
  if (!parsedDeleteGrant.ok) return { ok: false, code: "malformed_delete_grant" };

  return {
    ok: true,
    grants: {
      message: input.message,
      readKeys: [parsedReadKey.key],
      senderDeleteGrants: [parsedDeleteGrant.grant],
    },
  };
}

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
