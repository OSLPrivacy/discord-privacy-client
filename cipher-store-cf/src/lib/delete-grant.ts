export const DELETE_GRANT_RECORD = "osl/delete-grant/v1";
export const MESSAGE_READ_KEY_RECORD = "osl/message-read-key/v1";

const NAME_RE = /^[A-Za-z0-9._:@/-]{1,256}$/;
const READ_KEY_RE = /^[0-9a-f]{32}$/;

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
