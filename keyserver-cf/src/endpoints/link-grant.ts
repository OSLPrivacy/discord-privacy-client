/// POST /v1/link-grant — issue one anonymous, single-use authorization
/// for a view-once link creation at the cipher-store.
///
/// ## Why this endpoint exists at all
///
/// `cipher-store-cf`'s `POST /v1/link` refuses every request with 503
/// until `LINK_GRANT_PUBKEY_B64` is installed, and with 401 unless the
/// caller presents a signed grant. That is deliberate: a one-time link
/// store with no creation gate is an open, logless, self-deleting file
/// host, which is a malware distribution service, which is what gets the
/// domain Safe-Browsing-flagged and the Cloudflare account — including
/// the keyserver the strong OSL lane depends on — taken down.
///
/// This is the only issuer of those grants.
///
/// ## The privacy split, which is the whole point
///
/// The request is *identified*: signed by a registered OSL identity, so
/// this Worker knows exactly who asked, can rate-limit them, and can
/// refuse a bad actor.
///
/// The grant is *anonymous*: `aud` / `exp` / `jti`, nothing else. The
/// cipher-store learns only that some vouched client asked.
///
/// So the keyserver knows who but never sees the link, and the
/// cipher-store sees the link but never knows who. Correlating a person
/// to a link requires compromising both, and neither one alone can be
/// compelled to produce the join. Do not "improve" this by putting a
/// user id, a hashed user id, or a per-user counter in the grant —
/// `cipher-store-cf/test/link-grant.test.ts` pins the claim set for
/// exactly that reason.
///
/// ## Fail-closed matrix
///
///   no issuer key / unusable key / mismatched pair  -> 503
///   unregistered identity                           -> 401
///   stale or future timestamp                       -> 401
///   bad signature                                   -> 401
///   replayed request_id                             -> 409
///   over the per-minute or per-day budget           -> 429
///
/// Every one of those refuses to mint. There is no degraded mode.

import type { Env } from "../env.js";
import {
  canonicalLinkGrantBytes,
  LINK_GRANT_FRESHNESS_WINDOW_MS,
} from "../lib/canonical.js";
import { verifyEd25519 } from "../lib/crypto.js";
import { getUserForVerify } from "../lib/db.js";
import {
  badRequest,
  conflict,
  json,
  serviceUnavailable,
  tooMany,
  unauthorized,
} from "../lib/http.js";
import { loadIssuerKey, mintGrant } from "../lib/link-grant-issuer.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import {
  decodeBase64,
  isHighEntropyRequestId,
  isNonEmptyBase64,
  isProtocolId,
} from "../lib/validation.js";

/// Per-minute native limits. These are Cloudflare's permissive,
/// eventually-consistent counters — abuse control, not authorization.
/// The durable bound is the daily quota below.
const GRANT_IP_PER_MINUTE = 10;
const GRANT_IDENTITY_PER_MINUTE = 5;

/// The durable per-identity ceiling, enforced at the D1 write boundary
/// by a trigger so concurrent requests cannot race past it.
///
/// 50/day is far above any plausible human use of a lane whose whole
/// purpose is "the recipient does not have OSL", and far below what
/// makes the store attractive as a file host. It bounds ONE identity;
/// the anti-Sybil bound is the per-IP limit here plus the per-IP limit
/// on `/v1/register`, since registration is open by design.
export const LINK_GRANT_DAILY_MAX = 50;

/// How long a spent `request_id` is remembered. Comfortably longer than
/// the freshness window, so a replay is always caught by the receipt
/// rather than by the clock.
const RECEIPT_TTL_SECONDS = 24 * 60 * 60;

const DAY_SECONDS = 24 * 60 * 60;

