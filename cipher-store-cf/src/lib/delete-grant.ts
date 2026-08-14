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
}
