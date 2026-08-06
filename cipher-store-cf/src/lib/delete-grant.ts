export const DELETE_GRANT_RECORD = "osl/delete-grant/v1";
export const MESSAGE_READ_KEY_RECORD = "osl/message-read-key/v1";

const NAME_RE = /^[A-Za-z0-9._:@/-]{1,256}$/;
const READ_KEY_RE = /^[0-9a-f]{32}$/;

export interface DeleteGrantRecord {
  record: typeof DELETE_GRANT_RECORD;
  message: string;
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
  if (!hasExactly(record, ["record", "message", "owner", "scope"])) {
    return { ok: false, code: "malformed_delete_grant" };
  }
  if (!validName(record.message) || !validName(record.owner) || !validName(record.scope)) {
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
}
