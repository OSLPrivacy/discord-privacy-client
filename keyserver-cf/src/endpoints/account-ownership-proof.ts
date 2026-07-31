/// POST /v1/account-ownership/proof — redeem an issued account-ownership
/// challenge and record the durable platform-account binding.
///
/// This is the consumer half of `/v1/account-ownership/challenge`. Without it
/// the challenge nonce is decorative: it is minted, handed out, and never
/// checked, so an account-ownership claim proves nothing.
///
/// The route is the ONLY writer of `account_ownership_challenges
/// .spent_at_unix_seconds` and the ONLY writer of
/// `account_ownership_proof_bindings`. Both properties matter:
///
///   S1 single use — the challenge row is spent with a conditional UPDATE
///      (`WHERE spent_at_unix_seconds IS NULL`). The first redemption wins;
///      every replay of the same proof matches zero rows and is refused. The
///      in-memory `spent` flag checked by `verify_ownership_proof` is a fast
///      path, not the gate; the gate is the row.
///   S2 admission — the binding row can only be written while its exact
///      challenge commitment is spent AND still fresh, which migration 0037
///      enforces with a BEFORE INSERT trigger. The trigger also requires the
///      owner identity to already exist in `users`, so an ownership binding
///      can never precede the identity it binds to.
///
/// D1 retains commitments only. The clear Discord snowflake and the clear OSL
/// owner id travel in the request so the server can recompute
/// `sha256(canonical binding)`; neither the snowflake nor the raw nonce is
/// stored. A single hex digit of difference anywhere in the binding tuple
/// (account, owner, nonce, issued_at, expires_at) fails the lookup, which is
/// what makes "proof bound to a different account" unrepresentable rather
/// than merely refused.

import type { Env } from "../env.js";
import {
  canonicalChallengeBindingBytes,
  sha256Hex,
  type IssuedAccountOwnershipChallenge,
} from "../lib/account-ownership-challenge.js";
import {
  verify_ownership_proof,
  type Account,
  type AccountOwnershipError,
} from "../lib/account-ownership-proof.js";
import { decodeCanonicalBase64 } from "../lib/identity-authority.js";
import { getUserForRegistration } from "../lib/db.js";
import {
  badRequest,
  conflict,
  error,
  forbidden,
  json,
  serviceUnavailable,
  tooMany,
} from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import { isDiscordSnowflake, isProtocolId } from "../lib/validation.js";

const PROOF_IP_PER_MINUTE = 10;

const ALLOWED_KEYS = new Set([
  "service",
  "service_account_id",
  "owner_user_id",
  "proof",
]);

/// Refusal statuses. 400 = the caller's request cannot be parsed as a proof at
/// all; 403 = a well-formed proof that does not authorise this binding; 409 =
/// a proof that was already spent.
const REFUSAL_STATUS: Record<AccountOwnershipError, number> = {
  no_proof_presented: 400,
  proof_malformed: 400,
  unsupported_service: 400,
  proof_for_different_account: 403,
  proof_for_different_owner: 403,
  proof_stale: 403,
  proof_replayed: 409,
};

interface ChallengeRow {
  nonce_sha256: string;
  binding_sha256: string;
  issued_at_unix_seconds: number;
  expires_at_unix_seconds: number;
  spent_at_unix_seconds: number | null;
}

