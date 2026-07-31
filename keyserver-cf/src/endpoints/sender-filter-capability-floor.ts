import type { Env } from "../env.js";
import { canonicalSenderFilterFloorGetBytes } from "../lib/canonical.js";
import {
  CONTROL_INBOX_DISPOSITION_CAPABILITY,
} from "../lib/control-inbox-sweep.js";
import { verifyEd25519 } from "../lib/crypto.js";
import { getUserForVerify } from "../lib/db.js";
import {
  badRequest,
  json,
  notFound,
  serviceUnavailable,
  unauthorized,
} from "../lib/http.js";
import { decodeBase64, isProtocolId } from "../lib/validation.js";

const FLOOR_FORMAT = "osl.keyserver.sender-filter-capability-floor.v3";
const FLOOR_CAPABILITY_VERSION = 1;
const FLOOR_IDENTITY_ANCHOR_DOMAIN =
  "OSL-SENDER-FILTER-FLOOR-IDENTITY-v1\u0000";
const FRESHNESS_WINDOW_MS = 5 * 60 * 1000;
const REQUEST_ID_RE = /^[A-Za-z0-9_-]{43}$/;

interface FloorRow {
  identity_anchor_sha256: string;
  capability_version: number;
  monotonic_version: number;
  first_observed_at_ms: number;
}

function lp(value: Uint8Array): Uint8Array {
  const output = new Uint8Array(4 + value.length);
  new DataView(output.buffer).setUint32(0, value.length, false);
  output.set(value, 4);
  return output;
}

function concat(parts: Uint8Array[]): Uint8Array {
  const length = parts.reduce((total, part) => total + part.length, 0);
  const output = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) {
    output.set(part, offset);
    offset += part.length;
  }
  return output;
}

function decodeBase64Exact(value: string, length: number): Uint8Array | null {
  try {
    const bytes = decodeBase64(value);
    return bytes.length === length ? bytes : null;
  } catch {
    return null;
  }
}

async function identityAnchorSha256(
  userId: string,
  publicKey: Uint8Array,
): Promise<string> {
  const encoder = new TextEncoder();
  const bytes = concat([
    lp(encoder.encode(FLOOR_IDENTITY_ANCHOR_DOMAIN)),
    lp(encoder.encode(userId)),
    lp(publicKey),
  ]);
  const digest = new Uint8Array(
    await crypto.subtle.digest("SHA-256", bytes),
  );
  return Array.from(digest, (byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

function exactFloorRow(value: unknown, expectedAnchor: string): FloorRow | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const row = value as Record<string, unknown>;
  if (
    Object.keys(row).sort().join(",") !==
      "capability_version,first_observed_at_ms,identity_anchor_sha256,monotonic_version" ||
    row.identity_anchor_sha256 !== expectedAnchor ||
    row.capability_version !== FLOOR_CAPABILITY_VERSION ||
    row.monotonic_version !== 1 ||
    !Number.isSafeInteger(row.first_observed_at_ms) ||
    (row.first_observed_at_ms as number) <= 0
  ) {
    return null;
  }
  return row as unknown as FloorRow;
}

async function liveSenderFilterSchemaVersionOne(
  db: D1Database,
): Promise<boolean> {
  try {
    const marker = await db.prepare(
      `SELECT version
         FROM worker_schema_capabilities
        WHERE capability = ?`,
    ).bind(CONTROL_INBOX_DISPOSITION_CAPABILITY)
      .first<{ version: number }>();
    if (marker?.version !== FLOOR_CAPABILITY_VERSION) return false;
    await db.prepare(
      `SELECT delivery_status,
              delivery_reason,
              delivery_attempts,
              sender_disabled_first_seen_at,
              delivery_next_retry_at,
              delivery_retain_until
         FROM control_inbox
        LIMIT 0`,
    ).all();
    return true;
  } catch {
    return false;
  }
}

/**
 * Fresh, identity-authenticated observation of the D1-owned capability floor.
 *
 * The request supplies no capability, prior floor, state path, or genesis bit.
 * The Worker derives version 1 from its own live migration-0031 schema check,
 * then inserts an immutable identity-bound record in migration 0032's table.
 */
export async function handleSenderFilterCapabilityFloorGet(
  request: Request,
  env: Env,
  userId: string,
): Promise<Response> {
  const url = new URL(request.url);
  const timestampText = url.searchParams.get("ts") ?? "";
  const timestampMs = Number(timestampText);
  const requestId = url.searchParams.get("request_id") ?? "";
  const signatureB64 = url.searchParams.get("sig") ?? "";
  if (
    !isProtocolId(userId) ||
    !Number.isSafeInteger(timestampMs) ||
    timestampMs <= 0 ||
    Math.abs(Date.now() - timestampMs) > FRESHNESS_WINDOW_MS ||
    !REQUEST_ID_RE.test(requestId) ||
    signatureB64.length === 0
  ) {
    return badRequest("sender-filter floor request is malformed or stale");
  }

  const user = await getUserForVerify(env.DB, userId);
  if (!user) return notFound();
  const publicKey = decodeBase64Exact(user.ik_ed25519_pub, 32);
  const signature = decodeBase64Exact(signatureB64, 64);
  if (!publicKey) return unauthorized("registered identity key is unusable");
  if (!signature) return badRequest("signature must be exact 64-byte base64");
  const canonical = canonicalSenderFilterFloorGetBytes({
    user_id: userId,
    timestamp_ms: timestampMs,
    request_id: requestId,
  });
  if (!(await verifyEd25519(publicKey, canonical, signature))) {
    return unauthorized("signature verification failed");
  }

  // This is the non-caller genesis authority. A client cannot make the
  // capability true: the Worker rechecks marker + columns in its own D1.
  if (!(await liveSenderFilterSchemaVersionOne(env.DB))) {
    return serviceUnavailable("sender-filter capability schema unavailable");
  }

  try {
    const identityAnchor = await identityAnchorSha256(userId, publicKey);
    const results = await env.DB.batch([
      env.DB.prepare(
        `INSERT INTO sender_filter_capability_floors (
           identity_anchor_sha256,
           capability_version,
           monotonic_version,
           first_observed_at_ms
         ) VALUES (?, 1, 1, ?)
         ON CONFLICT(identity_anchor_sha256) DO NOTHING`,
      ).bind(identityAnchor, timestampMs),
      env.DB.prepare(
        `SELECT identity_anchor_sha256,
                capability_version,
                monotonic_version,
                first_observed_at_ms
           FROM sender_filter_capability_floors
          WHERE identity_anchor_sha256 = ?`,
      ).bind(identityAnchor),
    ]);
    const row = exactFloorRow(results[1]?.results?.[0], identityAnchor);
    if (!row) {
      return serviceUnavailable("sender-filter capability floor unavailable");
    }
    return json({
      format: FLOOR_FORMAT,
      recipient_user_id: userId,
      identity_anchor_sha256: row.identity_anchor_sha256,
      capability_version: row.capability_version,
      monotonic_version: row.monotonic_version,
      first_observed_at_ms: row.first_observed_at_ms,
      request_timestamp_ms: timestampMs,
      request_id: requestId,
    });
  } catch {
    console.error("[sender-filter capability floor GET] failed");
    return serviceUnavailable("sender-filter capability floor unavailable");
  }
}
