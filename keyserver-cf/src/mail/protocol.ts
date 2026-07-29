import type { Env } from "../env.js";
import { getUserForVerify } from "../lib/db.js";
import { isHighEntropyRequestId, isNonEmptyBase64, isProtocolId } from "../lib/validation.js";
import { verifySignedRequest } from "../lib/signed-request.js";

const FRESHNESS_MS = 5 * 60 * 1000;

export interface AuthorizedMailRequest {
  body: Record<string, unknown>;
  userId: string;
  requestId: string;
  timestampMs: number;
  message: Uint8Array;
}

export function mailSignedMessage(operation: string, body: Record<string, unknown>): Uint8Array {
  const unsigned = { ...body };
  delete unsigned.signature_b64;
  return new TextEncoder().encode(
    `OSL-MAIL-${operation}-v1\n${canonicalJson(unsigned)}\n`,
  );
}

export async function authorizeMailRequest(
  env: Env,
  operation: string,
  body: Record<string, unknown>,
): Promise<AuthorizedMailRequest | null> {
  if (!isProtocolId(body.user_id) || !isHighEntropyRequestId(body.request_id)) return null;
  if (!isNonEmptyBase64(body.signature_b64)) return null;
  if (typeof body.timestamp_ms !== "number" || !Number.isSafeInteger(body.timestamp_ms)) return null;
  if (Math.abs(Date.now() - body.timestamp_ms) > FRESHNESS_MS) return null;
  const current = await getUserForVerify(env.DB, body.user_id);
  if (!current) return null;
  const message = mailSignedMessage(operation, body);
  if (!await verifySignedRequest(current.ik_ed25519_pub, message, body.signature_b64)) return null;
  return {
    body,
    userId: body.user_id,
    requestId: body.request_id,
    timestampMs: body.timestamp_ms,
    message,
  };
}

export async function requestDigest(message: Uint8Array): Promise<Uint8Array> {
  return new Uint8Array(await crypto.subtle.digest("SHA-256", message));
}

export function canonicalJson(value: unknown): string {
  if (value === null || typeof value === "boolean" || typeof value === "string") {
    return JSON.stringify(value);
  }
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value)) throw new Error("canonical JSON only accepts safe integers");
    return String(value);
  }
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (typeof value === "object") {
    const record = value as Record<string, unknown>;
    return `{${Object.keys(record).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(record[key])}`).join(",")}}`;
  }
  throw new Error("unsupported canonical JSON value");
}

export function base64Encode(bytes: Uint8Array): string {
  let out = "";
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    out += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return btoa(out);
}

export function base64Decode(value: string): Uint8Array {
  const raw = atob(value);
  const out = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
  return out;
}

export function randomRequestId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  return base64Encode(bytes).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}

