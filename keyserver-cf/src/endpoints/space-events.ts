//! Opaque Space membership-event transport (T21-C5).
//!
//! Recipient routing is exclusively by a rotating delivery tag.  Do not add
//! identity, Space, roster, or event-kind columns: every such field lets the
//! relay enumerate a Space rather than merely relay ciphertext.
//!
//! D-273 — delivery follows owner decision D15, "delete on ACKNOWLEDGED
//! receipt, never on transmission", the same rule this worker already holds
//! the wrapped-key lane to (`lib/db.ts` `fetchWrappedKeyAuthenticated`, and
//! `test/integration/wrapped-key-reservation.test.ts`).  The drain used to
//! DELETE every row it returned, before the response was serialized, so a
//! dropped response destroyed the only copy of a membership event and no retry
//! could recover it.  It now *reserves* what it returns and an explicit
//! acknowledgement destroys it:
//!
//!   GET  /v1/space-events/:tag   returns ≤64 events and leases them for
//!                                LEASE_SECONDS -- they stop being returned,
//!                                so the frozen T21-C1 wording "returns and
//!                                consumes" still holds, but nothing is
//!                                destroyed on transmission.
//!   POST /v1/space-events/ack    the caller names the event ids it actually
//!                                received; only those rows are deleted.
//!
//! WHICH WAY THIS ERRS, stated plainly: **toward duplicate delivery.**  An
//! unacknowledged event becomes visible again when its lease expires and is
//! delivered again, for as long as its `expires_at` allows.  A duplicated
//! membership event is a client-side dedupe problem; a lost one is
//! unrecoverable.  The cost is a redelivery latency of at most LEASE_SECONDS
//! after a dropped response, and duplicates for a caller that never
//! acknowledges.
//!
//! The retention floor is the other half (D-274): `expires_at` was a read
//! filter only, so nothing ever left this table.  `lib/space-event-sweep.ts`
//! now deletes expired rows on the hourly cron, and MAX_EVENT_LIFETIME_SECONDS
//! bounds how far ahead a sender may push `expires_at` -- a sweep over an
//! unbounded expiry is not a retention policy.

import type { Env } from "../env.js";
import { badRequest, json, serverError } from "../lib/http.js";

const TAG_BYTES = 32;
const MAX_CIPHERTEXT_BYTES = 64 * 1024;
const MAX_DRAIN = 64;
const EVENT_ID_BYTES = 16;
/** Hex, matching the control-inbox lane's `inbox_id_hex` convention. */
const EVENT_ID_HEX_CHARS = EVENT_ID_BYTES * 2;
/**
 * How long a drained event stops being returned before it is offered again.
 * Short on purpose: it is the redelivery delay a caller pays when its response
 * is dropped, and the wrapped-key lane's lesson is that the recoverable side
 * of the trade should be the cheap one.
 */
export const LEASE_SECONDS = 60;
/**
 * The same 7-day ceiling the wrapped-key lane puts on its own retention
 * (`MAX_WRAPPED_KEY_LIFETIME_MS`).  Without it `expires_at` is any safe
 * integer, so one sender could pin ciphertext in the relay effectively
 * forever and the D-274 sweep would never reach it.
 */
export const MAX_EVENT_LIFETIME_SECONDS = 7 * 24 * 60 * 60;

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

