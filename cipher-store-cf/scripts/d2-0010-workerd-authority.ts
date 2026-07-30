/**
 * Binding-backed D2 readback authority.
 *
 * This module accepts no caller-supplied readback JSON. It reconstructs one
 * exact-size readback from a durable D1 authority receipt, the shipping D1
 * attachment row, live quota aggregates, and independent R2 head/get calls.
 * The authority tables are intentionally absent from production migrations:
 * production stays unprovisioned and fail-closed until an external review.
 */

import {
  type D2PostOperationReadbackAnchor,
  type D2RawD1ResourceReadback,
  type D2RawQuotaResourceReadback,
  type D2RawR2ResourceReadback,
} from "./d2-0010-authoritative-admission.js";
import {
  D2_DATABASE_ID,
  D2_R2_BUCKET,
  type D2ProbeKind,
} from "./d2-0010-release-contract.js";

const SHA256_RE = /^[0-9a-f]{64}$/;
const PROBE_ID_RE = /^[0-9a-f]{32}$/;

interface AuthorityResourceRow {
  probe_id: string;
  kind: D2ProbeKind;
  transcript_bytes_sha256: string;
  account_sha256: string;
  attachment_id: string;
  object_key: string;
  row_version: number;
  object_version: string;
  etag: string;
  size_bytes: number;
  sha256: string;
}

interface BindingAuthorityRow extends AuthorityResourceRow {
  attachment_row_id: string | null;
  attachment_row_object_key: string | null;
  attachment_row_size_bytes: number | null;
  attachment_row_state: string | null;
  attachment_row_upload_id: string | null;
  quota_account_sha256: string | null;
  counter_version: number;
  reservation_rows: number;
  reservation_bytes: number;
  content_rows: number;
  content_bytes: number;
  live_reservation_rows: number;
  live_reservation_bytes: number;
  live_content_rows: number;
  live_content_bytes: number;
}

export interface D2BindingReadback {
  anchor: D2PostOperationReadbackAnchor;
  bytes: Uint8Array;
  raw: {
    d1: D2RawD1ResourceReadback;
    r2: D2RawR2ResourceReadback;
    quota: D2RawQuotaResourceReadback;
  };
  object: {
    key: string;
    version: string;
    etag: string;
    size: number;
    sha256: string;
  };
  quota: {
    account_sha256: string;
    counter_version: number;
    reservation_rows: number;
    reservation_bytes: number;
    content_rows: number;
    content_bytes: number;
  };
}

function fail(message: string): never {
  throw new Error(`D2 Workerd authority: ${message}`);
}

function exactPositiveInt(value: unknown, label: string): number {
  if (!Number.isSafeInteger(value) || (value as number) <= 0) {
    fail(`${label} must be a positive safe integer`);
  }
  return value as number;
}

function exactNonNegativeInt(value: unknown, label: string): number {
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    fail(`${label} must be a non-negative safe integer`);
  }
  return value as number;
}

function exactDigest(value: unknown, label: string): string {
  if (typeof value !== "string" || !SHA256_RE.test(value)) {
    fail(`${label} must be a lowercase SHA-256 digest`);
  }
  return value;
}

function exactText(value: unknown, label: string): string {
  if (typeof value !== "string" || value.length === 0) {
    fail(`${label} must be nonempty`);
  }
  return value;
}

