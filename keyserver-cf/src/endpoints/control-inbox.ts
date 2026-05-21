/// Phase 6.4: SKDM inbox endpoints.
///
///   POST   /v1/skdm/inbox          enqueue an SKDM bundle for a recipient
///   GET    /v1/skdm/inbox/:user_id drain the user's own inbox (FIFO)
///   DELETE /v1/skdm/inbox/:id      delete a specific row after apply
///
/// All three are authorized by an ed25519 signature over canonical
/// bytes (see ../lib/canonical.ts). Signing key is the requester's
/// `ik_ed25519_pub` row in the existing `users` table -- one less
/// auth surface to maintain.
///
/// The bundle blob is opaque to this server; it's the same v=3
/// multi-recipient PQ-hybrid wire encrypt_v5_send already produces.
///
/// TTL: CONTROL_INBOX_TTL_SECONDS (7 days). The existing keyserver
/// cron sweep deletes rows where expires_at < now (see ./sweep
/// caller; we just rely on the standard sweep job picking up the
/// expires_at index).

import type { Env } from "../env.js";
import {
  canonicalControlInboxDeleteBytes,
  canonicalControlInboxGetBytes,
  canonicalControlInboxPostBytes,
  CONTROL_INBOX_FRESHNESS_WINDOW_MS,
} from "../lib/canonical.js";
import { verifyEd25519 } from "../lib/crypto.js";
import { getUserForVerify } from "../lib/db.js";
import {
  badRequest,
  error,
  json,
  notFound,
  tooMany,
  unauthorized,
} from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import {
  decodeBase64,
  isNonEmptyBase64,
  isPlainString,
} from "../lib/validation.js";

const CONTROL_INBOX_TTL_SECONDS = 7 * 24 * 60 * 60;
const MAX_BUNDLE_BYTES = 16 * 1024;
const MAX_DRAIN_ROWS = 64;
const INBOX_ID_BYTES = 16;
const INBOX_ID_HEX_LEN = INBOX_ID_BYTES * 2;
const INBOX_ID_HEX_RE = /^[0-9a-f]{32}$/;

function genInboxId(): Uint8Array {
  const buf = new Uint8Array(INBOX_ID_BYTES);
  crypto.getRandomValues(buf);
  return buf;
}

function idToHex(id: Uint8Array): string {
  let hex = "";
  for (const b of id) hex += b.toString(16).padStart(2, "0");
  return hex;
}

