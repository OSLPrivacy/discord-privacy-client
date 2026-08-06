import type { Env } from "../env.js";
import { getUserForVerify } from "../lib/db.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import { badRequest, conflict, json, tooMany, unauthorized } from "../lib/http.js";
import { isHighEntropyRequestId, isNonEmptyBase64, isProtocolId } from "../lib/validation.js";
import { verifySignedRequest } from "../lib/signed-request.js";
import {
  USERNAME_FRESHNESS_MS,
  UsernameNotAnalyzable,
  usernameClaimMessage,
  usernameSkeleton,
  validNormalizedUsername,
  validateFriendCode,
} from "../lib/username.js";

/// Every lookup answer is padded to exactly this many bytes of JSON.
///
/// The ceiling has to clear the largest answer this route can produce: a
/// `friend_code` is capped at 8199 characters by `handleUsernameClaim`, a
/// username at 30, and the surrounding JSON scaffolding is under 100. Both
/// fields are drawn from alphabets (`base64url` + `OSLFR1.`, and
/// `[a-z0-9_]`) with no JSON escapes, so the encoded length is exactly the
/// character count and this bound is not an estimate.
export const USERNAME_LOOKUP_RESPONSE_BYTES = 9216;

/// D81. Build the ONE response shape this route is allowed to emit.
///
/// A hit and a miss must be indistinguishable to anyone who can see the
/// response but not decrypt it, which means three things have to match, not
/// one: the status (always 200 -- a 404 is a plaintext answer at the TLS
/// record layer), the header set (identical, via the shared `json` helper),
/// and the body length. The third is the one that is easy to get wrong here:
/// `friend_code` carries an entire signed key bundle and ranges over roughly
/// 8 KiB, so an unpadded hit leaks *which* username was resolved by size
/// alone, not merely that one was.
///
/// `found` is what a caller branches on. It is inside the encrypted body, so
/// it tells the client everything and an observer nothing.
function paddedLookupResponse(
  row: { username: string; friend_code: string } | null,
): Response {
  const body: Record<string, unknown> = {
    found: row !== null,
    username: row?.username ?? null,
    friend_code: row?.friend_code ?? null,
    pad: "",
  };
  const encoder = new TextEncoder();
  const baseline = encoder.encode(JSON.stringify(body)).length;
  const needed = USERNAME_LOOKUP_RESPONSE_BYTES - baseline;
  if (needed < 0) {
    // Unreachable given the claim-time bounds above. If it ever happens, a
    // short answer is a length leak, so refuse rather than emit one.
    return json({ error: "username record exceeds the padded response bound" }, { status: 500 });
  }
  // "A" needs no JSON escaping, so each character contributes exactly one
  // byte and the total lands on the target rather than near it.
  body.pad = "A".repeat(needed);
  return json(body, { status: 200 });
}