export async function handleLinkGrant(request: Request, env: Env): Promise<Response> {
  // Cheapest guard first: a flood should not reach D1 or the key.
  const ipLimit = await checkRateLimit(env, callerIp(request), GRANT_IP_PER_MINUTE, "link-grant-ip");
  if (!ipLimit.ok) return tooMany(ipLimit.retryAfter);

  // Before anything else that could look like a partial success: if this
  // deployment has no usable issuer key, say so plainly.
  const issuer = await loadIssuerKey(env);
  if (!issuer) {
    return serviceUnavailable("link grant issuance is not enabled on this deployment");
  }

  let body: Record<string, unknown>;
  try {
    body = (await request.json()) as Record<string, unknown>;
  } catch {
    return badRequest("malformed JSON body");
  }

  const userId = body.user_id;
  const timestampMs = body.timestamp_ms;
  const requestId = body.request_id;
  const signatureB64 = body.signature_b64;
  if (!isProtocolId(userId)) return badRequest("user_id must be a bounded identifier");
  if (!isHighEntropyRequestId(requestId)) {
    return badRequest("request_id must be a 256-bit base64url value");
  }
  if (!isNonEmptyBase64(signatureB64)) return badRequest("signature_b64 required");
  if (
    typeof timestampMs !== "number" ||
    !Number.isSafeInteger(timestampMs) ||
    timestampMs <= 0 ||
    Math.abs(Date.now() - timestampMs) > LINK_GRANT_FRESHNESS_WINDOW_MS
  ) {
    return unauthorized("fresh signed link-grant request required");
  }

  const user = await getUserForVerify(env.DB, userId);
  // Identical refusal for "no such identity" and "bad signature": this
  // route must not become an oracle for which user ids are registered.
  if (!user) return unauthorized("link-grant authorization failed");

  const message = canonicalLinkGrantBytes({
    user_id: userId,
    timestamp_ms: timestampMs,
    request_id: requestId,
  });
  let publicKey: Uint8Array;
  let signature: Uint8Array;
  try {
    publicKey = decodeBase64(user.ik_ed25519_pub);
    signature = decodeBase64(signatureB64);
  } catch {
    return unauthorized("link-grant authorization failed");
  }
  if (!(await verifyEd25519(publicKey, message, signature))) {
    return unauthorized("link-grant authorization failed");
  }

  // Only now that the caller has proved who they are is it worth
  // spending their identity's budget.
  const identityLimit = await checkRateLimit(
    env,
    userId,
    GRANT_IDENTITY_PER_MINUTE,
    "link-grant-identity",
  );
  if (!identityLimit.ok) return tooMany(identityLimit.retryAfter);

  const nowSeconds = Math.floor(Date.now() / 1000);
  const requestDigest = new Uint8Array(await crypto.subtle.digest("SHA-256", message));
  const day = Math.floor(nowSeconds / DAY_SECONDS);

  // One transaction, two invariants:
  //   1. the receipt makes this signed request single-use, so a captured
  //      request cannot be replayed into a second grant;
  //   2. the quota upsert trips a BEFORE INSERT trigger once the daily
  //      ceiling is reached. The trigger fires even though the statement
  //      resolves to an UPDATE, which is what makes the cap race-safe
  //      rather than a check-then-write.
  // Either both land or neither does: a refused quota must not burn the
  // request id, and a replayed request must not spend quota.
  try {
    await env.DB.batch([
      env.DB.prepare(
        "INSERT INTO link_grant_receipts (user_id, request_digest, expires_at) VALUES (?, ?, ?)",
      ).bind(userId, requestDigest, nowSeconds + RECEIPT_TTL_SECONDS),
      env.DB.prepare(
        `INSERT INTO link_grant_quota (user_id, day, issued) VALUES (?, ?, 1)
           ON CONFLICT(user_id, day) DO UPDATE SET issued = issued + 1`,
      ).bind(userId, day),
    ]);
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    if (detail.includes("link grant daily quota exceeded")) {
      // Not a rate limit in the "slow down" sense — waiting until the
      // next UTC day is the only thing that helps, so say how long.
      return tooMany((day + 1) * DAY_SECONDS - nowSeconds);
    }
    if (detail.includes("UNIQUE") || detail.includes("PRIMARY")) {
      return conflict("signed link-grant request already used");
    }
    console.error("[link-grant] issuance bookkeeping failed");
    return serviceUnavailable("link grant issuance is temporarily unavailable");
  }

  const grant = await mintGrant(issuer, nowSeconds);
  // The response body is the entire product: an opaque header value and
  // its expiry. No identity, no link id, no echo of the request.
  return json({ authorization: grant.authorization, expires_at: grant.expiresAt });
}
