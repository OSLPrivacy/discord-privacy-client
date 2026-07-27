/// R2-backed opaque attachment transport.
///
/// Bodies are already sealed by OSL. The Worker never receives identity,
/// filename, MIME, conversation, or plaintext metadata. Upload bodies are
/// deliberately buffered into bounded `Uint8Array`s before R2 because workerd
/// rejects unknown-length streams; fetch still streams. D1 stores only opaque
/// object state, expiry, part receipts, and a SHA-256 digest of the bearer
/// capability.

import type { Env } from "../env.js";
import {
  INCOMPLETE_SESSION_TTL_SECONDS,
  MAX_ATTACHMENT_PART_BYTES,
  MAX_ATTACHMENT_PARTS,
  MAX_DIRECT_ATTACHMENT_BYTES,
  MAX_INCOMPLETE_SESSION_BYTES,
  MAX_INCOMPLETE_SESSION_ROWS,
  MAX_LIVE_ATTACHMENT_BYTES,
  MAX_LIVE_ATTACHMENT_ROWS,
  MAX_SEALED_ATTACHMENT_BYTES,
} from "../lib/attachment-limits.js";
import { error, json, notFound } from "../lib/http.js";

export {
  MAX_ATTACHMENT_PART_BYTES,
  MAX_ATTACHMENT_PARTS,
  MAX_DIRECT_ATTACHMENT_BYTES,
  MAX_SEALED_ATTACHMENT_BYTES,
} from "../lib/attachment-limits.js";

const CAPABILITY_RE = /^[0-9a-f]{32}$/;
const ID_RE = /^[0-9a-f]{32}$/;

function parseAllowedTtlSeconds(raw: string | null): number | null {
  switch (raw) {
    case "3600": return 3600;
    case "86400": return 86400;
    case "259200": return 259200;
    case "604800": return 604800;
    default: return null;
  }
}

function readCapability(request: Request): string | null {
  const raw = request.headers.get("x-osl-fetch-token")?.trim().toLowerCase();
  return raw && CAPABILITY_RE.test(raw) ? raw : null;
}

function constantTimeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let difference = 0;
  for (let index = 0; index < a.length; index++) {
    difference |= a.charCodeAt(index) ^ b.charCodeAt(index);
  }
  return difference === 0;
}

async function capabilityDigestHex(capability: string): Promise<string> {
  const bytes = new Uint8Array(capability.length / 2);
  for (let index = 0; index < bytes.length; index++) {
    bytes[index] = Number.parseInt(capability.slice(index * 2, index * 2 + 2), 16);
  }
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  let output = "";
  for (const byte of digest) output += byte.toString(16).padStart(2, "0");
  return output;
}

function randomHex(bytes: number): string {
  const value = new Uint8Array(bytes);
  crypto.getRandomValues(value);
  let output = "";
  for (const byte of value) output += byte.toString(16).padStart(2, "0");
  return output;
}

function unsignedLength(raw: string | null, max: number): number | Response | null {
  if (raw === null) return null;
  if (!/^\d+$/.test(raw)) {
    return error(400, "bad_content_length", "length must be an unsigned integer");
  }
  const length = Number(raw);
  if (!Number.isSafeInteger(length) || length > max) {
    return error(413, "too_large", `attachment data exceeds ${max} bytes`);
  }
  if (length === 0) return error(400, "empty_body", "attachment data required");
  return length;
}