function toHex(value: Uint8Array): string {
  return Array.from(value, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function fromHex(value: unknown): Uint8Array | null {
  if (typeof value !== "string" || value.length !== EVENT_ID_HEX_CHARS) return null;
  if (!/^[0-9a-f]+$/.test(value)) return null;
  const bytes = new Uint8Array(EVENT_ID_BYTES);
  for (let i = 0; i < EVENT_ID_BYTES; i += 1) {
    bytes[i] = parseInt(value.slice(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}

function id(): Uint8Array {
  const value = new Uint8Array(EVENT_ID_BYTES);
  crypto.getRandomValues(value);
  return value;
}

export async function handleSpaceEventPost(request: Request, env: Env): Promise<Response> {
  let body: Record<string, unknown>;
  try { body = await request.json() as Record<string, unknown>; } catch { return badRequest("invalid JSON"); }
  const recipientTag = decode(body.recipient_tag);
  const ciphertext = decode(body.ciphertext);
  const expiresAt = body.expires_at;
  const now = Date.now() / 1000;
  // `typeof expiresAt !== "number"` is the narrowing half of the check
  // `Number.isSafeInteger` already performed at runtime -- that predicate is
  // typed `(value: unknown) => boolean`, not a type guard, so it rejected a
  // string at runtime while leaving `expiresAt` `unknown` to the compiler and
  // the `<=` comparison below unchecked. Both clauses are kept: neither is
  // redundant to the reader, and the accept/reject set is unchanged.
  //
  // D-274: the upper bound is new. `expires_at` is what the retention sweep
  // keys on, so an unbounded `expires_at` is an unbounded retention. It joins
  // the SAME indistinguishable rejection below -- a caller still cannot tell a
  // bad tag length from a bad ciphertext length from an out-of-range expiry.
  if (!recipientTag || recipientTag.length !== TAG_BYTES || !ciphertext || ciphertext.length === 0 || ciphertext.length > MAX_CIPHERTEXT_BYTES || typeof expiresAt !== "number" || !Number.isSafeInteger(expiresAt) || expiresAt <= now || expiresAt > now + MAX_EVENT_LIFETIME_SECONDS) {
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
    const rows = await env.DB.prepare("SELECT id, ciphertext FROM space_event_queue WHERE recipient_tag = ? AND expires_at > ? AND lease_until <= ? ORDER BY created_at LIMIT ?")
      .bind(recipientTag, now, now, MAX_DRAIN).all<{ id: Uint8Array; ciphertext: Uint8Array }>();
    const results = rows.results ?? [];
    // D-273: RESERVE, never destroy. The lease is what makes this drain
    // "consume" (T21-C1) -- these rows stop being returned -- while leaving the
    // only copy in place until the recipient says it has it. A failure here
    // fails the whole request: the rows stay visible and the caller retries,
    // which is the duplicate-delivery side of the trade, not the lossy one.
    if (results.length > 0) {
      await env.DB.batch(results.map((row) =>
        env.DB.prepare("UPDATE space_event_queue SET lease_until = ? WHERE id = ? AND recipient_tag = ?")
          .bind(now + LEASE_SECONDS, row.id, recipientTag)));
    }
    return json({ events: results.map((row) => ({ event_id: toHex(row.id), ciphertext: encode(row.ciphertext) })) });
  } catch { return serverError("could not drain Space events"); }
}

/**
 * POST /v1/space-events/ack — the acknowledgement half of D15.
 *
 * A NEW route. `03-CONTRACTS/spaces.md` T21-C1 is frozen and describes only
 * `POST /v1/space-events` and the `GET` drain; neither is changed here, and the
 * drain's method is untouched. The tag rides in the BODY, not the path, for the
 * reason D81 moved the username lookup off the path: a capability in a request
 * path is written into every intermediary's default log.
 *
 * Capability is unchanged from the rest of the lane: possession of the tag.
 * A caller that can acknowledge an event could already have drained it.
 */
export async function handleSpaceEventAck(request: Request, env: Env): Promise<Response> {
  let body: Record<string, unknown>;
  try { body = await request.json() as Record<string, unknown>; } catch { return badRequest("invalid JSON"); }
  const recipientTag = decode(body.recipient_tag);
  const eventIds = body.event_ids;
  if (!recipientTag || recipientTag.length !== TAG_BYTES || !Array.isArray(eventIds) || eventIds.length === 0 || eventIds.length > MAX_DRAIN) {
    return badRequest("invalid Space event acknowledgement");
  }
  const ids: Uint8Array[] = [];
  for (const value of eventIds) {
    const decoded = fromHex(value);
    // One indistinguishable rejection, as on the POST: a per-field message
    // would tell a caller which half of its request the relay disliked.
    if (!decoded) return badRequest("invalid Space event acknowledgement");
    ids.push(decoded);
  }
  try {
    // Scoped to the tag, exactly as the control-inbox delete is scoped to its
    // recipient: a leaked event id alone must not let anyone destroy the row.
    // `lease_until > 0` is the second half -- an event that was never delivered
    // cannot be acknowledged, so no ack can destroy something unseen.
    await env.DB.batch(ids.map((eventId) =>
      env.DB.prepare("DELETE FROM space_event_queue WHERE id = ? AND recipient_tag = ? AND lease_until > 0")
        .bind(eventId, recipientTag)));
    // Deliberately a constant. Reporting how many rows matched would turn this
    // route into the existence oracle the drain is careful not to be.
    return json({ acknowledged: true });
  } catch { return serverError("could not acknowledge Space events"); }
}