function hexToId(hex: string): Uint8Array | null {
  if (hex.length !== INBOX_ID_HEX_LEN) return null;
  if (!INBOX_ID_HEX_RE.test(hex)) return null;
  const out = new Uint8Array(INBOX_ID_BYTES);
  for (let i = 0; i < INBOX_ID_BYTES; i++) {
    out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

function freshnessOk(ts: unknown): ts is number {
  if (typeof ts !== "number" || !Number.isFinite(ts) || ts <= 0) return false;
  return Math.abs(Date.now() - ts) <= CONTROL_INBOX_FRESHNESS_WINDOW_MS;
}

/// Decode base64 WITHOUT throwing. `decodeBase64` calls `atob`, which
/// throws `InvalidCharacterError` on a malformed / null / undefined
/// value; an unguarded call turned a missing-or-bad signing key into
/// an opaque HTTP 500 ("internal error") that broke the whole drain
/// loop. Returns null on any decode failure so callers can answer
/// with a clean 401/400 instead.
function safeDecodeBase64(value: unknown): Uint8Array | null {
  if (typeof value !== "string" || value.length === 0) return null;
  try {
    return decodeBase64(value);
  } catch {
    return null;
  }
}

function bytesToU8(v: unknown): Uint8Array | null {
  if (v == null) return null;
  if (v instanceof Uint8Array) return v;
  if (v instanceof ArrayBuffer) return new Uint8Array(v);
  if (ArrayBuffer.isView(v)) {
    const view = v as ArrayBufferView;
    return new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
  }
  if (typeof v === "string") {
    try {
      const bin = atob(v);
      const arr = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) arr[i] = bin.charCodeAt(i);
      return arr;
    } catch {
      return null;
    }
  }
  return null;
}

async function sha256(bytes: Uint8Array): Promise<Uint8Array> {
  const hash = await crypto.subtle.digest("SHA-256", bytes);
  return new Uint8Array(hash);
}

export async function handleControlInboxPost(
  request: Request,
  env: Env,
): Promise<Response> {
  // Generous-ish: peers can post a lot of SKDMs in a busy session
  // (SKDM fan-out posts once per recipient). Own bucket so the
  // drain loop's GET/DELETE traffic can't starve sends.
  const rl = await checkRateLimit(env, callerIp(request), 1200, "ci-post");
  if (!rl.ok) return tooMany(rl.retryAfter);

  let body: Record<string, unknown>;
  try {
    body = (await request.json()) as Record<string, unknown>;
  } catch {
    return badRequest("malformed JSON body");
  }

  if (!isPlainString(body.sender_id)) return badRequest("sender_id required");
  if (!isPlainString(body.recipient_id))
    return badRequest("recipient_id required");
  if (!isPlainString(body.scope_id)) return badRequest("scope_id required");
  if (!isNonEmptyBase64(body.bundle_b64))
    return badRequest("bundle_b64 required (non-empty base64)");
  if (!isNonEmptyBase64(body.signature_b64))
    return badRequest("signature_b64 required");
  if (!freshnessOk(body.timestamp_ms))
    return badRequest(
      `timestamp_ms required (positive number within ${CONTROL_INBOX_FRESHNESS_WINDOW_MS}ms of server clock)`,
    );

  const bundle = decodeBase64(body.bundle_b64);
  if (bundle.length === 0) return badRequest("bundle is empty");
  if (bundle.length > MAX_BUNDLE_BYTES)
    return badRequest(`bundle exceeds ${MAX_BUNDLE_BYTES} bytes`);

  const sender = await getUserForVerify(env.DB, body.sender_id);
  if (!sender) return notFound();

  const bundleHash = await sha256(bundle);
  const message = canonicalControlInboxPostBytes({
    sender_id: body.sender_id,
    recipient_id: body.recipient_id,
    scope_id: body.scope_id,
    timestamp_ms: body.timestamp_ms,
    bundle_sha256: bundleHash,
  });
  const pubBytes = safeDecodeBase64(sender.ik_ed25519_pub);
  const sigBytes = safeDecodeBase64(body.signature_b64);
  if (!pubBytes) {
    return unauthorized("no usable ed25519 signing key on file for sender");
  }
  if (!sigBytes) return badRequest("signature is not valid base64");
  const ok = await verifyEd25519(pubBytes, message, sigBytes);
  if (!ok) return unauthorized("signature verification failed");

  // Insert. Retry on the (vanishingly unlikely) primary-key
  // collision; 128-bit random id space means it's basically never.
  const now = Math.floor(Date.now() / 1000);
  const expiresAt = now + CONTROL_INBOX_TTL_SECONDS;
  for (let attempt = 0; attempt < 5; attempt++) {
    const id = genInboxId();
    try {
      await env.DB.prepare(
        "INSERT INTO control_inbox (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
      )
        .bind(
          id,
          body.recipient_id,
          body.sender_id,
          body.scope_id,
          bundle,
          expiresAt,
          now,
        )
        .run();
      return json(
        { id: idToHex(id), expires_at: expiresAt },
        { status: 201 },
      );
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.includes("UNIQUE") || msg.includes("PRIMARY")) continue;
      throw err;
    }
  }
  return badRequest("could not allocate a fresh id (retry)");
}

export async function handleControlInboxGet(
  request: Request,
  env: Env,
  userId: string,
): Promise<Response> {
  try {
    return await handleControlInboxGetInner(request, env, userId);
  } catch (err) {
    // Surface the real cause instead of the dispatch-level opaque
    // "internal error" so a failing drain is diagnosable from the
    // client console.
    const msg = err instanceof Error ? err.message : String(err);
    console.error("[control-inbox GET] error:", msg);
    return error(500, `control-inbox GET: ${msg}`);
  }
}

