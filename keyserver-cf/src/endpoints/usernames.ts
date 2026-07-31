import type { Env } from "../env.js";
import { getUserForVerify } from "../lib/db.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import { badRequest, conflict, json, notFound, tooMany, unauthorized } from "../lib/http.js";
import { isHighEntropyRequestId, isNonEmptyBase64, isProtocolId } from "../lib/validation.js";
import { verifySignedRequest } from "../lib/signed-request.js";
import {
  USERNAME_FRESHNESS_MS,
  usernameClaimMessage,
  validNormalizedUsername,
  validateFriendCode,
} from "../lib/username.js";

export async function handleUsernameLookup(request: Request, env: Env, username: string): Promise<Response> {
  // Exact lookup only. Rejecting non-canonical input prevents a supposedly
  // convenient lowercase transform from resolving a different identifier.
  if (!validNormalizedUsername(username)) return badRequest("username must already be normalized");
  const rlIp = await checkRateLimit(env, callerIp(request), 120, "username-lookup-ip");
  if (!rlIp.ok) return tooMany(rlIp.retryAfter);
  const row = await env.DB.prepare(
    "SELECT username, friend_code FROM username_directory WHERE username = ?",
  ).bind(username).first<{ username: string; friend_code: string }>();
  return row ? json(row) : notFound("username not found");
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
  let result: D1Result[];
  try {
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
      env.DB.prepare(
        `INSERT INTO username_directory (username, user_id, friend_code, claimed_at, updated_at)
         SELECT ?1, ?2, ?3, ?4, ?4
          WHERE EXISTS (SELECT 1 FROM username_claim_receipts WHERE user_id = ?2 AND request_digest = ?5)
         ON CONFLICT(username) DO UPDATE SET
           username = excluded.username, friend_code = excluded.friend_code, updated_at = excluded.updated_at
         WHERE username_directory.user_id = excluded.user_id`,
      ).bind(username, userId, body.friend_code, now, digest),
    ]);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (/username_directory\.username|UNIQUE|PRIMARY/i.test(message)) return conflict("username is unavailable");
    throw error;
  }
  if ((result[1]?.meta?.changes ?? 0) !== 1) return conflict("username claim replayed or identity changed");
  if ((result[3]?.meta?.changes ?? 0) !== 1) return conflict("username is unavailable");
  return json({ username, user_id: userId }, { status: 200 });
}
