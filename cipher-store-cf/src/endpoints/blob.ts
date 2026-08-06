/// Opaque message-payload endpoints. Payload bytes live only in R2; D1 holds
/// capability digests and lifecycle metadata.
import type { Env } from "../env.js";
import { isPadmeLength, MAX_LIVE_BLOB_BYTES, MAX_LIVE_BLOB_ROWS } from "../lib/blob-limits.js";
import { applyBurn } from "../lib/burn-policy.js";
import { DELETE_GRANT_RECORD, validateDeleteGrant } from "../lib/delete-grant.js";
import { constantTimeEqualHex, sha256Hex } from "../lib/digest.js";
import { error, json, notFound } from "../lib/http.js";
import { R2PayloadStore } from "../lib/payload-store.js";
import { parseUploadTtl } from "../lib/ttl.js";
import {
  isOhttpReadyBlobFetch,
  ohttpBlobFetchResponse,
} from "./blob-request-profile.js";

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
  const deleteGrantMessage = request.headers.get("x-osl-delete-message")?.trim() ?? null;
  const deleteGrantOwner = request.headers.get("x-osl-delete-owner")?.trim() ?? null;
  const burnScope = request.headers.get("x-osl-burn-scope")?.trim() ?? null;
  const objectClass = request.headers.get("x-osl-object-class");
  if (!blobId || !fetchDigest || !ackDigest || !manageDigest || !deliveryTag
      || (objectClass !== "single-ack" && objectClass !== "multi-fetch")) return null;
  const deleteGrantFields = [deleteGrantMessage, deleteGrantOwner, burnScope];
  if (deleteGrantFields.some((field) => field !== null) && deleteGrantFields.some((field) => field === null)) {
    return null;
  }
  if (deleteGrantMessage !== null) {
    const owner = deleteGrantOwner!;
    const scope = burnScope!;
    const validation = validateDeleteGrant({
      grant: {
        record: DELETE_GRANT_RECORD,
        message: deleteGrantMessage,
        owner,
        scope,
      },
      message: deleteGrantMessage,
      owner,
      burnScope: scope,
      allowedBurnScope: scope,
    });
    if (!validation.ok) return null;
  }
  return {
    blobId,
    fetchDigest,
    ackDigest,
    manageDigest,
    deliveryTag,
    objectClass,
    deleteGrantMessage,
    deleteGrantOwner,
    burnScope,
  };
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

  // Aggregate backstop (audit HIGH-2) and id admission in ONE write statement,
  // so the COUNT/SUM predicates cannot race another insert: every concurrent
  // upload evaluates them against the state its own row is written into, not
  // against a value read before someone else's row landed (D-256).
  //
  // The `NOT EXISTS` predicate belongs inside the same statement for the same
  // reason. It also replaces the `409 blob_id_collision` that made this route
  // an existence oracle (D-255): zero rows changed is answered below without
  // telling the caller which predicate stopped it.
  const admitted = await insertBlobRow(env, headers, body.bytes, expiresAt, now);
  if (!admitted) {
    // Nothing was written. Deciding what to say must not depend on the id, so
    // only the aggregate is re-read: at capacity EVERY upload is refused
    // whatever id it names, which is a fact about the store, not about this
    // blob. Otherwise the id was taken, and the caller gets the answer an
    // unused id gets. An upload carries no authority over the id it names --
    // the storage grant is anonymous and claims only aud/exp/jti -- so it has
    // earned no more than that. First writer keeps the row.
    const capacity = await env.DB.prepare(
      "SELECT COUNT(*) AS rows, COALESCE(SUM(size_bytes), 0) AS bytes FROM blob_capability_index",
    ).first<{ rows: number; bytes: number }>();
    if ((capacity?.rows ?? 0) >= MAX_LIVE_BLOB_ROWS || (capacity?.bytes ?? 0) + body.bytes.byteLength > MAX_LIVE_BLOB_BYTES) {
      return error(503, "storage_capacity", "blob storage is temporarily at capacity");
    }
    return json({ id: headers.blobId, expires_at: expiresAt }, 201);
  }

  // R2 receives the SHA-256(fetch_cap) key, supplied here as a digest. This
  // avoids ever persisting the bearer capability in D1 or object metadata.
  // It happens only after the row is ours, so a caller who named someone
  // else's id never reaches `putByDigest` and cannot overwrite its bytes.
  //
  // Winning the row is NOT the whole guard, because the object key is not the
  // id: a caller may hold a genuinely fresh id and still name another blob's
  // fetch digest (D-264). `putByDigest` is therefore conditional on absence,
  // like the attachment direct upload. A refused write is deliberately not
  // reported and does not roll the row back -- both would be new answers about
  // an object this caller was never told about, which is exactly the D-255
  // signal one key space over. The refusal is the property; the response is
  // unchanged.
  const payloads = new R2PayloadStore(env.PAYLOADS);
  try {
    await payloads.putByDigest(headers.fetchDigest, body.bytes);
  } catch (cause) {
    // Roll the index row back by id -- the row this call created. Deleting by
    // fetch digest would destroy a payload the digest may be shared with.
    await env.DB.prepare("DELETE FROM blob_capability_index WHERE blob_id = ?").bind(headers.blobId).run();
    throw cause;
  }
  return json({ id: headers.blobId, expires_at: expiresAt }, 201);
}

