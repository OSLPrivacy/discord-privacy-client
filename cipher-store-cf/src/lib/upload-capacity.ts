import type { Env } from "../env.js";
import { base64UrlToBytes } from "./digest.js";

export const UPLOAD_REQUEST_FLOOR_BYTES = 1_000_000;
export const UPLOAD_GRANT_SCHEME = "OSL-Upload-Grant";
export const UPLOAD_RESERVATION_SCHEME = "OSL-Upload-Reservation";
export const UPLOAD_GRANT_DOMAIN = "OSL-UPLOAD-GRANT-v1";
export const UPLOAD_RESERVATION_DOMAIN = "OSL-UPLOAD-RESERVATION-v1";
export type UploadAuthority = "monthly-included" | "sold";

const ID_RE = /^[0-9a-f]{32}$/;
const RESERVATION_KEYS = [
  "schema", "authority", "reservationId", "grantId", "capacityBytes",
  "baseExpiry", "effectiveExpiry", "outageExtensionSeconds",
] as const;
const GRANT_KEYS = ["schema", "aud", ...RESERVATION_KEYS.slice(1)] as const;

type CommonClaims = {
  authority: UploadAuthority;
  reservationId: string;
  grantId: string;
  capacityBytes: number;
  baseExpiry: number;
  effectiveExpiry: number;
  outageExtensionSeconds: number;
};

export type VerifiedUploadGrant = CommonClaims & {
  payloadText: string;
};

export type CapacityClaim = {
  objectId: string;
  grantId: string;
  debitBytes: number;
  ciphertextBytes: number;
  state: "pending" | "accepted";
  classAWrites: number;
  isNew: boolean;
};

export type CapacityRefusal = { ok: false; status: number; code: string; message: string };
export type CapacityResult<T> = { ok: true; value: T } | CapacityRefusal;

const refusal = (status: number, code: string, message: string): CapacityRefusal =>
  ({ ok: false, status, code, message });

function exactKeys(value: unknown, expected: readonly string[]): value is Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const actual = Object.keys(value as object).sort();
  return actual.length === expected.length
    && actual.every((key, index) => key === [...expected].sort()[index]);
}

function authorityKey(env: Env, authority: UploadAuthority): string | undefined {
  return authority === "monthly-included"
    ? env.MONTHLY_UPLOAD_AUTHORITY_PUBKEY_B64
    : env.SOLD_UPLOAD_AUTHORITY_PUBKEY_B64;
}

function domainSeparated(domainText: string, payload: Uint8Array): Uint8Array {
  const domain = new TextEncoder().encode(domainText);
  const output = new Uint8Array(domain.byteLength + 1 + payload.byteLength);
  output.set(domain);
  output[domain.byteLength] = 0;
  output.set(payload, domain.byteLength + 1);
  return output;
}

async function signedPayload(
  request: Request,
  env: Env,
  scheme: string,
  domain: string,
  expectedKeys: readonly string[],
): Promise<CapacityResult<{ claims: Record<string, unknown>; payloadText: string }>> {
  const header = request.headers.get("authorization");
  if (!header?.startsWith(`${scheme} `)) return refusal(401, "upload_authority_required", "typed upload authority required");
  const parts = header.slice(scheme.length + 1).trim().split(".");
  if (parts.length !== 2) return refusal(401, "upload_authority_malformed", "typed upload authority malformed");
  const payload = base64UrlToBytes(parts[0] ?? "");
  const signature = base64UrlToBytes(parts[1] ?? "");
  if (!payload || payload.byteLength === 0 || payload.byteLength > 2048 || !signature || signature.byteLength !== 64) {
    return refusal(401, "upload_authority_malformed", "typed upload authority malformed");
  }
  let claims: unknown;
  try { claims = JSON.parse(new TextDecoder().decode(payload)); } catch {
    return refusal(401, "upload_authority_malformed", "typed upload authority malformed");
  }
  if (!exactKeys(claims, expectedKeys)) return refusal(401, "upload_authority_claims", "typed upload authority claims refused");
  const authority = claims.authority;
  if (authority !== "monthly-included" && authority !== "sold") {
    return refusal(401, "upload_authority_type", "upload authority type refused");
  }
  const configured = authorityKey(env, authority);
  const rawKey = configured ? base64UrlToBytes(configured.replace(/=+$/, "")) : null;
  if (!rawKey || rawKey.byteLength !== 32) return refusal(503, "upload_authority_unconfigured", "upload authority unavailable");
  try {
    const key = await crypto.subtle.importKey("raw", rawKey, { name: "Ed25519" }, false, ["verify"]);
    const valid = await crypto.subtle.verify(
      { name: "Ed25519" }, key, signature, domainSeparated(domain, payload),
    );
    if (!valid) return refusal(401, "upload_authority_signature", "upload authority signature refused");
  } catch {
    return refusal(503, "upload_authority_unconfigured", "upload authority unavailable");
  }
  return { ok: true, value: { claims, payloadText: new TextDecoder().decode(payload) } };
}

