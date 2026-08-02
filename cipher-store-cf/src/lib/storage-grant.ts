/// Anonymous, one-time admission grants for the OSL blob store.
///
/// The keyserver decides whether to issue these grants; this Worker learns
/// only that a request carries a fresh, valid grant.  The signed payload is
/// deliberately limited to `aud`, `exp`, and `jti` so identity and tier never
/// cross that boundary.

import type { Env } from "../env.js";
import { base64UrlToBytes } from "./digest.js";

export const STORAGE_GRANT_SCHEME = "OSL-Link-Grant";
export const STORAGE_GRANT_AUDIENCE = "osl-blob-store";
export const STORAGE_GRANT_DOMAIN = "OSL-LINK-GRANT-v1";
export const MAX_STORAGE_GRANT_LIFETIME_SECONDS = 600;

export type StorageGrantResult =
  | { ok: true }
  | { ok: false; status: number; code: string; message: string };

const unauthorized = (code: string, message: string): StorageGrantResult => ({
  ok: false,
  status: 401,
  code,
  message,
});

function domainSeparated(payload: Uint8Array): Uint8Array {
  const domain = new TextEncoder().encode(STORAGE_GRANT_DOMAIN);
  const out = new Uint8Array(domain.byteLength + 1 + payload.byteLength);
  out.set(domain, 0);
  out[domain.byteLength] = 0;
  out.set(payload, domain.byteLength + 1);
  return out;
}

/// Verify and atomically consume an upload-admission grant.  INSERT success
/// is the claim, rather than a read followed by a write, so concurrent replay
/// attempts cannot both win.
export async function verifyStorageGrant(
  request: Request,
  env: Env,
): Promise<StorageGrantResult> {
  const configured = env.LINK_GRANT_PUBKEY_B64;
  if (!configured) {
    return {
      ok: false,
      status: 503,
      code: "storage_grant_unconfigured",
      message: "blob upload is not enabled on this deployment",
    };
  }
  const publicKey = base64UrlToBytes(configured.replace(/=+$/, ""));
  if (!publicKey || publicKey.byteLength !== 32) {
    return {
      ok: false,
      status: 503,
      code: "storage_grant_unconfigured",
      message: "blob upload is not enabled on this deployment",
    };
  }

  const header = request.headers.get("authorization");
  if (!header || !header.startsWith(STORAGE_GRANT_SCHEME + " ")) {
    return unauthorized("grant_required", "blob upload requires an OSL grant");
  }
  const parts = header.slice(STORAGE_GRANT_SCHEME.length + 1).trim().split(".");
  if (parts.length !== 2) return unauthorized("grant_malformed", "grant must be payload.signature");
  const payloadBytes = base64UrlToBytes(parts[0] ?? "");
  const signature = base64UrlToBytes(parts[1] ?? "");
  if (!payloadBytes || payloadBytes.byteLength === 0 || payloadBytes.byteLength > 512) {
    return unauthorized("grant_malformed", "grant payload is malformed");
  }
  if (!signature || signature.byteLength !== 64) {
    return unauthorized("grant_malformed", "grant signature is malformed");
  }

  let claims: { aud?: unknown; exp?: unknown; jti?: unknown };
  try {
    claims = JSON.parse(new TextDecoder().decode(payloadBytes));
  } catch {
    return unauthorized("grant_malformed", "grant payload is not JSON");
  }
  if (
    !claims ||
    typeof claims !== "object" ||
    Object.keys(claims).length !== 3 ||
    !["aud", "exp", "jti"].every((key) => Object.hasOwn(claims, key))
  ) {
    return unauthorized("grant_claims", "grant claims are malformed");
  }
  if (claims.aud !== STORAGE_GRANT_AUDIENCE) {
    return unauthorized("grant_audience", "grant is not for blob upload");
  }
  const exp = claims.exp;
  const jti = claims.jti;
  if (typeof exp !== "number" || !Number.isSafeInteger(exp)) {
    return unauthorized("grant_malformed", "grant exp is malformed");
  }
  if (typeof jti !== "string" || !/^[0-9a-f]{32}$/.test(jti)) {
    return unauthorized("grant_malformed", "grant jti is malformed");
  }
  const now = Math.floor(Date.now() / 1000);
  if (exp <= now) return unauthorized("grant_expired", "grant has expired");
  if (exp - now > MAX_STORAGE_GRANT_LIFETIME_SECONDS) {
    return unauthorized("grant_lifetime", "grant lifetime exceeds the cap");
  }

  let verified = false;
  try {
    const key = await crypto.subtle.importKey("raw", publicKey, { name: "Ed25519" }, false, ["verify"]);
    verified = await crypto.subtle.verify({ name: "Ed25519" }, key, signature, domainSeparated(payloadBytes));
  } catch {
    console.error("[storage-grant] verify unavailable");
    return {
      ok: false,
      status: 503,
      code: "storage_grant_unconfigured",
      message: "blob upload is not enabled on this deployment",
    };
  }
  if (!verified) return unauthorized("grant_signature", "grant signature did not verify");

  try {
    const consumed = await env.DB.prepare(
      `INSERT INTO storage_grant_consumed (jti, expires_at)
       VALUES (?, ?)
       ON CONFLICT(jti) DO NOTHING
       RETURNING jti`,
    ).bind(jti, exp + 60).first<{ jti: string }>();
    if (!consumed) return unauthorized("grant_replay", "grant has already been used");
  } catch {
    console.error("[storage-grant] replay store unavailable");
    return {
      ok: false,
      status: 503,
      code: "grant_store_unavailable",
      message: "blob upload is temporarily unavailable",
    };
  }
  return { ok: true };
}