/// `POST /v1/usernames/lookup` — resolve a handle to its friend code.
///
/// # Why the username is in the BODY and not the path
///
/// This route used to be `GET /v1/usernames/:username`. Cloudflare records
/// `ClientRequestURI` for a proxied zone by default and does not record
/// request headers or bodies, so that form wrote the plaintext handle a
/// caller was interested in -- next to that caller's `ClientIP` -- into
/// retained platform state that no OSL setting turns off. `[observability]
/// enabled = false` in `wrangler.toml` suppresses this Worker's own logs; it
/// does not touch the zone's HTTP request data.
///
/// The username is not a secret -- the directory is public and enumerable by
/// design. What is sensitive is *the pairing*: "this address looked up this
/// person, at this second". Moving the value into the body removes the
/// pairing from the retained surface entirely, and costs nothing, because the
/// route is not cacheable anyway (`cache-control: no-store`).
export async function handleUsernameLookup(request: Request, env: Env): Promise<Response> {
  const rlIp = await checkRateLimit(env, callerIp(request), 120, "username-lookup-ip");
  if (!rlIp.ok) return tooMany(rlIp.retryAfter);
  let body: Record<string, unknown>;
  try { body = await request.json() as Record<string, unknown>; }
  catch { return badRequest("malformed JSON body"); }
  // Exact lookup only. Rejecting non-canonical input prevents a supposedly
  // convenient lowercase transform from resolving a different identifier.
  if (!validNormalizedUsername(body.username)) {
    return badRequest("username must already be normalized");
  }
  const row = await env.DB.prepare(
    "SELECT username, friend_code FROM username_directory WHERE username = ?",
  ).bind(body.username).first<{ username: string; friend_code: string }>();
  return paddedLookupResponse(row);
}
export async function handleUsernameClaim(request: Request, env: Env): Promise<Response> {
  const rlIp = await checkRateLimit(env, callerIp(request), 10, "username-claim-ip");
  if (!rlIp.ok) return tooMany(rlIp.retryAfter);
  let body: Record<string, unknown>;
  try { body = await request.json() as Record<string, unknown>; }
  catch { return badRequest("malformed JSON body"); }
  if (!validNormalizedUsername(body.username)) return badRequest("username must already be normalized");
  if (!isProtocolId(body.user_id)) return badRequest("user_id invalid");
  if (typeof body.friend_code !== "string" || body.friend_code.length < 24 || body.friend_code.length > 8199) return badRequest("friend_code invalid");
  if (!isHighEntropyRequestId(body.request_id)) return badRequest("request_id invalid");
  if (!isNonEmptyBase64(body.signature_b64)) return badRequest("signature_b64 invalid");
  if (typeof body.timestamp_ms !== "number" || !Number.isSafeInteger(body.timestamp_ms) || body.timestamp_ms <= 0) return badRequest("timestamp_ms invalid");
  if (Math.abs(Date.now() - body.timestamp_ms) > USERNAME_FRESHNESS_MS) return badRequest("timestamp_ms stale");

  const username = body.username;
  const userId = body.user_id;
  const current = await getUserForVerify(env.DB, userId);
  if (!current) return unauthorized("registered identity required");
  const validInvite = await validateFriendCode(body.friend_code, userId, current.ik_ed25519_pub);
  if (!validInvite) return badRequest("friend_code is not a valid invite for this identity");
  const message = usernameClaimMessage({
    username,
    user_id: userId,
    friend_code: body.friend_code,
    request_id: body.request_id,
    timestamp_ms: body.timestamp_ms,
  });
  if (!await verifySignedRequest(current.ik_ed25519_pub, message, body.signature_b64)) {
    return unauthorized("username claim signature invalid");
  }
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", message));
  const now = new Date().toISOString();
  // D-248. Compute the skeleton BEFORE the batch and refuse the claim if it
  // cannot be computed. A claim that reached the directory without one would
  // be a row whose confusables nothing constrains.
  let skeleton: string;
  try {
    skeleton = usernameSkeleton(username);
  } catch (error) {
    if (error instanceof UsernameNotAnalyzable) return badRequest(error.message);
    throw error;
  }
  let result: D1Result[];
  try {
    const proofRecord = JSON.stringify({
      username,
      user_id: userId,
      request_id: body.request_id,
      signature_b64: body.signature_b64,
      timestamp_ms: body.timestamp_ms,
    });
    result = await env.DB.batch([
      env.DB.prepare("DELETE FROM username_claim_receipts WHERE expires_at < ?").bind(Math.floor(Date.now() / 1000)),
      env.DB.prepare(
        `INSERT INTO username_claim_receipts (user_id, request_digest, expires_at)
         SELECT ?1, ?2, ?3 WHERE EXISTS (
           SELECT 1 FROM users WHERE user_id = ?1 AND ik_ed25519_pub = ?4
         )`,
      ).bind(userId, digest, Math.floor(Date.now() / 1000) + 10 * 60, current.ik_ed25519_pub),
      env.DB.prepare(
        `DELETE FROM username_directory
          WHERE user_id = ?1 AND username <> ?2
            AND EXISTS (SELECT 1 FROM username_claim_receipts WHERE user_id = ?1 AND request_digest = ?3)
            AND NOT EXISTS (
              SELECT 1 FROM username_directory
               WHERE username = ?2 AND user_id <> ?1
            )`,
      ).bind(userId, username, digest),
      // D-248. `username_skeleton` is `?6`, the UTS #39 skeleton, NOT `?1`.
      // Writing `?1` there made the skeleton unique index decorative: a
      // skeleton equal to the raw name cannot collide with anything the raw
      // name did not already collide with on the primary key.
      //
      // `display_username` stays `?1` deliberately. The claim path is
      // validate-don't-transform (D-162), so the accepted spelling IS what the
      // user typed; there is no second form to display and inventing one here
      // would be the transform D-162 removed.
      //
      // The upsert branch now rewrites both derived columns. Without that, a
      // row written by an older generation keeps its raw skeleton forever
      // through every refresh, which is exactly how D-248 would have survived
      // its own fix. If the corrected skeleton collides with another identity's
      // the batch aborts and the caller gets 409 — fail closed, on purpose.
      env.DB.prepare(
        `INSERT INTO username_directory
           (username, username_skeleton, display_username, user_id, friend_code, claimed_at, updated_at)
         SELECT ?1, ?6, ?1, ?2, ?3, ?4, ?4
          WHERE EXISTS (SELECT 1 FROM username_claim_receipts WHERE user_id = ?2 AND request_digest = ?5)
         ON CONFLICT(username) DO UPDATE SET
           username = excluded.username, username_skeleton = excluded.username_skeleton,
           display_username = excluded.display_username,
           friend_code = excluded.friend_code, updated_at = excluded.updated_at
         WHERE username_directory.user_id = excluded.user_id`,
      ).bind(username, userId, body.friend_code, now, digest, skeleton),
      env.DB.prepare(
        `INSERT INTO saved_names
           (public_name, public_identity_key, proof_record, claimed_at)
         SELECT ?1, ?2, ?3, ?4
          WHERE EXISTS (SELECT 1 FROM username_claim_receipts WHERE user_id = ?5 AND request_digest = ?6)
         ON CONFLICT(public_identity_key) DO UPDATE SET
           public_name = excluded.public_name,
           public_identity_key = excluded.public_identity_key,
           proof_record = excluded.proof_record,
           claimed_at = excluded.claimed_at`,
      ).bind(username, current.ik_ed25519_pub, proofRecord, now, userId, digest),
    ]);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (/username_directory\.username|username is retired|UNIQUE|PRIMARY/i.test(message)) {
      return conflict("username is unavailable");
    }
    throw error;
  }
  if ((result[1]?.meta?.changes ?? 0) !== 1) return conflict("username claim replayed or identity changed");
  if ((result[3]?.meta?.changes ?? 0) !== 1) return conflict("username is unavailable");
  if ((result[4]?.meta?.changes ?? 0) !== 1) return conflict("saved name unavailable");
  return json({ username, user_id: userId }, { status: 200 });
}
