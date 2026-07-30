/// Link-creation authorization.
///
/// ## Why this exists
///
/// A one-time link store with no creation gate is an open, logless,
/// self-deleting file host -- which is a malware distribution service.
/// That is the thing that gets a domain Safe-Browsing-flagged, gets the
/// URL filtered by Discord, and gets enforcement taken against the
/// *account* -- which would also take down the cipher-store and the
/// keyserver that the strong OSL lane depends on. Creation is therefore
/// restricted; retrieval is not (the recipient is by definition not an
/// OSL user).
///
/// ## What a grant is
///
/// An **anonymous, short-lived, single-use** Ed25519-signed token issued
/// by the keyserver to a client that has completed OSL identity
/// registration. It deliberately carries no user id: the cipher-store
/// must not learn who created a link, only that *some* vouched client
/// did. Enforcement handles (revoking a bad actor) live at the
/// keyserver, which already knows identities; the cipher-store stays
/// identity-free.
///
/// Wire format:
///   Authorization: OSL-Link-Grant <base64url(payload)>.<base64url(sig)>
///   payload = UTF-8 JSON {"aud":"osl-link-create","exp":<unix>,"jti":"<32 hex>"}
///   sig     = Ed25519( "OSL-LINK-GRANT-v1" || 0x00 || payloadBytes )
///
/// The 0x00 separator cannot occur inside the ASCII domain string, so
/// the signed message is unambiguous.
///
/// ## Fail-closed
///
/// If `LINK_GRANT_PUBKEY_B64` is not configured, creation is REFUSED.
/// An unconfigured verifier must never degrade to an open store.

import type { Env } from "../env.js";
import { base64UrlToBytes } from "./digest.js";

export const GRANT_SCHEME = "OSL-Link-Grant";
export const GRANT_AUDIENCE = "osl-link-create";
export const GRANT_DOMAIN = "OSL-LINK-GRANT-v1";
/// A grant may not be minted further ahead than this. Bounds the damage
/// from a grant captured in transit or lifted off a compromised client.
export const MAX_GRANT_LIFETIME_SECONDS = 600;

export type GrantResult =
  | { ok: true }
  | { ok: false; status: number; code: string; message: string };

const unauthorized = (code: string, message: string): GrantResult => ({
  ok: false,
  status: 401,
  code,
  message,
});

function domainSeparated(payload: Uint8Array): Uint8Array {
  const domain = new TextEncoder().encode(GRANT_DOMAIN);
  const out = new Uint8Array(domain.byteLength + 1 + payload.byteLength);
  out.set(domain, 0);
  out[domain.byteLength] = 0x00;
  out.set(payload, domain.byteLength + 1);
  return out;
}

/// Verifies the grant and consumes its `jti` so the same grant cannot
/// mint two links. Returns `{ ok: true }` only when every check passes.
export async function verifyLinkGrant(
  request: Request,
  env: Env,
): Promise<GrantResult> {
  const configured = env.LINK_GRANT_PUBKEY_B64;
  if (!configured) {
    // Deliberate: no key, no link creation. The alternative -- an open
    // creation route -- is the failure mode this whole module prevents.
    return {
      ok: false,
      status: 503,
      code: "link_creation_unconfigured",
      message: "link creation is not enabled on this deployment",
    };
  }
  const publicKey = base64UrlToBytes(configured.replace(/=+$/, ""));
  if (!publicKey || publicKey.byteLength !== 32) {
    return {
      ok: false,
      status: 503,
      code: "link_creation_unconfigured",
      message: "link creation is not enabled on this deployment",
    };
  }

  const header = request.headers.get("authorization");
  if (!header || !header.startsWith(GRANT_SCHEME + " ")) {
    return unauthorized("grant_required", "link creation requires an OSL grant");
  }
  const parts = header.slice(GRANT_SCHEME.length + 1).trim().split(".");
  if (parts.length !== 2) {
    return unauthorized("grant_malformed", "grant must be payload.signature");
  }
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
  if (claims.aud !== GRANT_AUDIENCE) {
    return unauthorized("grant_audience", "grant is not for link creation");
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
  if (exp <= now) {
    return unauthorized("grant_expired", "grant has expired");
  }
  if (exp - now > MAX_GRANT_LIFETIME_SECONDS) {
    return unauthorized("grant_lifetime", "grant lifetime exceeds the cap");
  }

  let verified = false;
  try {
    const key = await crypto.subtle.importKey(
      "raw",
      publicKey,
      { name: "Ed25519" },
      false,
      ["verify"],
    );
    verified = await crypto.subtle.verify(
      { name: "Ed25519" },
      key,
      signature,
      domainSeparated(payloadBytes),
    );
  } catch {
    console.error("[link-grant] verify unavailable");
    return {
      ok: false,
      status: 503,
      code: "link_creation_unconfigured",
      message: "link creation is not enabled on this deployment",
    };
  }
  if (!verified) {
    return unauthorized("grant_signature", "grant signature did not verify");
  }

  // Single use. The jti record is transient (expires with the grant plus
  // sweep slack) and carries no identity -- it exists only to stop replay.
  // INSERT success is the claim; there is deliberately no prior read.
  try {
    const consumed = await env.DB.prepare(
      `INSERT INTO link_grant_consumed (jti, expires_at)
       VALUES (?, ?)
       ON CONFLICT(jti) DO NOTHING
       RETURNING jti`,
    )
      .bind(jti, exp + 60)
      .first<{ jti: string }>();
    if (!consumed) {
      return unauthorized("grant_replay", "grant has already been used");
    }
  } catch {
    // Fail closed: without replay suppression a single captured grant
    // becomes an unbounded creation capability.
    console.error("[link-grant] replay store unavailable");
    return {
      ok: false,
      status: 503,
      code: "grant_store_unavailable",
      message: "link creation is temporarily unavailable",
    };
  }
  return { ok: true };
}
