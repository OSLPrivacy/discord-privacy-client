//! One-use Space invite redemption.
//!
//! The invite row names only a creator delivery tag and a purpose. Redeeming it
//! queues one opaque admission request to that tag, then burns the invite row by
//! setting `consumed_at`. The relay never learns the requester or Space.

import type { Env } from "../env.js";
import { badRequest, conflict, gone, json, serverError } from "../lib/http.js";
import { decodeBase64 } from "../lib/validation.js";

const INVITE_ID_BYTES = 16;
const CREATOR_TAG_BYTES = 32;
const MAX_REQUEST_CIPHERTEXT_BYTES = 64 * 1024;

function toHex(value: Uint8Array): string {
  return Array.from(value, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function decodeBytes(value: unknown, length: number): Uint8Array | null {
  if (typeof value !== "string") return null;
  try {
    const decoded = decodeBase64(value);
    return decoded.length === length ? decoded : null;
  } catch {
    return null;
  }
}

function storedBlob(value: unknown, length: number): Uint8Array | null {
  let bytes: Uint8Array;
  if (value instanceof Uint8Array) bytes = value;
  else if (value instanceof ArrayBuffer) bytes = new Uint8Array(value);
  else if (
    Array.isArray(value) &&
    value.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255)
  ) {
    bytes = Uint8Array.from(value);
  }
  else return null;
  return bytes.length === length ? bytes : null;
}

function decodeRequestCiphertext(value: unknown): Uint8Array | null {
  if (typeof value !== "string") return null;
  try {
    const decoded = decodeBase64(value);
    return decoded.length > 0 && decoded.length <= MAX_REQUEST_CIPHERTEXT_BYTES
      ? decoded
      : null;
  } catch {
    return null;
  }
}

async function fingerprint(ciphertext: Uint8Array): Promise<string> {
  return toHex(new Uint8Array(await crypto.subtle.digest("SHA-256", ciphertext)));
}

export async function handleSpaceInviteRedeem(
  request: Request,
  env: Env,
): Promise<Response> {
  let body: Record<string, unknown>;
  try {
    body = (await request.json()) as Record<string, unknown>;
  } catch {
    return badRequest("invalid Space invite redemption");
  }

  const inviteId = decodeBytes(body.invite_id, INVITE_ID_BYTES);
  const requestCiphertext = decodeRequestCiphertext(body.request_ciphertext);
  if (!inviteId || !requestCiphertext) {
    return badRequest("invalid Space invite redemption");
  }

  const now = Math.floor(Date.now() / 1000);
  const row = await env.DB.prepare(
    `SELECT creator, intended_use, expires_at, consumed_at
       FROM one_use_invite_links
      WHERE invite_id = ?`,
  )
    .bind(inviteId)
    .first<{
      creator: Uint8Array;
      intended_use: string;
      expires_at: number;
      consumed_at: number | null;
    }>();
  if (!row || row.intended_use !== "space_admission") {
    return badRequest("invalid Space invite redemption");
  }
  if (row.consumed_at !== null) return conflict("invite already used");
  if (row.expires_at <= now) return gone("invite expired");

  const creator = storedBlob(row.creator, CREATOR_TAG_BYTES);
  if (!creator) {
    return serverError("stored Space invite is invalid");
  }

  const requestFingerprint = await fingerprint(requestCiphertext);
  const eventId = crypto.getRandomValues(new Uint8Array(16));
  try {
    const results = await env.DB.batch([
      env.DB.prepare(
        `UPDATE one_use_invite_links
            SET consumed_at = ?
          WHERE invite_id = ?
            AND consumed_at IS NULL
            AND expires_at > ?`,
      ).bind(now, inviteId, now),
      env.DB.prepare(
        `INSERT INTO space_event_queue
           (id, recipient_tag, ciphertext, expires_at, created_at, lease_until)
         SELECT ?, creator, ?, expires_at, ?, 0
           FROM one_use_invite_links
          WHERE invite_id = ? AND consumed_at = ?`,
      ).bind(eventId, requestCiphertext, now, inviteId, now),
    ]);
    if ((results[0]?.meta?.changes ?? 0) !== 1) {
      const changed = await env.DB.prepare(
        "SELECT expires_at, consumed_at FROM one_use_invite_links WHERE invite_id = ?",
      )
        .bind(inviteId)
        .first<{ expires_at: number; consumed_at: number | null }>();
      if (changed?.consumed_at !== null) return conflict("invite already used");
      if (changed && changed.expires_at <= now) return gone("invite expired");
      return badRequest("invalid Space invite redemption");
    }
    if ((results[1]?.meta?.changes ?? 0) !== 1) {
      return serverError("could not queue Space invite redemption");
    }
  } catch {
    return serverError("could not redeem Space invite");
  }

  return json({ accepted: true, request_fingerprint: requestFingerprint }, { status: 202 });
}