async function handleControlInboxGetInner(
  request: Request,
  env: Env,
  userId: string,
): Promise<Response> {
  // Drain GETs poll every ~10s; own bucket keeps them off the POST
  // counter so a chatty server can't rate-limit its own sends.
  const rl = await checkRateLimit(env, callerIp(request), 3600, "ci-get");
  if (!rl.ok) return tooMany(rl.retryAfter);

  const url = new URL(request.url);
  const ts = parseInt(url.searchParams.get("ts") || "", 10);
  const sigB64 = url.searchParams.get("sig") || "";
  if (!freshnessOk(ts)) return badRequest("ts required (?ts= within window)");
  if (!sigB64) return badRequest("sig required (?sig=)");
  if (!isPlainString(userId)) return badRequest("user_id required");

  const user = await getUserForVerify(env.DB, userId);
  if (!user) return notFound();

  const message = canonicalControlInboxGetBytes({
    user_id: userId,
    timestamp_ms: ts,
  });
  const pubBytes = safeDecodeBase64(user.ik_ed25519_pub);
  const sigBytes = safeDecodeBase64(sigB64);
  if (!pubBytes) {
    return unauthorized("no usable ed25519 signing key on file for this user");
  }
  if (!sigBytes) return badRequest("signature is not valid base64");
  const ok = await verifyEd25519(pubBytes, message, sigBytes);
  if (!ok) return unauthorized("signature verification failed");

  // Drain in FIFO order. The recipient's poll loop calls DELETE
  // per-row after apply; we don't auto-delete on read so a crash
  // between GET response and apply doesn't lose the SKDM.
  const now = Math.floor(Date.now() / 1000);
  const rows = await env.DB.prepare(
    "SELECT id, sender_id, scope_id, bundle, created_at FROM control_inbox " +
      "WHERE recipient_id = ? AND expires_at >= ? " +
      "ORDER BY created_at ASC LIMIT ?",
  )
    .bind(userId, now, MAX_DRAIN_ROWS)
    .all<{
      id: unknown;
      sender_id: string;
      scope_id: string;
      bundle: unknown;
      created_at: number;
    }>();

  const items = (rows.results || []).map((r) => {
    const idBytes = bytesToU8(r.id) ?? new Uint8Array(0);
    const bundleBytes = bytesToU8(r.bundle) ?? new Uint8Array(0);
    let bundleB64 = "";
    try {
      let bin = "";
      for (const b of bundleBytes) bin += String.fromCharCode(b);
      bundleB64 = btoa(bin);
    } catch {
      bundleB64 = "";
    }
    return {
      id: idToHex(idBytes),
      sender_id: r.sender_id,
      scope_id: r.scope_id,
      bundle_b64: bundleB64,
      created_at: r.created_at,
    };
  });

  return json({ items });
}

export async function handleControlInboxDelete(
  request: Request,
  env: Env,
  inboxIdHex: string,
): Promise<Response> {
  // Drain deletes up to MAX_DRAIN_ROWS items per cycle; own bucket.
  const rl = await checkRateLimit(env, callerIp(request), 3600, "ci-del");
  if (!rl.ok) return tooMany(rl.retryAfter);

  let body: Record<string, unknown>;
  try {
    body = (await request.json()) as Record<string, unknown>;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (!isPlainString(body.user_id)) return badRequest("user_id required");
  if (!isNonEmptyBase64(body.signature_b64))
    return badRequest("signature_b64 required");
  if (!freshnessOk(body.timestamp_ms))
    return badRequest("timestamp_ms required (within freshness window)");

  const id = hexToId(inboxIdHex);
  if (!id) return badRequest("inbox id must be 32 hex chars");

  const user = await getUserForVerify(env.DB, body.user_id);
  if (!user) return notFound();

  const message = canonicalControlInboxDeleteBytes({
    user_id: body.user_id,
    inbox_id_hex: inboxIdHex,
    timestamp_ms: body.timestamp_ms,
  });
  const pubBytes = safeDecodeBase64(user.ik_ed25519_pub);
  const sigBytes = safeDecodeBase64(body.signature_b64);
  if (!pubBytes) {
    return unauthorized("no usable ed25519 signing key on file for this user");
  }
  if (!sigBytes) return badRequest("signature is not valid base64");
  const ok = await verifyEd25519(pubBytes, message, sigBytes);
  if (!ok) return unauthorized("signature verification failed");

  // Scope the delete to rows owned by this recipient -- a leaked
  // inbox_id alone shouldn't let someone else nuke the row.
  await env.DB.prepare(
    "DELETE FROM control_inbox WHERE id = ? AND recipient_id = ?",
  )
    .bind(id, body.user_id)
    .run();

  return new Response(null, { status: 204 });
}
