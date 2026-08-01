/// Blob endpoints: upload / fetch / delete.
///
/// Validation rules:
///   - Upload body: raw bytes (application/octet-stream).
///   - Upload size: 1..=65536 bytes (64 KB cap).
///   - TTL header `X-OSL-TTL-Seconds`: must be one of
///       3600 (1h), 86400 (24h), 259200 (72h), 604800 (7d).
///   - Fetch-token header `X-OSL-Fetch-Token`: 32 hex chars (16 bytes).
///     Required on upload and delete, with no exceptions. A stored
///     row whose `fetch_token` is NULL (only reachable for rows written
///     before migration 0002) is treated as absent by delete. This opaque
///     token is not an identity or sender-authentication credential, and it
///     is deliberately carried in a HEADER: the blob id travels in the URL
///     path and the platform records request paths by default.
///   - ID path param: 16 hex chars.

import type { Env } from "../env.js";
import { MAX_LIVE_BLOB_BYTES, MAX_LIVE_BLOB_ROWS } from "../lib/blob-limits.js";
import { error, json, notFound } from "../lib/http.js";
import { hexToId, idToHex, newBlobId } from "../lib/id.js";

export const MAX_BLOB_BYTES = 64 * 1024;
function parseAllowedTtlSeconds(raw: string | null): number | null {
  switch (raw) {
    case "3600": return 3600; // 1h
    case "86400": return 86400; // 24h
    case "259200": return 259200; // 72h
    case "604800": return 604800; // 7d
    default: return null;
  }
}

const FETCH_TOKEN_HEX_LEN = 32; // 16 bytes
const FETCH_TOKEN_HEX_RE = /^[0-9a-f]{32}$/;

function readFetchToken(request: Request): string | null {
  const raw = request.headers.get("x-osl-fetch-token");
  if (raw === null) return null;
  const lower = raw.trim().toLowerCase();
  if (lower.length !== FETCH_TOKEN_HEX_LEN) return null;
  if (!FETCH_TOKEN_HEX_RE.test(lower)) return null;
  return lower;
}

/// Constant-time string comparison. Same-length only -- caller has
/// already validated both sides are 32 hex chars.
function constantTimeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) {
    diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  return diff === 0;
}

export async function handleUpload(request: Request, env: Env): Promise<Response> {
  const declaredLength = request.headers.get("content-length");
  if (declaredLength !== null) {
    if (!/^\d+$/.test(declaredLength)) {
      return error(400, "bad_content_length", "Content-Length must be an unsigned integer");
    }
    const declaredBytes = Number(declaredLength);
    if (!Number.isSafeInteger(declaredBytes) || declaredBytes > MAX_BLOB_BYTES) {
      return error(413, "too_large", `blob exceeds ${MAX_BLOB_BYTES} bytes`);
    }
  }

  const ttlHeader = request.headers.get("x-osl-ttl-seconds");
  const ttl = parseAllowedTtlSeconds(ttlHeader);
  if (ttl === null) {
    return error(
      400,
      "bad_ttl",
      "X-OSL-TTL-Seconds must be 3600 (1h), 86400 (24h), 259200 (72h), or 604800 (7d)"
    );
  }

  // Phase 6: capability token required for every new upload. The
  // header value is opaque to the worker (HMAC-derived client-side);
  // the worker just persists + later constant-time-compares.
  const fetchToken = readFetchToken(request);
  if (fetchToken === null) {
    return error(
      400,
      "bad_fetch_token",
      "X-OSL-Fetch-Token must be 32 hex chars (16 bytes)"
    );
  }

  const body = await readBoundedBody(request, MAX_BLOB_BYTES);
  if (body.status === "too_large") {
    return error(413, "too_large", `blob exceeds ${MAX_BLOB_BYTES} bytes`);
  }
  const data = body.bytes;
  if (data.length === 0) {
    return error(400, "empty_body", "blob body required");
  }
  if (data.length > MAX_BLOB_BYTES) {
    return error(413, "too_large", `blob exceeds ${MAX_BLOB_BYTES} bytes`);
  }

  // Generate ID + insert. On primary-key collision (vanishingly
  // unlikely with 64-bit IDs but technically possible), retry up to
  // a small bound.
  const now = Math.floor(Date.now() / 1000);
  const expiresAt = now + ttl;
  for (let attempt = 0; attempt < 5; attempt++) {
    const id = newBlobId();
    try {
      // Aggregate backstop (audit HIGH-2). One SQLite write statement, so the
      // COUNT/SUM predicates cannot race another insert. A primary-key
      // collision still raises a constraint error and is handled below, so
      // zero affected rows here means exactly one thing: at capacity.
      const inserted = await env.DB.prepare(
        `INSERT INTO blobs (id, data, size_bytes, expires_at, created_at, fetch_token)
         SELECT ?, ?, ?, ?, ?, ?
          WHERE (SELECT COUNT(*) FROM blobs) < ?
            AND COALESCE((SELECT SUM(size_bytes) FROM blobs), 0) <= ? - ?`
      )
        .bind(
          id,
          data,
          data.length,
          expiresAt,
          now,
          fetchToken,
          MAX_LIVE_BLOB_ROWS,
          MAX_LIVE_BLOB_BYTES,
          data.length,
        )
        .run();
      if ((inserted.meta?.changes ?? 0) !== 1) {
        return error(503, "storage_capacity", "blob storage is temporarily at capacity");
      }
      return json({ id: idToHex(id), expires_at: expiresAt }, 201);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.includes("UNIQUE") || msg.includes("PRIMARY")) {
        // Collision — retry.
        continue;
      }
      throw err;
    }
  }
  return error(
    500,
    "id_collision_loop",
    "could not allocate a fresh ID after retries"
  );
}