export async function handleAccountOwnershipProof(
  request: Request,
  env: Env,
): Promise<Response> {
  const ipLimit = await checkRateLimit(
    env,
    callerIp(request),
    PROOF_IP_PER_MINUTE,
    "account-ownership-proof-ip",
  );
  if (!ipLimit.ok) return tooMany(ipLimit.retryAfter);

  let parsed: unknown;
  try {
    parsed = await request.json();
  } catch {
    return badRequest("malformed JSON body");
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    return badRequest("account ownership proof body must be an object");
  }
  const body = parsed as Record<string, unknown>;
  for (const key of Object.keys(body)) {
    if (!ALLOWED_KEYS.has(key)) {
      return badRequest("account ownership proof body contains unsupported fields");
    }
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
  const serviceAccountId = body.service_account_id;
  const ownerUserId = body.owner_user_id;

  // A missing proof is refused here rather than being routed into the
  // verifier, so "no proof presented" can never be mistaken for "verified".
  const submitted = body.proof;
  if (!submitted || typeof submitted !== "object" || Array.isArray(submitted)) {
    return refuse("no_proof_presented");
  }
  const proof = submitted as Record<string, unknown>;
  const evidence = proof.e;
  if (!evidence || typeof evidence !== "object" || Array.isArray(evidence)) {
    return refuse("proof_malformed");
  }
  const e = evidence as Record<string, unknown>;
  const issuedAt = e.issued_at_unix_seconds;
  const expiresAt = e.expires_at_unix_seconds;
  if (
    typeof e.nonce_b64 !== "string" ||
    !Number.isSafeInteger(issuedAt) ||
    !Number.isSafeInteger(expiresAt) ||
    (issuedAt as number) <= 0 ||
    (expiresAt as number) <= (issuedAt as number)
  ) {
    return refuse("proof_malformed");
  }

  // Commitments are recomputed from the caller's clear tuple. This both finds
  // the issued row and proves the caller is answering THAT challenge.
  let nonceSha256: string;
  let bindingSha256: string;
  try {
    const nonceBytes = decodeCanonicalBase64(
      e.nonce_b64,
      32,
      "account ownership proof nonce",
    );
    const claimed: IssuedAccountOwnershipChallenge = {
      challenge_version: 1,
      service: "discord",
      service_account_id: serviceAccountId,
      owner_user_id: ownerUserId,
      nonce: e.nonce_b64,
      issued_at_unix_seconds: issuedAt as number,
      expires_at_unix_seconds: expiresAt as number,
      spent: false,
    };
    [nonceSha256, bindingSha256] = await Promise.all([
      sha256Hex(nonceBytes),
      sha256Hex(canonicalChallengeBindingBytes(claimed)),
    ]);
  } catch {
    return refuse("proof_malformed");
  }

  let row: ChallengeRow | null;
  try {
    row = await env.DB.prepare(
      `SELECT nonce_sha256, binding_sha256, issued_at_unix_seconds,
              expires_at_unix_seconds, spent_at_unix_seconds
         FROM account_ownership_challenges
        WHERE nonce_sha256 = ?`,
    ).bind(nonceSha256).first<ChallengeRow>();
  } catch {
    return serviceUnavailable("account ownership challenge storage unavailable");
  }
  if (!row) {
    return forbidden(
      "account ownership proof answers no issued challenge",
    );
  }
  // The stored commitment covers account, owner, nonce and lifetime together.
  // Answering a real nonce while claiming a different account or owner lands
  // here, not on a signature check.
  if (
    row.binding_sha256 !== bindingSha256 ||
    row.issued_at_unix_seconds !== issuedAt ||
    row.expires_at_unix_seconds !== expiresAt
  ) {
    return refuse("proof_for_different_account");
  }

  // The signing key is the one the owner identity is registered under, never
  // one supplied with the proof. Without this the proof would only show that
  // SOMEBODY signed the nonce.
  let owner: { user_id: string; ik_ed25519_pub: string } | null;
  try {
    owner = await getUserForRegistration(env.DB, ownerUserId);
  } catch {
    return serviceUnavailable("account ownership challenge storage unavailable");
  }
  if (!owner) {
    return forbidden(
      "account ownership proof owner is not a registered identity",
    );
  }

  const now = Math.floor(Date.now() / 1000);
  const account: Account = {
    platform_id: serviceAccountId,
    owner_user_id: ownerUserId,
    owner_ed25519_pub_b64: owner.ik_ed25519_pub,
    proof_challenge: {
      platform_id: serviceAccountId,
      owner_user_id: ownerUserId,
      nonce_b64: e.nonce_b64,
      issued_at_unix_seconds: row.issued_at_unix_seconds,
      expires_at_unix_seconds: row.expires_at_unix_seconds,
      spent: row.spent_at_unix_seconds !== null,
    },
    ownership_proof: submitted as Account["ownership_proof"],
  };

  const verified = await verify_ownership_proof(account, now);
  if (!verified.ok) return refuse(verified.error);

  // The trigger in migration 0037 requires spent_at >= issued_at and
  // verified_at within [spent_at, expires_at). A clock behind the issuer would
  // otherwise turn a valid redemption into a 503; clamp instead of failing a
  // proof the verifier just accepted.
  const spentAt = Math.max(now, row.issued_at_unix_seconds);
  if (spentAt >= row.expires_at_unix_seconds) {
    return refuse("proof_stale");
  }

  // Spend-then-bind. The conditional UPDATE is the single-use gate: a replay
  // matches zero rows, so it can never reach the INSERT with a fresh binding.
  // The batch is transactional, so a refused INSERT does not burn the
  // challenge.
  let spendChanges = 0;
  try {
    const results = await env.DB.batch([
      env.DB.prepare(
        `UPDATE account_ownership_challenges
            SET spent_at_unix_seconds = ?
          WHERE nonce_sha256 = ?
            AND spent_at_unix_seconds IS NULL`,
      ).bind(spentAt, nonceSha256),
      env.DB.prepare(
        `INSERT INTO account_ownership_proof_bindings (
           binding_sha256, nonce_sha256, owner_user_id, service,
           proof_type, verified_at_unix_seconds
         ) VALUES (?, ?, ?, 'discord', ?, ?)`,
      ).bind(
        bindingSha256,
        nonceSha256,
        ownerUserId,
        account.ownership_proof?.proof_type ?? "",
        spentAt,
      ),
    ]);
    spendChanges = results[0]?.meta?.changes ?? 0;
  } catch (err) {
    const text = String(err);
    if (
      text.includes("UNIQUE constraint failed") ||
      text.includes("already spent")
    ) {
      return refuse("proof_replayed");
    }
    if (text.includes("challenge is required")) {
      return forbidden("account ownership proof admission was refused");
    }
    return serviceUnavailable("account ownership proof storage unavailable");
  }
  if (spendChanges !== 1) {
    return refuse("proof_replayed");
  }

  return json(
    {
      result: "account_ownership_proof_recorded",
      service: "discord",
      owner_user_id: ownerUserId,
      verified_at_unix_seconds: spentAt,
    },
    { status: 201 },
  );
}

function refuse(reason: AccountOwnershipError): Response {
  const status = REFUSAL_STATUS[reason];
  if (status === 409) return conflict(`account ownership proof rejected: ${reason}`);
  if (status === 403) return forbidden(`account ownership proof rejected: ${reason}`);
  return error(status, `account ownership proof rejected: ${reason}`);
}