function canonicalJson(value: unknown): string {
  if (value === null) return "null";
  if (typeof value === "number") {
    if (!Number.isFinite(value)) fail("canonical input contains a non-finite number");
    return JSON.stringify(value);
  }
  if (typeof value === "string" || typeof value === "boolean") {
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (typeof value !== "object") fail("canonical input is not JSON");
  const object = value as Record<string, unknown>;
  return `{${Object.keys(object).sort().map(
    (key) => `${JSON.stringify(key)}:${canonicalJson(object[key])}`,
  ).join(",")}}`;
}

async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return [...digest].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

function base64url(bytes: Uint8Array): string {
  const alphabet =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
  let encoded = "";
  for (let offset = 0; offset < bytes.length; offset += 3) {
    const a = bytes[offset] ?? 0;
    const b = bytes[offset + 1] ?? 0;
    const c = bytes[offset + 2] ?? 0;
    encoded += alphabet[a >> 2];
    encoded += alphabet[((a & 3) << 4) | (b >> 4)];
    if (offset + 1 < bytes.length) {
      encoded += alphabet[((b & 15) << 2) | (c >> 6)];
    }
    if (offset + 2 < bytes.length) encoded += alphabet[c & 63];
  }
  return encoded;
}

async function encodedReadback(value: object): Promise<{
  base64url: string;
  sha256: string;
}> {
  const bytes = new TextEncoder().encode(canonicalJson(value));
  return {
    base64url: base64url(bytes),
    sha256: await sha256Hex(bytes),
  };
}

/**
 * Load an exact-size witness exclusively from real bound resources.
 *
 * `d2_admission_resource_readbacks` is a durable receipt created by an
 * independent post-operation observer. The receipt is necessary because the
 * shipping attachment table does not store R2 version, ETag, or a content
 * digest. Every receipt field is rechecked against the shipping D1 row and
 * independently loaded R2 metadata/body before an anchor is returned.
 */
export async function loadD2ExactReadbackFromBindings(
  db: D1Database,
  bucket: R2Bucket,
  probeId: string,
): Promise<D2BindingReadback | null> {
  if (!PROBE_ID_RE.test(probeId)) fail("probe ID is malformed");
  // One D1 statement gives the observer one internally consistent D1 snapshot:
  // durable authority receipt, shipping attachment row, durable quota version,
  // and independently recomputed live quota aggregates.
  const receipt = await db.prepare(
    `SELECT receipt.probe_id,
            receipt.kind,
            receipt.transcript_bytes_sha256,
            receipt.account_sha256,
            receipt.attachment_id,
            receipt.object_key,
            receipt.row_version,
            receipt.object_version,
            receipt.etag,
            receipt.size_bytes,
            receipt.sha256,
            object_row.id AS attachment_row_id,
            object_row.object_key AS attachment_row_object_key,
            object_row.size_bytes AS attachment_row_size_bytes,
            object_row.state AS attachment_row_state,
            object_row.upload_id AS attachment_row_upload_id,
            quota.account_sha256 AS quota_account_sha256,
            quota.counter_version,
            quota.reservation_rows,
            quota.reservation_bytes,
            quota.content_rows,
            quota.content_bytes,
            COALESCE((
              SELECT SUM(CASE WHEN live.state <> 'ready' THEN 1 ELSE 0 END)
                FROM attachment_objects AS live
            ), 0) AS live_reservation_rows,
            COALESCE((
              SELECT SUM(
                CASE WHEN live.state <> 'ready' THEN live.size_bytes ELSE 0 END
              )
                FROM attachment_objects AS live
            ), 0) AS live_reservation_bytes,
            COALESCE((
              SELECT SUM(CASE WHEN live.state = 'ready' THEN 1 ELSE 0 END)
                FROM attachment_objects AS live
            ), 0) AS live_content_rows,
            COALESCE((
              SELECT SUM(
                CASE WHEN live.state = 'ready' THEN live.size_bytes ELSE 0 END
              )
                FROM attachment_objects AS live
            ), 0) AS live_content_bytes
       FROM d2_admission_resource_readbacks AS receipt
       LEFT JOIN attachment_objects AS object_row
         ON object_row.id = receipt.attachment_id
       LEFT JOIN d2_admission_quota_readbacks AS quota
         ON quota.account_sha256 = receipt.account_sha256
      WHERE receipt.probe_id = ?`,
  ).bind(probeId).first<BindingAuthorityRow>();
  if (!receipt) return null;
  if (
    receipt.probe_id !== probeId
    || receipt.kind !== "exact-size"
    || receipt.attachment_id !== probeId
    || receipt.object_key !== `attachments/${probeId}`
  ) {
    fail("durable receipt identifies the wrong resource");
  }
  const transcriptBytesSha256 = exactDigest(
    receipt.transcript_bytes_sha256,
    "transcript digest",
  );
  const accountSha256 = exactDigest(receipt.account_sha256, "account digest");
  const rowVersion = exactPositiveInt(receipt.row_version, "D1 row version");
  const objectVersion = exactText(receipt.object_version, "R2 object version");
  const etag = exactText(receipt.etag, "R2 ETag");
  const expectedSize = exactPositiveInt(receipt.size_bytes, "receipt size");
  const expectedSha256 = exactDigest(receipt.sha256, "receipt body digest");

  if (
    receipt.attachment_row_id !== receipt.attachment_id
    || receipt.attachment_row_object_key !== receipt.object_key
    || receipt.attachment_row_state !== "ready"
    || receipt.attachment_row_upload_id !== null
    || exactPositiveInt(
      receipt.attachment_row_size_bytes,
      "D1 attachment size",
    ) !== expectedSize
  ) {
    fail("shipping D1 attachment row does not match the durable receipt");
  }

  if (receipt.quota_account_sha256 !== accountSha256) {
    fail("durable quota state is absent or belongs to another account");
  }
  const quota = {
    account_sha256: accountSha256,
    counter_version: exactPositiveInt(
      receipt.counter_version,
      "quota counter version",
    ),
    reservation_rows: exactNonNegativeInt(
      receipt.reservation_rows,
      "reservation rows",
    ),
    reservation_bytes: exactNonNegativeInt(
      receipt.reservation_bytes,
      "reservation bytes",
    ),
    content_rows: exactNonNegativeInt(receipt.content_rows, "content rows"),
    content_bytes: exactNonNegativeInt(
      receipt.content_bytes,
      "content bytes",
    ),
  };
  const liveCounters = {
    reservation_rows: exactNonNegativeInt(
      receipt.live_reservation_rows,
      "live reservation rows",
    ),
    reservation_bytes: exactNonNegativeInt(
      receipt.live_reservation_bytes,
      "live reservation bytes",
    ),
    content_rows: exactNonNegativeInt(
      receipt.live_content_rows,
      "live content rows",
    ),
    content_bytes: exactNonNegativeInt(
      receipt.live_content_bytes,
      "live content bytes",
    ),
  };
  if (
    quota.reservation_rows !== liveCounters.reservation_rows
    || quota.reservation_bytes !== liveCounters.reservation_bytes
    || quota.content_rows !== liveCounters.content_rows
    || quota.content_bytes !== liveCounters.content_bytes
  ) {
    fail("live quota aggregates do not match the durable counter state");
  }

  const [head, body] = await Promise.all([
    bucket.head(receipt.object_key),
    bucket.get(receipt.object_key),
  ]);
  if (!head || !body) fail("R2 object is absent");
  const bytes = new Uint8Array(await body.arrayBuffer());
  if (bytes.byteLength === 0) fail("R2 object is empty");
  const actualSha256 = await sha256Hex(bytes);
  const metadataMatches = (
    head.key === receipt.object_key
    && body.key === receipt.object_key
    && head.version === body.version
    && head.etag === body.etag
    && head.size === body.size
    && head.version === objectVersion
    && head.etag === etag
    && head.size === expectedSize
    && bytes.byteLength === expectedSize
    && actualSha256 === expectedSha256
  );
  if (!metadataMatches) {
    fail("live R2 key/version/etag/size/digest does not match the receipt");
  }

  const d1Readback: D2RawD1ResourceReadback = {
    format: "osl.cipher-store.d2-raw-d1-resource-readback.v2",
    probe_id: probeId,
    kind: receipt.kind,
    database_id: D2_DATABASE_ID,
    attachment_id: receipt.attachment_row_id,
    object_key: receipt.attachment_row_object_key,
    observation: "ready",
    row_state: "ready",
    row_version: rowVersion,
    expected_size_bytes: receipt.attachment_row_size_bytes,
    row_sha256: actualSha256,
  };
  const r2Readback: D2RawR2ResourceReadback = {
    format: "osl.cipher-store.d2-raw-r2-resource-readback.v2",
    probe_id: probeId,
    kind: receipt.kind,
    bucket_name: D2_R2_BUCKET,
    object_key: head.key,
    observation: "exact",
    object_version: head.version,
    etag: head.etag,
    size_bytes: head.size,
    sha256: actualSha256,
  };
  const counters = {
    counter_version: quota.counter_version,
    reservation_rows: quota.reservation_rows,
    reservation_bytes: quota.reservation_bytes,
    content_rows: quota.content_rows,
    content_bytes: quota.content_bytes,
  };
  const quotaReadback: D2RawQuotaResourceReadback = {
    format: "osl.cipher-store.d2-raw-quota-resource-readback.v2",
    probe_id: probeId,
    kind: receipt.kind,
    account_sha256: accountSha256,
    observation: "retained",
    ...counters,
    counters_sha256: await sha256Hex(
      new TextEncoder().encode(canonicalJson(counters)),
    ),
  };
  const [d1Encoded, r2Encoded, quotaEncoded] = await Promise.all([
    encodedReadback(d1Readback),
    encodedReadback(r2Readback),
    encodedReadback(quotaReadback),
  ]);
  return {
    anchor: {
      probe_id: probeId,
      kind: receipt.kind,
      transcript_bytes_sha256: transcriptBytesSha256,
      d1_readback_base64url: d1Encoded.base64url,
      d1_readback_sha256: d1Encoded.sha256,
      r2_readback_base64url: r2Encoded.base64url,
      r2_readback_sha256: r2Encoded.sha256,
      quota_readback_base64url: quotaEncoded.base64url,
      quota_readback_sha256: quotaEncoded.sha256,
    },
    bytes,
    raw: {
      d1: d1Readback,
      r2: r2Readback,
      quota: quotaReadback,
    },
    object: {
      key: head.key,
      version: head.version,
      etag: head.etag,
      size: head.size,
      sha256: actualSha256,
    },
    quota,
  };
}
