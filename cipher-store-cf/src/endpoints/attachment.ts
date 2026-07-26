/// R2-backed opaque attachment transport.
///
/// Bodies are already sealed by OSL. The Worker never receives identity,
/// filename, MIME, conversation, or plaintext metadata. Large objects use R2
/// multipart uploads with bounded streamed parts; D1 stores only opaque object
/// state, expiry, part receipts, and a SHA-256 digest of the bearer capability.

import type { Env } from "../env.js";
import {
  MAX_ATTACHMENT_PART_BYTES,
  MAX_ATTACHMENT_PARTS,
  MAX_DIRECT_ATTACHMENT_BYTES,
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

export function boundedAttachmentStream(
  source: ReadableStream<Uint8Array>,
  maxBytes: number,
): { body: ReadableStream<Uint8Array>; bytesRead: () => number } {
  let total = 0;
  const body = source.pipeThrough(new TransformStream<Uint8Array, Uint8Array>({
    transform(chunk, controller) {
      total += chunk.byteLength;
      if (total > maxBytes) throw new Error("attachment_too_large");
      controller.enqueue(chunk);
    },
  }));
  return { body, bytesRead: () => total };
}

interface AttachmentRow {
  object_key: string;
  size_bytes: number;
  expires_at: number;
  fetch_token_sha256_hex: string;
  state: "uploading" | "completing" | "ready";
  upload_id: string | null;
}

async function authorizedRow(
  request: Request,
  env: Env,
  id: string,
): Promise<AttachmentRow | Response> {
  if (!ID_RE.test(id)) return error(400, "bad_id", "id must be 32 lowercase hex chars");
  const row = await env.DB.prepare(
    `SELECT object_key, size_bytes, expires_at, fetch_token_sha256_hex, state, upload_id
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
  const inserted = await env.DB.prepare(
    `INSERT INTO attachment_objects
     (id, object_key, size_bytes, expires_at, created_at, fetch_token_sha256_hex, state, upload_id)
     SELECT ?, ?, ?, ?, ?, ?, ?, ?
      WHERE (SELECT COUNT(*) FROM attachment_objects) < ?
        AND COALESCE((SELECT SUM(size_bytes) FROM attachment_objects), 0) <= ? - ?`,
  ).bind(
    values.id,
    values.objectKey,
    values.size,
    values.expiresAt,
    values.createdAt,
    values.digest,
    values.state,
    values.uploadId,
    MAX_LIVE_ATTACHMENT_ROWS,
    MAX_LIVE_ATTACHMENT_BYTES,
    values.size,
  ).run();
  return (inserted.meta.changes ?? 0) === 1
    ? null
    : error(503, "storage_capacity", "attachment storage is temporarily at capacity");
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

  const id = randomHex(16);
  const objectKey = `attachments/${id}`;
  const multipart = await env.ATTACHMENTS.createMultipartUpload(objectKey);
  const now = Math.floor(Date.now() / 1000);
  try {
    const rejected = await insertObject(env, {
      id,
      objectKey,
      size: declared,
      expiresAt: now + ttl,
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
  return json({
    id,
    expires_at: now + ttl,
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
  const counted = boundedAttachmentStream(request.body, MAX_ATTACHMENT_PART_BYTES);
  try {
    const part = await upload.uploadPart(partNumber, counted.body);
    const size = counted.bytesRead();
    if (size === 0 || size !== declared) {
      await upload.abort().catch(() => undefined);
      await env.DB.prepare("DELETE FROM attachment_objects WHERE id = ?").bind(id).run();
      return error(400, size === 0 ? "empty_body" : "content_length_mismatch", "invalid attachment part length");
    }
    await env.DB.prepare(
      `UPDATE attachment_parts SET etag = ?
       WHERE attachment_id = ? AND part_number = ? AND size_bytes = ?`,
    ).bind(part.etag, id, partNumber, size).run();
    return json({ part_number: part.partNumber, size_bytes: size }, 201);
  } catch (uploadError) {
    if (uploadError instanceof Error && uploadError.message === "attachment_too_large") {
      await upload.abort().catch(() => undefined);
      await env.DB.prepare("DELETE FROM attachment_objects WHERE id = ?").bind(id).run();
      return error(413, "too_large", `attachment part exceeds ${MAX_ATTACHMENT_PART_BYTES} bytes`);
    }
    throw uploadError;
  }
}

interface PartRow { part_number: number; size_bytes: number; etag: string }

export async function handleAttachmentComplete(request: Request, env: Env, id: string): Promise<Response> {
  const row = await authorizedRow(request, env, id);
  if (row instanceof Response) return row;
  if (row.state !== "uploading" || !row.upload_id) {
    return row.state === "ready"
      ? json({ id, expires_at: row.expires_at, size_bytes: row.size_bytes }, 200)
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
  try {
    await env.DB.batch([
      env.DB.prepare(
        "UPDATE attachment_objects SET state = 'ready', upload_id = NULL WHERE id = ? AND state = 'completing'",
      ).bind(id),
      env.DB.prepare("DELETE FROM attachment_parts WHERE attachment_id = ?").bind(id),
    ]);
    return json({ id, expires_at: row.expires_at, size_bytes: row.size_bytes }, 201);
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

  const id = randomHex(16);
  const objectKey = `attachments/${id}`;
  const counted = boundedAttachmentStream(request.body, MAX_DIRECT_ATTACHMENT_BYTES);
  try {
    const stored = await env.ATTACHMENTS.put(objectKey, counted.body, {
      onlyIf: { etagDoesNotMatch: "*" },
    });
    const size = counted.bytesRead();
    if (!stored) return error(503, "id_collision", "could not allocate attachment storage");
    if (size === 0 || stored.size !== size || (typeof declared === "number" && declared !== size)) {
      await env.ATTACHMENTS.delete(objectKey);
      return error(400, size === 0 ? "empty_body" : "content_length_mismatch", "invalid attachment length");
    }
    const now = Math.floor(Date.now() / 1000);
    const rejected = await insertObject(env, {
      id,
      objectKey,
      size,
      expiresAt: now + ttl,
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
    await env.ATTACHMENTS.delete(objectKey).catch(() => undefined);
    if (uploadError instanceof Error && uploadError.message === "attachment_too_large") {
      return error(413, "too_large", `attachment exceeds ${MAX_DIRECT_ATTACHMENT_BYTES} bytes`);
    }
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