/// Read a request body into memory under a hard ceiling.
///
/// # Why this buffers instead of streaming into R2
///
/// It used to stream: the body was piped through a counting `TransformStream`
/// and handed straight to `put`/`uploadPart`. That never worked on a real
/// Workers runtime. workerd requires a streamed R2 body to have a *known*
/// length, and the output of `pipeThrough` does not carry one, so every
/// attachment upload failed with `TypeError: Provided readable stream must have
/// a known length`. The R2 test double accepted any stream, so the suite stayed
/// green while production rejected every request. Found 2026-07-26 by running
/// the post-deploy probe against a local workerd.
///
/// `FixedLengthStream` would restore true streaming, but it is a workerd global
/// that does not exist under this package's Node-based test runner. Using it
/// would mean the tested path and the shipped path differ — which is precisely
/// the gap that hid this bug for as long as it existed. Buffering keeps one
/// path for both.
///
/// The ceiling is the caller's existing bound: 8 MiB for one multipart part,
/// 26 MiB for a direct upload. A 512 MiB attachment is still never held in
/// memory — it arrives as up to 65 separately bounded parts.
export async function readBoundedAttachmentBody(
  source: ReadableStream<Uint8Array>,
  maxBytes: number,
): Promise<{ status: "ok"; bytes: Uint8Array } | { status: "too_large" }> {
  const reader = source.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    if (!value) continue;
    total += value.byteLength;
    if (total > maxBytes) {
      await reader.cancel("attachment too large");
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

interface AttachmentRow {
  object_key: string;
  size_bytes: number;
  /** Reclaim deadline. Short while incomplete, the content expiry once ready. */
  expires_at: number;
  /** The expiry promised in the session receipt; applied on completion. */
  content_expires_at: number | null;
  fetch_token_sha256_hex: string;
  state: "uploading" | "completing" | "ready";
  upload_id: string | null;
}

/// The expiry a caller was promised. Rows written before migration 0006 are
/// backfilled, so the fallback only covers a row read mid-migration.
function promisedExpiry(row: AttachmentRow): number {
  return row.content_expires_at ?? row.expires_at;
}

async function authorizedRow(
  request: Request,
  env: Env,
  id: string,
): Promise<AttachmentRow | Response> {
  if (!ID_RE.test(id)) return error(400, "bad_id", "id must be 32 lowercase hex chars");
  const row = await env.DB.prepare(
    `SELECT object_key, size_bytes, expires_at, content_expires_at,
            fetch_token_sha256_hex, state, upload_id
       FROM attachment_objects WHERE id = ? LIMIT 1`,
  ).bind(id).first<AttachmentRow>();
  if (!row || row.expires_at <= Math.floor(Date.now() / 1000)) return notFound();
  const presented = readCapability(request);
  if (presented === null) return error(401, "fetch_token_required", "X-OSL-Fetch-Token header required");
  const presentedDigest = await capabilityDigestHex(presented);
  if (!constantTimeEqual(row.fetch_token_sha256_hex, presentedDigest)) {
    return error(403, "fetch_token_mismatch", "fetch token does not match");
  }
  return row;
}

async function insertObject(
  env: Env,
  values: {
    id: string;
    objectKey: string;
    size: number;
    expiresAt: number;
    contentExpiresAt: number;
    createdAt: number;
    digest: string;
    state: AttachmentRow["state"];
    uploadId: string | null;
  },
): Promise<Response | null> {
  // This is one SQLite write statement. D1 serializes concurrent writers, so
  // the COUNT/SUM predicates and INSERT cannot race with another allocation.
  // Deletion releases capacity automatically because live usage is derived
  // from the authoritative object rows rather than a separate counter.
  //
  // The final predicate is the HIGH-1 fix. A row that does not yet hold stored
  // ciphertext is admitted against its own small reservation pool as well as
  // the global backstop, so bodyless sessions can never consume the capacity
  // that completed attachments need. `state` is bound twice so the reservation
  // predicate is skipped entirely for a directly-uploaded `ready` object.
  const inserted = await env.DB.prepare(
    `INSERT INTO attachment_objects
     (id, object_key, size_bytes, expires_at, content_expires_at, created_at,
      fetch_token_sha256_hex, state, upload_id)
     SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?
      WHERE (SELECT COUNT(*) FROM attachment_objects) < ?
        AND COALESCE((SELECT SUM(size_bytes) FROM attachment_objects), 0) <= ? - ?
        AND (? = 'ready' OR (
              (SELECT COUNT(*) FROM attachment_objects WHERE state <> 'ready') < ?
          AND COALESCE(
                (SELECT SUM(size_bytes) FROM attachment_objects WHERE state <> 'ready'), 0
              ) <= ? - ?
        ))`,
  ).bind(
    values.id,
    values.objectKey,
    values.size,
    values.expiresAt,
    values.contentExpiresAt,
    values.createdAt,
    values.digest,
    values.state,
    values.uploadId,
    MAX_LIVE_ATTACHMENT_ROWS,
    MAX_LIVE_ATTACHMENT_BYTES,
    values.size,
    values.state,
    MAX_INCOMPLETE_SESSION_ROWS,
    MAX_INCOMPLETE_SESSION_BYTES,
    values.size,
  ).run();
  return (inserted.meta.changes ?? 0) === 1
    ? null
    : error(503, "storage_capacity", "attachment storage is temporarily at capacity");
}

/// Non-authoritative headroom probe. The conditional INSERT above is what
/// actually enforces the pool; this only avoids creating — and immediately
/// aborting — an R2 multipart upload for every request of a flood.
async function reservationPoolExhausted(env: Env, size: number): Promise<boolean> {
  const usage = await env.DB.prepare(
    `SELECT COUNT(*) AS rows_used, COALESCE(SUM(size_bytes), 0) AS bytes_used
       FROM attachment_objects WHERE state <> 'ready'`,
  ).first<{ rows_used: number; bytes_used: number }>();
  if (!usage) return false;
  return usage.rows_used >= MAX_INCOMPLETE_SESSION_ROWS
    || usage.bytes_used + size > MAX_INCOMPLETE_SESSION_BYTES;
}

export async function handleAttachmentSessionCreate(request: Request, env: Env): Promise<Response> {
  const ttl = parseAllowedTtlSeconds(request.headers.get("x-osl-ttl-seconds"));
  if (ttl === null) return error(400, "bad_ttl", "unsupported attachment TTL");
  const capability = readCapability(request);
  if (capability === null) return error(400, "bad_fetch_token", "invalid fetch token");
  const declared = unsignedLength(request.headers.get("x-osl-size-bytes"), MAX_SEALED_ATTACHMENT_BYTES);
  if (declared instanceof Response || declared === null) {
    return declared ?? error(400, "size_required", "X-OSL-Size-Bytes header required");
  }

  if (await reservationPoolExhausted(env, declared)) {
    return error(503, "storage_capacity", "attachment storage is temporarily at capacity");
  }

  const id = randomHex(16);
  const objectKey = `attachments/${id}`;
  const multipart = await env.ATTACHMENTS.createMultipartUpload(objectKey);
  const now = Math.floor(Date.now() / 1000);
  // The caller's TTL is a promise about stored content, so it is recorded but
  // not applied: until the upload completes the row is reclaimable within
  // INCOMPLETE_SESSION_TTL_SECONDS, sliding forward on each accepted part.
  const contentExpiresAt = now + ttl;
  try {
    const rejected = await insertObject(env, {
      id,
      objectKey,
      size: declared,
      expiresAt: now + INCOMPLETE_SESSION_TTL_SECONDS,
      contentExpiresAt,
      createdAt: now,
      digest: await capabilityDigestHex(capability),
      state: "uploading",
      uploadId: multipart.uploadId,
    });
    if (rejected) {
      await multipart.abort();
      return rejected;
    }
  } catch (databaseError) {
    await multipart.abort().catch(() => undefined);
    throw databaseError;
  }
  // `expires_at` reports the promised content expiry. The shipping Rust client
  // compares this value against the completion receipt and refuses the upload
  // if they differ, and its response struct is `deny_unknown_fields`, so the
  // internal reclaim deadline cannot be surfaced as an extra field.
  return json({
    id,
    expires_at: contentExpiresAt,
    size_bytes: declared,
    max_part_bytes: MAX_ATTACHMENT_PART_BYTES,
    max_parts: MAX_ATTACHMENT_PARTS,
  }, 201);
}

export async function handleAttachmentPartUpload(
  request: Request,
  env: Env,
  id: string,
  partNumber: number,
): Promise<Response> {
  if (!Number.isInteger(partNumber) || partNumber < 1 || partNumber > MAX_ATTACHMENT_PARTS) {
    return error(400, "bad_part", `part number must be between 1 and ${MAX_ATTACHMENT_PARTS}`);
  }
  const row = await authorizedRow(request, env, id);
  if (row instanceof Response) return row;
  if (row.state !== "uploading" || !row.upload_id) {
    return error(409, "upload_not_open", "attachment upload is not open");
  }
  const expectedParts = Math.ceil(row.size_bytes / MAX_ATTACHMENT_PART_BYTES);
  if (partNumber > expectedParts) {
    return error(400, "bad_part", "part number exceeds the declared attachment size");
  }
  if (!request.body) return error(400, "empty_body", "attachment part required");
  const declared = unsignedLength(request.headers.get("content-length"), MAX_ATTACHMENT_PART_BYTES);
  if (declared instanceof Response || declared === null) {
    return declared ?? error(411, "content_length_required", "Content-Length is required for attachment parts");
  }
  const expectedLength = partNumber < expectedParts
    ? MAX_ATTACHMENT_PART_BYTES
    : row.size_bytes - MAX_ATTACHMENT_PART_BYTES * (expectedParts - 1);
  if (declared !== expectedLength) {
    return error(400, "bad_part_length", "part length does not match the declared attachment size");
  }

  // One conditional UPSERT makes a retry replace its own reservation while
  // atomically excluding that old value from the aggregate size check.
  const reserved = await env.DB.prepare(
    `INSERT INTO attachment_parts (attachment_id, part_number, size_bytes, etag)
     SELECT ?, ?, ?, NULL
      WHERE EXISTS (
        SELECT 1 FROM attachment_objects WHERE id = ? AND state = 'uploading'
      )
        AND ? + COALESCE((
          SELECT SUM(size_bytes) FROM attachment_parts
           WHERE attachment_id = ? AND part_number <> ?
        ), 0) <= (
          SELECT size_bytes FROM attachment_objects WHERE id = ?
        )
     ON CONFLICT(attachment_id, part_number) DO UPDATE SET
       size_bytes = excluded.size_bytes, etag = NULL`,
  ).bind(
    id,
    partNumber,
    declared,
    id,
    declared,
    id,
    partNumber,
    id,
  ).run();
  if ((reserved.meta.changes ?? 0) !== 1) {
    return error(409, "part_exceeds_declared_size", "attachment parts exceed the declared size");
  }

  const upload = env.ATTACHMENTS.resumeMultipartUpload(row.object_key, row.upload_id);
  // Read and validate before touching R2: the length is now known up front, so
  // an invalid part never becomes a partial object that has to be aborted.
  const counted = await readBoundedAttachmentBody(request.body, MAX_ATTACHMENT_PART_BYTES);
  if (counted.status === "too_large") {
    await upload.abort().catch(() => undefined);
    await env.DB.prepare("DELETE FROM attachment_objects WHERE id = ?").bind(id).run();
    return error(413, "too_large", `attachment part exceeds ${MAX_ATTACHMENT_PART_BYTES} bytes`);
  }
  const size = counted.bytes.byteLength;
  if (size === 0 || size !== declared) {
    await upload.abort().catch(() => undefined);
    await env.DB.prepare("DELETE FROM attachment_objects WHERE id = ?").bind(id).run();
    return error(400, size === 0 ? "empty_body" : "content_length_mismatch", "invalid attachment part length");
  }
  try {
    const part = await upload.uploadPart(partNumber, counted.bytes);
    await env.DB.prepare(
      `UPDATE attachment_parts SET etag = ?
       WHERE attachment_id = ? AND part_number = ? AND size_bytes = ?`,
    ).bind(part.etag, id, partNumber, size).run();
    // Accepted progress slides the reclaim deadline forward, so a slow but
    // genuine multi-part upload is never cut off by the short hold that keeps
    // abandoned reservations from parking capacity. Never past the promised
    // content expiry, which stays the outer bound.
    const slideTo = Math.min(
      Math.floor(Date.now() / 1000) + INCOMPLETE_SESSION_TTL_SECONDS,
      promisedExpiry(row),
    );
    await env.DB.prepare(
      `UPDATE attachment_objects SET expires_at = ?
        WHERE id = ? AND state = 'uploading' AND expires_at < ?`,
    ).bind(slideTo, id, slideTo).run();
    return json({ part_number: part.partNumber, size_bytes: size }, 201);
  } catch (uploadError) {
    // Oversize is now rejected before R2 is touched, so anything reaching here
    // is a genuine storage failure. Leave the reservation in place: it holds
    // only the short reclaim deadline and the client may retry the part.
    throw uploadError;
  }
}

interface PartRow { part_number: number; size_bytes: number; etag: string }

export async function handleAttachmentComplete(request: Request, env: Env, id: string): Promise<Response> {
  const row = await authorizedRow(request, env, id);
  if (row instanceof Response) return row;
  if (row.state !== "uploading" || !row.upload_id) {
    return row.state === "ready"
      ? json({ id, expires_at: promisedExpiry(row), size_bytes: row.size_bytes }, 200)
      : error(409, "upload_not_open", "attachment upload is not open");
  }
  const result = await env.DB.prepare(
    `SELECT part_number, size_bytes, etag FROM attachment_parts
     WHERE attachment_id = ? AND etag IS NOT NULL ORDER BY part_number`,
  ).bind(id).all<PartRow>();
  const parts = result.results ?? [];
  const total = parts.reduce((sum, part) => sum + part.size_bytes, 0);
  if (parts.length === 0 || total !== row.size_bytes
      || parts.some((part, index) => part.part_number !== index + 1)) {
    return error(409, "parts_incomplete", "attachment parts are incomplete");
  }
  const claimed = await env.DB.prepare(
    "UPDATE attachment_objects SET state = 'completing' WHERE id = ? AND state = 'uploading'",
  ).bind(id).run();
  if ((claimed.meta.changes ?? 0) !== 1) return error(409, "upload_not_open", "attachment upload is not open");

  const upload = env.ATTACHMENTS.resumeMultipartUpload(row.object_key, row.upload_id);
  let completed: R2Object;
  try {
    completed = await upload.complete(parts.map((part) => ({
      partNumber: part.part_number,
      etag: part.etag,
    })));
  } catch (completionError) {
    await env.DB.prepare(
      "UPDATE attachment_objects SET state = 'uploading' WHERE id = ? AND state = 'completing'",
    ).bind(id).run().catch(() => undefined);
    throw completionError;
  }
  if (completed.size !== row.size_bytes) {
    await env.ATTACHMENTS.delete(row.object_key).catch(() => undefined);
    await env.DB.prepare("DELETE FROM attachment_objects WHERE id = ?").bind(id).run();
    return error(500, "completed_size_mismatch", "completed attachment size did not match");
  }
  // The promised content expiry was fixed at session creation. The shipping
  // Rust client rejects a completion receipt whose `expires_at` differs from
  // the session receipt's value, so completion does not start a fresh TTL: it
  // only moves the reclaim deadline from the short incomplete-session hold to
  // the already-promised instant. A slow upload therefore gets less ready-state
  // lifetime.
  const contentExpiresAt = promisedExpiry(row);
  try {
    await env.DB.batch([
      env.DB.prepare(
        `UPDATE attachment_objects
            SET state = 'ready', upload_id = NULL, expires_at = ?
          WHERE id = ? AND state = 'completing'`,
      ).bind(contentExpiresAt, id),
      env.DB.prepare("DELETE FROM attachment_parts WHERE attachment_id = ?").bind(id),
    ]);
    return json({ id, expires_at: contentExpiresAt, size_bytes: row.size_bytes }, 201);
  } catch (metadataError) {
    // Leave the row in `completing`. Fetch stays closed; authenticated delete
    // and the expiry sweep use HEAD to remove the completed object safely.
    throw metadataError;
  }
}

export async function handleAttachmentUpload(request: Request, env: Env): Promise<Response> {
  const declared = unsignedLength(request.headers.get("content-length"), MAX_DIRECT_ATTACHMENT_BYTES);
  if (declared instanceof Response) return declared;
  const ttl = parseAllowedTtlSeconds(request.headers.get("x-osl-ttl-seconds"));
  if (ttl === null) return error(400, "bad_ttl", "unsupported attachment TTL");
  const capability = readCapability(request);
  if (capability === null) return error(400, "bad_fetch_token", "invalid fetch token");
  if (!request.body) return error(400, "empty_body", "attachment body required");

  // Read and validate before allocating storage. The body count is
  // authoritative for a chunked or dishonestly declared length, and rejecting
  // here means an invalid upload never creates an R2 object at all.
  const counted = await readBoundedAttachmentBody(request.body, MAX_DIRECT_ATTACHMENT_BYTES);
  if (counted.status === "too_large") {
    return error(413, "too_large", `attachment exceeds ${MAX_DIRECT_ATTACHMENT_BYTES} bytes`);
  }
  const size = counted.bytes.byteLength;
  if (size === 0 || (typeof declared === "number" && declared !== size)) {
    return error(400, size === 0 ? "empty_body" : "content_length_mismatch", "invalid attachment length");
  }

  const id = randomHex(16);
  const objectKey = `attachments/${id}`;
  try {
    const stored = await env.ATTACHMENTS.put(objectKey, counted.bytes, {
      onlyIf: { etagDoesNotMatch: "*" },
    });
    if (!stored) return error(503, "id_collision", "could not allocate attachment storage");
    if (stored.size !== size) {
      await env.ATTACHMENTS.delete(objectKey);
      return error(400, "content_length_mismatch", "invalid attachment length");
    }
    const now = Math.floor(Date.now() / 1000);
    // A direct upload already holds its ciphertext, so its content TTL starts
    // immediately and it is admitted against the global budget only.
    const rejected = await insertObject(env, {
      id,
      objectKey,
      size,
      expiresAt: now + ttl,
      contentExpiresAt: now + ttl,
      createdAt: now,
      digest: await capabilityDigestHex(capability),
      state: "ready",
      uploadId: null,
    });
    if (rejected) {
      await env.ATTACHMENTS.delete(objectKey);
      return rejected;
    }
    return json({ id, expires_at: now + ttl, size_bytes: size }, 201);
  } catch (uploadError) {
    // Oversize is rejected before this point, so anything here is a real
    // storage failure. Remove any partial object rather than orphaning it.
    await env.ATTACHMENTS.delete(objectKey).catch(() => undefined);
    throw uploadError;
  }
}

export async function handleAttachmentFetch(request: Request, env: Env, id: string): Promise<Response> {
  const row = await authorizedRow(request, env, id);
  if (row instanceof Response) return row;
  if (row.state !== "ready") return notFound();
  const object = await env.ATTACHMENTS.get(row.object_key);
  if (!object || object.size !== row.size_bytes) return notFound();
  return new Response(object.body, {
    status: 200,
    headers: {
      "content-type": "application/octet-stream",
      "content-length": String(row.size_bytes),
      "cache-control": "no-store",
    },
  });
}

export async function handleAttachmentDelete(request: Request, env: Env, id: string): Promise<Response> {
  const row = await authorizedRow(request, env, id);
  if (row instanceof Response) {
    if (row.status === 404 && ID_RE.test(id)) return new Response(null, { status: 204 });
    return row;
  }
  await removeAttachmentStorage(env, row);
  await env.DB.prepare("DELETE FROM attachment_objects WHERE id = ? AND object_key = ?")
    .bind(id, row.object_key).run();
  return new Response(null, { status: 204 });
}

export async function removeAttachmentStorage(
  env: Env,
  row: { object_key: string; upload_id: string | null },
): Promise<void> {
  if (row.upload_id) {
    // A completing session may already have become a normal object before D1
    // recorded readiness. HEAD distinguishes that crash window from a live
    // multipart upload without treating a transient abort failure as success.
    const completed = await env.ATTACHMENTS.head(row.object_key);
    if (completed) {
      await env.ATTACHMENTS.delete(row.object_key);
      return;
    }
    await env.ATTACHMENTS.resumeMultipartUpload(row.object_key, row.upload_id).abort();
  }
  await env.ATTACHMENTS.delete(row.object_key);
}