function commonClaims(value: Record<string, unknown>): CommonClaims | null {
  const authority = value.authority;
  const reservationId = value.reservationId;
  const grantId = value.grantId;
  const capacityBytes = value.capacityBytes;
  const baseExpiry = value.baseExpiry;
  const effectiveExpiry = value.effectiveExpiry;
  const outageExtensionSeconds = value.outageExtensionSeconds;
  if ((authority !== "monthly-included" && authority !== "sold")
    || typeof reservationId !== "string" || !ID_RE.test(reservationId)
    || typeof grantId !== "string" || !ID_RE.test(grantId)
    || typeof capacityBytes !== "number" || !Number.isSafeInteger(capacityBytes) || capacityBytes <= 0
    || typeof baseExpiry !== "number" || !Number.isSafeInteger(baseExpiry)
    || typeof effectiveExpiry !== "number" || !Number.isSafeInteger(effectiveExpiry)
    || typeof outageExtensionSeconds !== "number" || !Number.isSafeInteger(outageExtensionSeconds)
    || outageExtensionSeconds < 0 || effectiveExpiry !== baseExpiry + outageExtensionSeconds) return null;
  return { authority, reservationId, grantId, capacityBytes, baseExpiry, effectiveExpiry, outageExtensionSeconds };
}

export async function registerUploadReservation(request: Request, env: Env): Promise<CapacityResult<CommonClaims>> {
  const signed = await signedPayload(request, env, UPLOAD_RESERVATION_SCHEME, UPLOAD_RESERVATION_DOMAIN, RESERVATION_KEYS);
  if (!signed.ok) return signed;
  const claims = commonClaims(signed.value.claims);
  const schema = signed.value.claims.schema;
  const expectedSchema = claims?.authority === "sold"
    ? "osl-sold-upload-reservation-v1"
    : "osl-monthly-upload-reservation-v1";
  if (!claims || schema !== expectedSchema) return refusal(401, "upload_reservation_type", "upload reservation type refused");
  const now = Math.floor(Date.now() / 1000);
  try {
    await env.DB.prepare(`INSERT INTO upload_capacity_reservations
      (reservation_id,grant_id,authority,capacity_bytes,spent_bytes,base_expiry,
       effective_expiry,outage_extension_seconds,signed_reservation,created_at)
      VALUES(?,?,?,?,0,?,?,?,?,?) ON CONFLICT(reservation_id) DO NOTHING`).bind(
      claims.reservationId, claims.grantId, claims.authority, claims.capacityBytes,
      claims.baseExpiry, claims.effectiveExpiry, claims.outageExtensionSeconds,
      signed.value.payloadText, now,
    ).run();
    const row = await env.DB.prepare(`SELECT grant_id,authority,capacity_bytes,base_expiry,
      effective_expiry,outage_extension_seconds,signed_reservation
      FROM upload_capacity_reservations WHERE reservation_id=?`).bind(claims.reservationId).first<Record<string, unknown>>();
    const exact = row?.grant_id === claims.grantId && row.authority === claims.authority
      && Number(row.capacity_bytes) === claims.capacityBytes && Number(row.base_expiry) === claims.baseExpiry
      && Number(row.effective_expiry) === claims.effectiveExpiry
      && Number(row.outage_extension_seconds) === claims.outageExtensionSeconds
      && row.signed_reservation === signed.value.payloadText;
    if (!exact) return refusal(409, "upload_reservation_mismatch", "upload reservation mismatch");
    return { ok: true, value: claims };
  } catch {
    return refusal(409, "upload_reservation_mismatch", "upload reservation mismatch");
  }
}

