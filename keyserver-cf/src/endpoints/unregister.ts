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
import { getUserForVerify } from "../lib/db.js";
import {
  badRequest,
  json,
  notFound,
  tooMany,
  unauthorized,
} from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import {
  decodeBase64,
  isNonEmptyBase64,
  isPlainString,
} from "../lib/validation.js";

export async function handleUnregister(
  request: Request,
  env: Env,
  userId: string,
): Promise<Response> {
  // Rate-limit at the same threshold as the existing wrapped-keys
  // DELETE — small budget, identity ops are rare.
  const rl = await checkRateLimit(env, callerIp(request), 10);
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
  const ts = body.timestamp_ms;
  if (typeof ts !== "number" || !Number.isFinite(ts) || ts <= 0) {
    return badRequest("timestamp_ms required (positive number)");
  }
  const now = Date.now();
  if (Math.abs(now - ts) > UNREGISTER_FRESHNESS_WINDOW_MS) {
    return badRequest(
      `timestamp_ms not within ${UNREGISTER_FRESHNESS_WINDOW_MS}ms of server clock`,
    );
  }

  if (!isPlainString(userId)) return badRequest("user_id required");
  const user = await getUserForVerify(env.DB, userId);
  if (!user) {
    // Already absent — idempotent success.
    return json({ status: "noop", reason: "user not registered" });
  }

  const message = canonicalUnregisterBytes({
    user_id: userId,
    timestamp_ms: ts,
  });
  const pubBytes = decodeBase64(user.ik_ed25519_pub);
  const sigBytes = decodeBase64(body.signature_b64);
  const ok = await verifyEd25519(pubBytes, message, sigBytes);
  if (!ok) return unauthorized("signature verification failed");

  // Cascade the wipe to every table that holds rows owned by this
  // user_id. Each statement uses parameter binding so user_id can
  // never inject SQL.
  await env.DB.batch([
    env.DB.prepare("DELETE FROM users WHERE user_id = ?").bind(userId),
    env.DB
      .prepare("DELETE FROM wrapped_keys WHERE recipient_id = ? OR sender_id = ?")
      .bind(userId, userId),
    env.DB.prepare("DELETE FROM prekey_bundles WHERE user_id = ?").bind(userId),
    env.DB.prepare("DELETE FROM opk_pool WHERE user_id = ?").bind(userId),
  ]);

  return json({ status: "deleted", user_id: userId });
}

// Tag this as a no-op reference so the file is treated as a module.
void notFound;
