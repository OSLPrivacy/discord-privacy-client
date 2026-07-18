/// `DELETE /v1/pubkeys/:user_id` — account-burn unregister.
///
/// Authorisation: signature by the CURRENTLY-STORED `ik_ed25519_pub`
/// over a canonical (domain || user_id || timestamp_ms) tuple. Only
/// the legitimate identity holder (who has the secret matching the
/// stored ed25519 pubkey) can issue the delete.
///
/// Cascades the delete to all rows owned by this user across the
/// keyserver tables (wrapped_keys, prekey_bundles, etc.) so a
/// stale row in any auxiliary table can't strand the user_id.
///
/// Body:
///   {
///     "signature_b64":  "<base64 ed25519 signature>",
///     "timestamp_ms":   <int>,
///   }
///
/// Freshness window: timestamp_ms must be within
/// UNREGISTER_FRESHNESS_WINDOW_MS of server clock. Prevents
/// long-lived replay of an old signed delete.

import type { Env } from "../env.js";
import {
  canonicalUnregisterBytes,
  UNREGISTER_FRESHNESS_WINDOW_MS,
} from "../lib/canonical.js";
import { verifyEd25519 } from "../lib/crypto.js";
import { getUserForVerify, unregisterUserIfCurrent } from "../lib/db.js";
import {
  badRequest,
  conflict,
  json,
  tooMany,
  unauthorized,
} from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import {
  decodeBase64,
  isNonEmptyBase64,
  isProtocolId,
} from "../lib/validation.js";

export async function handleUnregister(
  request: Request,
  env: Env,
  userId: string,
): Promise<Response> {
  // Rate-limit at the same threshold as the existing wrapped-keys
  // DELETE — small budget, identity ops are rare.
  const rl = await checkRateLimit(env, callerIp(request), 10, "unregister");
  if (!rl.ok) return tooMany(rl.retryAfter);

  let body: Record<string, unknown>;
  try {
    body = (await request.json()) as Record<string, unknown>;
  } catch {
    return badRequest("malformed JSON body");
  }

  if (!isNonEmptyBase64(body.signature_b64)) {
    return badRequest("signature_b64 required");
  }
  const timestamp = body.timestamp_ms;
  if (
    typeof timestamp !== "number" ||
    !Number.isSafeInteger(timestamp) ||
    timestamp <= 0
  ) {
    return badRequest("timestamp_ms required (positive number)");
  }
  const ts = timestamp;
  const now = Date.now();
  if (Math.abs(now - ts) > UNREGISTER_FRESHNESS_WINDOW_MS) {
    return badRequest(
      `timestamp_ms not within ${UNREGISTER_FRESHNESS_WINDOW_MS}ms of server clock`,
    );
  }

  if (!isProtocolId(userId)) return badRequest("user_id must be a bounded identifier");
  const user = await getUserForVerify(env.DB, userId);
  if (!user) {
    // Already absent — idempotent success.
    return json({ status: "noop", reason: "user not registered" });
  }

  const message = canonicalUnregisterBytes({
    user_id: userId,
    timestamp_ms: ts,
  });
  let pubBytes: Uint8Array;
  let sigBytes: Uint8Array;
  try {
    pubBytes = decodeBase64(user.ik_ed25519_pub);
    sigBytes = decodeBase64(body.signature_b64);
  } catch {
    return badRequest("signature_b64 must be valid base64");
  }
  if (pubBytes.length !== 32 || sigBytes.length !== 64) {
    return badRequest("signature_b64 must decode to 64 bytes");
  }
  const ok = await verifyEd25519(pubBytes, message, sigBytes);
  if (!ok) return unauthorized("signature verification failed");

  const requestDigest = new Uint8Array(
    await crypto.subtle.digest("SHA-256", message),
  );

  const result = await unregisterUserIfCurrent(
    env.DB,
    userId,
    user.ik_ed25519_pub,
    requestDigest,
    Math.floor(Date.now() / 1000) + 10 * 60,
  );
  if (result === "replay") {
    return conflict("signed unregister request already used");
  }
  if (result === "stale_identity") {
    return conflict("identity key changed during unregister; retry");
  }

  return json({ status: "deleted", user_id: userId });
}