export async function verifyUploadGrant(request: Request, env: Env): Promise<CapacityResult<VerifiedUploadGrant>> {
  const signed = await signedPayload(request, env, UPLOAD_GRANT_SCHEME, UPLOAD_GRANT_DOMAIN, GRANT_KEYS);
  if (!signed.ok) return signed;
  const claims = commonClaims(signed.value.claims);
  if (!claims || signed.value.claims.schema !== "osl-upload-grant-v1" || signed.value.claims.aud !== "osl-cipher-store") {
    return refusal(401, "upload_grant_type", "typed upload grant refused");
  }
  const row = await env.DB.prepare(`SELECT reservation_id,grant_id,authority,capacity_bytes,
    base_expiry,effective_expiry,outage_extension_seconds FROM upload_capacity_reservations
    WHERE reservation_id=? AND grant_id=?`).bind(claims.reservationId, claims.grantId).first<Record<string, unknown>>();
  const matches = row?.authority === claims.authority
    && Number(row.capacity_bytes) === claims.capacityBytes
    && Number(row.base_expiry) === claims.baseExpiry
    && Number(row.effective_expiry) === claims.effectiveExpiry
    && Number(row.outage_extension_seconds) === claims.outageExtensionSeconds;
  if (!matches) return refusal(401, "upload_reservation_required", "exact upload reservation required");
  return { ok: true, value: { ...claims, payloadText: signed.value.payloadText } };
}

export async function claimUploadCapacity(
  env: Env,
  grant: VerifiedUploadGrant,
  objectId: string,
  ciphertextBytes: number,
): Promise<CapacityResult<CapacityClaim>> {
  const debitBytes = Math.max(ciphertextBytes, UPLOAD_REQUEST_FLOOR_BYTES);
  const now = Math.floor(Date.now() / 1000);
  const inserted = await env.DB.prepare(`INSERT INTO upload_capacity_claims
    (object_id,grant_id,debit_bytes,ciphertext_bytes,state,class_a_writes,claimed_at,accepted_at)
    SELECT ?,r.grant_id,?,?,'pending',0,?,NULL
      FROM upload_capacity_reservations r
     WHERE r.grant_id=? AND r.reservation_id=? AND r.authority=?
       AND r.capacity_bytes=? AND r.effective_expiry=? AND r.effective_expiry>?
       AND r.capacity_bytes-r.spent_bytes-
         COALESCE((SELECT SUM(c.debit_bytes) FROM upload_capacity_claims c
                   WHERE c.grant_id=r.grant_id AND c.state='pending'),0) >= ?
    ON CONFLICT(object_id) DO NOTHING`).bind(
    objectId, debitBytes, ciphertextBytes, now, grant.grantId, grant.reservationId,
    grant.authority, grant.capacityBytes, grant.effectiveExpiry, now, debitBytes,
  ).run();
  const row = await env.DB.prepare(`SELECT object_id,grant_id,debit_bytes,ciphertext_bytes,
    state,class_a_writes FROM upload_capacity_claims WHERE object_id=?`).bind(objectId).first<Record<string, unknown>>();
  if (!row || row.grant_id !== grant.grantId || Number(row.debit_bytes) !== debitBytes
    || Number(row.ciphertext_bytes) !== ciphertextBytes) {
    return refusal(402, "upload_capacity_exhausted", "signed upload capacity exhausted");
  }
  return { ok: true, value: {
    objectId, grantId: grant.grantId, debitBytes, ciphertextBytes,
    state: row.state as "pending" | "accepted", classAWrites: Number(row.class_a_writes),
    isNew: (inserted.meta?.changes ?? 0) === 1,
  } };
}

export async function capacityReceipt(env: Env, grantId: string): Promise<{
  signedBytes: number; spentBytes: number; heldBytes: number; remainingBytes: number;
}> {
  const row = await env.DB.prepare(`SELECT r.capacity_bytes,r.spent_bytes,
    COALESCE((SELECT SUM(c.debit_bytes) FROM upload_capacity_claims c
      WHERE c.grant_id=r.grant_id AND c.state='pending'),0) AS held_bytes
    FROM upload_capacity_reservations r WHERE r.grant_id=?`).bind(grantId).first<Record<string, unknown>>();
  if (!row) throw new Error("upload reservation disappeared");
  const signedBytes = Number(row.capacity_bytes);
  const spentBytes = Number(row.spent_bytes);
  const heldBytes = Number(row.held_bytes);
  return { signedBytes, spentBytes, heldBytes, remainingBytes: signedBytes - spentBytes - heldBytes };
}
