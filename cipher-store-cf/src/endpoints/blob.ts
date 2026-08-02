/// Opaque message-payload endpoints. Payload bytes live only in R2; D1 holds
/// capability digests and lifecycle metadata.
import type { Env } from "../env.js";
import { isPadmeLength, MAX_LIVE_BLOB_BYTES, MAX_LIVE_BLOB_ROWS } from "../lib/blob-limits.js";
import { applyBurn } from "../lib/burn-policy.js";
import { constantTimeEqualHex, sha256Hex } from "../lib/digest.js";
import { error, json, notFound } from "../lib/http.js";
import { R2PayloadStore } from "../lib/payload-store.js";
import { parseUploadTtl } from "../lib/ttl.js";

export const MAX_BLOB_BYTES = 64 * 1024;
const CAP_RE = /^[0-9a-f]{32}$/;
const DIGEST_RE = /^[0-9a-f]{64}$/;
const ID_RE = /^[0-9a-f]{32}$/;

function hexHeader(request: Request, name: string, re: RegExp): string | null {
  const value = request.headers.get(name)?.trim().toLowerCase();
  return value && re.test(value) ? value : null;
}

function uploadHeaders(request: Request) {
  const blobId = hexHeader(request, "x-osl-blob-id", ID_RE);
  const fetchDigest = hexHeader(request, "x-osl-fetch-digest", DIGEST_RE);
  const ackDigest = hexHeader(request, "x-osl-ack-digest", DIGEST_RE);
  const manageDigest = hexHeader(request, "x-osl-manage-digest", DIGEST_RE);
  const deliveryTag = hexHeader(request, "x-osl-delivery-tag", ID_RE);
  const objectClass = request.headers.get("x-osl-object-class");
  if (!blobId || !fetchDigest || !ackDigest || !manageDigest || !deliveryTag
      || (objectClass !== "single-ack" && objectClass !== "multi-fetch")) return null;
  return { blobId, fetchDigest, ackDigest, manageDigest, deliveryTag, objectClass };
}

export async function handleUpload(request: Request, env: Env): Promise<Response> {
  const length = request.headers.get("content-length");
  if (length !== null && (!/^\d+$/.test(length) || !Number.isSafeInteger(Number(length)))) {
    return error(400, "bad_content_length", "Content-Length must be an unsigned integer");
  }
  if (length !== null && Number(length) > MAX_BLOB_BYTES) return error(413, "too_large", `blob exceeds ${MAX_BLOB_BYTES} bytes`);
  const ttl = parseUploadTtl(request.headers.get("x-osl-ttl-seconds"), request.headers.get("x-osl-expiry-mode"));
  if (ttl === null) return error(400, "bad_ttl", "X-OSL-TTL-Seconds must be 3600 (1h), 86400 (24h), 259200 (72h), or 604800 (7d); default mode requires 604800");
  const headers = uploadHeaders(request);
  if (!headers) return error(400, "bad_blob_metadata", "blob id, capability digests, class, and delivery tag are required");
  const body = await readBoundedBody(request, MAX_BLOB_BYTES);
  if (body.status === "too_large") return error(413, "too_large", `blob exceeds ${MAX_BLOB_BYTES} bytes`);
  if (body.bytes.byteLength === 0) return error(400, "empty_body", "blob body required");
  if (!isPadmeLength(body.bytes.byteLength)) return error(400, "invalid_padding", "blob length must be Padmé-padded");

  const now = Math.floor(Date.now() / 1000);
  const expiresAt = now + ttl.ttl;
  const existing = await env.DB.prepare("SELECT 1 FROM blob_capability_index WHERE blob_id = ? LIMIT 1").bind(headers.blobId).first();
  if (existing) return error(409, "blob_id_collision", "blob id is already in use");
  const capacity = await env.DB.prepare(
    "SELECT COUNT(*) AS rows, COALESCE(SUM(size_bytes), 0) AS bytes FROM blob_capability_index",
  ).first<{ rows: number; bytes: number }>();
  if ((capacity?.rows ?? 0) >= MAX_LIVE_BLOB_ROWS || (capacity?.bytes ?? 0) + body.bytes.byteLength > MAX_LIVE_BLOB_BYTES) {
    return error(503, "storage_capacity", "blob storage is temporarily at capacity");
  }

  // R2 receives the SHA-256(fetch_cap) key, supplied here as a digest. This
  // avoids ever persisting the bearer capability in D1 or object metadata.
  const payloads = new R2PayloadStore(env.PAYLOADS);
  await payloads.putByDigest(headers.fetchDigest, body.bytes);
  try {
    await env.DB.prepare(
      `INSERT INTO blob_capability_index (
        blob_id, fetch_digest_sha256_hex, ack_digest_sha256_hex,
        manage_digest_sha256_hex, object_class, pool, delivery_tag,
        size_bytes, expires_at, created_at
      ) VALUES (?, ?, ?, ?, ?, 'undelivered', ?, ?, ?, ?)`,
    ).bind(headers.blobId, headers.fetchDigest, headers.ackDigest, headers.manageDigest,
      headers.objectClass, headers.deliveryTag, body.bytes.byteLength, expiresAt, now).run();
  } catch (cause) {
    await payloads.deleteByDigest(headers.fetchDigest);
    throw cause;
  }
  return json({ id: headers.blobId, expires_at: expiresAt }, 201);
}

