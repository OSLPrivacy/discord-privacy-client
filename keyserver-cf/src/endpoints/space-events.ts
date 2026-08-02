//! Opaque Space membership-event transport (T21-C5).
//!
//! Recipient routing is exclusively by a rotating delivery tag.  Do not add
//! identity, Space, roster, or event-kind columns: every such field lets the
//! relay enumerate a Space rather than merely relay ciphertext.

import type { Env } from "../env.js";
import { badRequest, json, serverError } from "../lib/http.js";

const TAG_BYTES = 32;
const MAX_CIPHERTEXT_BYTES = 64 * 1024;
const MAX_DRAIN = 64;

function decode(value: unknown): Uint8Array | null {
  if (typeof value !== "string" || value.length === 0) return null;
  try {
    const binary = atob(value);
    return Uint8Array.from(binary, (char) => char.charCodeAt(0));
  } catch { return null; }
}

function encode(value: Uint8Array): string {
  return btoa(String.fromCharCode(...value));
}

function id(): Uint8Array {
  const value = new Uint8Array(16);
  crypto.getRandomValues(value);
  return value;
}

export async function handleSpaceEventPost(request: Request, env: Env): Promise<Response> {
  let body: Record<string, unknown>;
  try { body = await request.json() as Record<string, unknown>; } catch { return badRequest("invalid JSON"); }
  const recipientTag = decode(body.recipient_tag);
  const ciphertext = decode(body.ciphertext);
  const expiresAt = body.expires_at;
  if (!recipientTag || recipientTag.length !== TAG_BYTES || !ciphertext || ciphertext.length === 0 || ciphertext.length > MAX_CIPHERTEXT_BYTES || !Number.isSafeInteger(expiresAt) || expiresAt <= Date.now() / 1000) {
    return badRequest("invalid opaque Space event envelope");
  }
  try {
    await env.DB.prepare("INSERT INTO space_event_queue (id, recipient_tag, ciphertext, expires_at, created_at) VALUES (?, ?, ?, ?, ?)")
      .bind(id(), recipientTag, ciphertext, expiresAt, Math.floor(Date.now() / 1000)).run();
    return json({ accepted: true }, { status: 202 });
  } catch { return serverError("could not queue Space event"); }
}

export async function handleSpaceEventDrain(tagText: string, env: Env): Promise<Response> {
  const recipientTag = decode(tagText);
  if (!recipientTag || recipientTag.length !== TAG_BYTES) return badRequest("invalid recipient tag");
  try {
    const now = Math.floor(Date.now() / 1000);
    const rows = await env.DB.prepare("SELECT id, ciphertext FROM space_event_queue WHERE recipient_tag = ? AND expires_at > ? ORDER BY created_at LIMIT ?")
      .bind(recipientTag, now, MAX_DRAIN).all<{ id: Uint8Array; ciphertext: Uint8Array }>();
    const results = rows.results ?? [];
    for (const row of results) await env.DB.prepare("DELETE FROM space_event_queue WHERE id = ? AND recipient_tag = ?").bind(row.id, recipientTag).run();
    return json({ events: results.map((row) => ({ ciphertext: encode(row.ciphertext) })) });
  } catch { return serverError("could not drain Space events"); }
}
