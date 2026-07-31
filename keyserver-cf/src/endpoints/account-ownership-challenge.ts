/// POST /v1/account-ownership/challenge - issue a short-lived
/// ProofChallenge-shaped nonce for a claimed Discord snowflake.
///
/// This endpoint grants no registration, lookup, binding, or account
/// authority. It only mints a nonce when the caller supplies the full binding
/// tuple and explicit consent. Missing consent or missing binding fields is a
/// refusal. The durable row stores hashes only; clear account identifiers stay
/// out of D1 and logs.

import type { Env } from "../env.js";
import {
  issueDiscordAccountOwnershipChallenge,
} from "../lib/account-ownership-challenge.js";
import {
  badRequest,
  forbidden,
  json,
  serviceUnavailable,
  tooMany,
} from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import { isDiscordSnowflake, isProtocolId } from "../lib/validation.js";

const CHALLENGE_IP_PER_MINUTE = 10;
const INSERT_ATTEMPTS = 3;

const ALLOWED_KEYS = new Set([
  "service",
  "service_account_id",
  "owner_user_id",
  "consent",
]);

export async function handleAccountOwnershipChallenge(
  request: Request,
  env: Env,
): Promise<Response> {
  const ipLimit = await checkRateLimit(
    env,
    callerIp(request),
    CHALLENGE_IP_PER_MINUTE,
    "account-ownership-challenge-ip",
  );
  if (!ipLimit.ok) return tooMany(ipLimit.retryAfter);

  let parsed: unknown;
  try {
    parsed = await request.json();
  } catch {
    return badRequest("malformed JSON body");
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    return badRequest("challenge body must be an object");
  }
  const body = parsed as Record<string, unknown>;
  for (const key of Object.keys(body)) {
    if (!ALLOWED_KEYS.has(key)) {
      return badRequest("challenge body contains unsupported fields");
    }
  }

  if (body.consent !== true) {
    return forbidden("account ownership challenge requires explicit consent");
  }
  if (body.service !== "discord") {
    return badRequest("unsupported account ownership proof service");
  }
  if (
    typeof body.service_account_id !== "string" ||
    !isDiscordSnowflake(body.service_account_id)
  ) {
    return badRequest("service_account_id must be a Discord snowflake");
  }
  if (!isProtocolId(body.owner_user_id)) {
    return badRequest("owner_user_id must be a bounded OSL identity");
  }
  if (isDiscordSnowflake(body.owner_user_id)) {
    return badRequest("owner_user_id must not be a Discord snowflake");
  }

  const now = Math.floor(Date.now() / 1000);
  try {
    await env.DB.prepare(
      "DELETE FROM account_ownership_challenges WHERE expires_at_unix_seconds <= ?",
    ).bind(now).run();
  } catch {
    return serviceUnavailable("account ownership challenge storage unavailable");
  }

  for (let attempt = 0; attempt < INSERT_ATTEMPTS; attempt++) {
    const challenge = await issueDiscordAccountOwnershipChallenge(
      body.service_account_id,
      body.owner_user_id,
      now,
    );
    try {
      await env.DB.prepare(
        `INSERT INTO account_ownership_challenges (
           nonce_sha256, binding_sha256, service,
           issued_at_unix_seconds, expires_at_unix_seconds,
           spent_at_unix_seconds
         ) VALUES (?, ?, 'discord', ?, ?, NULL)`,
      ).bind(
        challenge.nonceSha256,
        challenge.bindingSha256,
        challenge.response.issued_at_unix_seconds,
        challenge.response.expires_at_unix_seconds,
      ).run();
      return json(challenge.response, { status: 201 });
    } catch (err) {
      if (String(err).includes("UNIQUE constraint failed")) continue;
      return serviceUnavailable("account ownership challenge storage unavailable");
    }
  }

  return serviceUnavailable("account ownership challenge storage unavailable");
}