export async function readBoundedBody(request: Request, maxBytes: number): Promise<{ status: "ok"; bytes: Uint8Array } | { status: "too_large" }> {
  if (!request.body) return { status: "ok", bytes: new Uint8Array() };
  const reader = request.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    if (!value) continue;
    total += value.byteLength;
    if (total > maxBytes) { await reader.cancel("blob too large"); return { status: "too_large" }; }
    chunks.push(value);
  }
  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  return { status: "ok", bytes };
}

type BlobRow = { fetch_digest_sha256_hex: string; manage_digest_sha256_hex: string; expires_at: number };

async function liveRow(env: Env, blobId: string): Promise<BlobRow | null> {
  if (!ID_RE.test(blobId)) return null;
  const row = await env.DB.prepare(
    "SELECT fetch_digest_sha256_hex, manage_digest_sha256_hex, expires_at FROM blob_capability_index WHERE blob_id = ? LIMIT 1",
  ).bind(blobId).first<BlobRow>();
  return row && row.expires_at >= Math.floor(Date.now() / 1000) ? row : null;
}

export async function handleFetch(request: Request, env: Env, blobId: string): Promise<Response> {
  const fetchCap = hexHeader(request, "x-osl-fetch-cap", CAP_RE);
  const row = await liveRow(env, blobId);
  if (!fetchCap || !row || !constantTimeEqualHex(await sha256Hex(fetchCap), row.fetch_digest_sha256_hex)) return notFound();
  const bytes = await new R2PayloadStore(env.PAYLOADS).get(fetchCap);
  if (!bytes) return notFound();
  return new Response(bytes, { status: 200, headers: { "content-type": "application/octet-stream", "cache-control": "no-store", "content-length": String(bytes.byteLength) } });
}

export async function handleDelete(request: Request, env: Env, blobId: string): Promise<Response> {
  return applyBurn({
    async manageCapabilityDigestFor(id) {
      if (!ID_RE.test(id)) return null;
      const row = await env.DB.prepare(
        "SELECT manage_digest_sha256_hex FROM blob_capability_index WHERE blob_id = ? LIMIT 1",
      ).bind(id).first<{ manage_digest_sha256_hex: string }>();
      return row?.manage_digest_sha256_hex ?? null;
    },
    async destroy(id) {
      // Read the digest as part of deletion, rather than trusting the caller's
      // authority or relying on freshness. This also cleans a row that expired
      // just before its scheduled sweep.
      const row = await env.DB.prepare(
        "SELECT fetch_digest_sha256_hex FROM blob_capability_index WHERE blob_id = ? LIMIT 1",
      ).bind(id).first<{ fetch_digest_sha256_hex: string }>();
      if (!row) return;
      await new R2PayloadStore(env.PAYLOADS).deleteByDigest(row.fetch_digest_sha256_hex);
      await env.DB.prepare("DELETE FROM blob_capability_index WHERE blob_id = ?").bind(id).run();
    },
  }, blobId, request.headers.get("x-osl-manage-cap"));
}