/// Claim `blobId` and the storage it needs, or claim nothing. Returns whether
/// the row was written; the caller may not learn which predicate refused it.
async function insertBlobRow(
  env: Env,
  headers: NonNullable<ReturnType<typeof uploadHeaders>>,
  bytes: Uint8Array,
  expiresAt: number,
  now: number,
): Promise<boolean> {
  try {
    const inserted = await env.DB.prepare(
      `INSERT INTO blob_capability_index (
        blob_id, fetch_digest_sha256_hex, ack_digest_sha256_hex,
        manage_digest_sha256_hex, object_class, pool, delivery_tag,
        size_bytes, expires_at, created_at,
        delete_grant_message, delete_grant_owner, burn_scope
      )
      SELECT ?, ?, ?, ?, ?, 'undelivered', ?, ?, ?, ?, ?, ?, ?
       WHERE NOT EXISTS (SELECT 1 FROM blob_capability_index WHERE blob_id = ?)
         AND (SELECT COUNT(*) FROM blob_capability_index) < ?
         AND COALESCE((SELECT SUM(size_bytes) FROM blob_capability_index), 0) <= ? - ?`,
    ).bind(headers.blobId, headers.fetchDigest, headers.ackDigest, headers.manageDigest,
      headers.objectClass, headers.deliveryTag, bytes.byteLength, expiresAt, now,
      headers.deleteGrantMessage, headers.deleteGrantOwner, headers.burnScope,
      headers.blobId, MAX_LIVE_BLOB_ROWS, MAX_LIVE_BLOB_BYTES, bytes.byteLength).run();
    return (inserted.meta?.changes ?? 0) === 1;
  } catch (cause) {
    // The primary key is the backstop behind `NOT EXISTS`: a row that landed
    // between the predicate and the write raises here instead of overwriting.
    // That is the taken-id case, and it is answered exactly like one.
    const message = cause instanceof Error ? cause.message : String(cause);
    if (message.includes("UNIQUE") || message.includes("PRIMARY")) return false;
    throw cause;
  }
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
  // Preserve the header-only bearer boundary and the one-shot OHTTP shape
  // before touching storage. Its negative surface remains the shared 404.
  if (!isOhttpReadyBlobFetch(request)) return notFound();
  const fetchCap = hexHeader(request, "x-osl-fetch-cap", CAP_RE);
  const row = await liveRow(env, blobId);
  if (!fetchCap || !row || !constantTimeEqualHex(await sha256Hex(fetchCap), row.fetch_digest_sha256_hex)) return notFound();
  const bytes = await new R2PayloadStore(env.PAYLOADS).get(fetchCap);
  if (!bytes) return notFound();
  return ohttpBlobFetchResponse(bytes);
}

export async function handleDelete(request: Request, env: Env, blobId: string): Promise<Response> {
  const grant = deleteGrantHeader(request);
  if (grant === null) {
    return error(403, "delete_grant_required", "delete grant required");
  }
  return applyBurn({
    async manageCapabilityDigestFor(id) {
      if (!ID_RE.test(id)) return null;
      const row = await env.DB.prepare(
        `SELECT manage_digest_sha256_hex, delete_grant_message, delete_grant_owner, burn_scope
           FROM blob_capability_index
          WHERE blob_id = ?
          LIMIT 1`,
      ).bind(id).first<DeleteGrantBlobRow>();
      if (!row || !deleteGrantAllowsStoredCopyDelete(grant, row)) return null;
      return row.manage_digest_sha256_hex;
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

type DeleteGrantBlobRow = {
  manage_digest_sha256_hex: string;
  delete_grant_message: string | null;
  delete_grant_owner: string | null;
  burn_scope: string | null;
};

function deleteGrantHeader(request: Request): unknown | null {
  const value = request.headers.get("x-osl-delete-grant");
  if (value === null || value.trim() === "") return null;
  return value;
}

function deleteGrantAllowsStoredCopyDelete(grant: unknown, row: DeleteGrantBlobRow): boolean {
  if (row.delete_grant_message === null || row.delete_grant_owner === null || row.burn_scope === null) {
    return false;
  }
  const validation = validateDeleteGrant({
    grant,
    message: row.delete_grant_message,
    owner: row.delete_grant_owner,
    burnScope: row.burn_scope,
    allowedBurnScope: row.burn_scope,
  });
  return validation.ok;
}