/**
 * Read a request stream with a hard in-memory ceiling. Content-Length is only
 * an early rejection hint; this counter is authoritative for chunked bodies
 * and dishonest declarations.
 */
export async function readBoundedBody(
  request: Request,
  maxBytes: number,
): Promise<{ status: "ok"; bytes: Uint8Array } | { status: "too_large" }> {
  if (!request.body) return { status: "ok", bytes: new Uint8Array(0) };

  const reader = request.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    if (!value) continue;
    total += value.byteLength;
    if (total > maxBytes) {
      await reader.cancel("blob too large");
      return { status: "too_large" };
    }
    chunks.push(value);
  }

  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return { status: "ok", bytes };
}

export async function handleFetch(
  request: Request,
  env: Env,
  hex: string
): Promise<Response> {
  const id = hexToId(hex);
  // A malformed id must look exactly like an absent id. Otherwise the
  // 400/404 distinction turns this public route into an existence oracle.
  if (!id) return notFound();
  const row = await env.DB.prepare(
    "SELECT data, expires_at FROM blobs WHERE id = ? LIMIT 1"
  )
    .bind(id)
    .first<{ data: unknown; expires_at: number }>();
  if (!row) return notFound();
  const now = Math.floor(Date.now() / 1000);
  if (row.expires_at < now) {
    // Expired but the sweep hasn't run yet. Treat as gone.
    return notFound();
  }
  // D1's BLOB return type is officially ArrayBuffer but in practice
  // varies by runtime version (Uint8Array, plain string, or even an
  // object with byteLength). Normalise to bytes regardless.
  const bytes = blobToBytes(row.data);
  return new Response(bytes, {
    status: 200,
    headers: {
      "content-type": "application/octet-stream",
      "cache-control": "no-store",
      "content-length": String(bytes.byteLength),
    },
  });
}

function blobToBytes(v: unknown): Uint8Array {
  if (v == null) return new Uint8Array(0);
  if (v instanceof Uint8Array) return v;
  if (v instanceof ArrayBuffer) return new Uint8Array(v);
  if (ArrayBuffer.isView(v)) {
    const view = v as ArrayBufferView;
    return new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
  }
  if (Array.isArray(v)) {
    return new Uint8Array(v as number[]);
  }
  if (typeof v === "string") {
    // Some D1 surfaces return BLOB as base64. Try that first; fall
    // through to UTF-8 bytes if it doesn't decode cleanly.
    try {
      const bin = atob(v);
      const arr = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) arr[i] = bin.charCodeAt(i);
      return arr;
    } catch {
      return new TextEncoder().encode(v);
    }
  }
  return new Uint8Array(0);
}

export async function handleDelete(
  request: Request,
  env: Env,
  hex: string
): Promise<Response> {
  const id = hexToId(hex);
  if (!id) return error(400, "bad_id", "id must be 16 hex chars");
  // Phase 6: DELETE is gated the same way as FETCH. Without this an
  // caller with only blob_id cannot delete a current blob. The client
  // currently derives this token from public scope metadata, so this is
  // capability separation rather than strong conversation membership.
  //
  // D81: a NULL-capability legacy row is treated as absent here too. It used
  // to accept ANY delete, so knowing the path-logged id was enough to destroy
  // a stranger's undelivered ciphertext. See the matching note in
  // `handleFetch`.
  //
  // The answer is 204 with no deletion, not 404: this route already answers
  // 204 for an id that was never stored, and diverging would turn the refusal
  // into an existence oracle for legacy rows. Nothing is lost -- the row is
  // unreachable by every path and expires on its own clock.
  const row = await env.DB.prepare(
    "SELECT fetch_token FROM blobs WHERE id = ? LIMIT 1"
  )
    .bind(id)
    .first<{ fetch_token: string | null }>();
  if (row && row.fetch_token === null) {
    return new Response(null, { status: 204 });
  }
  if (row) {
    const presented = readFetchToken(request);
    if (presented === null) {
      return error(
        401,
        "fetch_token_required",
        "X-OSL-Fetch-Token header required to delete this blob"
      );
    }
    if (!constantTimeEqual(row.fetch_token!, presented)) {
      return error(403, "fetch_token_mismatch", "fetch token does not match");
    }
  }
  await env.DB.prepare("DELETE FROM blobs WHERE id = ?").bind(id).run();
  return new Response(null, { status: 204 });
}
