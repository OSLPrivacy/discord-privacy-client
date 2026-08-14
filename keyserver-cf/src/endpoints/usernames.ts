import type { Env } from "../env.js";
import {
  canonicalChallengeBindingBytes,
  sha256Hex,
  type IssuedAccountOwnershipChallenge,
} from "../lib/account-ownership-challenge.js";
import {
  canonicalAccountOwnershipProofBytes,
  verify_ownership_proof,
  type Account,
  type AccountOwnershipError,
  type AccountOwnershipProof,
} from "../lib/account-ownership-proof.js";
import { verifyEd25519 } from "../lib/crypto.js";
import { getUserForVerify } from "../lib/db.js";
import { decodeCanonicalBase64, decodeCanonicalEd25519SignatureBytes } from "../lib/identity-authority.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import { badRequest, conflict, error, forbidden, json, notFound, tooMany, unauthorized } from "../lib/http.js";
import { decodeBase64, isDiscordSnowflake, isHighEntropyRequestId, isNonEmptyBase64, isProtocolId } from "../lib/validation.js";
import { verifySignedRequest } from "../lib/signed-request.js";
import {
  USERNAME_FRESHNESS_MS,
  USERNAME_RULES_MESSAGE,
  UsernameNotAnalyzable,
  usernameClaimMessage,
  usernameMoveMessage,
  usernameReleaseMessage,
  usernameSkeleton,
  validNormalizedUsername,
  validateFriendCode,
} from "../lib/username.js";

interface PublicNameDirectoryRow {
  name: string;
  identity_fingerprint: string;
}

async function identityFingerprint(ed25519PublicKeyB64: string): Promise<string> {
  return sha256Hex(decodeBase64(ed25519PublicKeyB64));
}

export async function handlePublicNameExactSearch(request: Request, env: Env): Promise<Response> {
  const rlIp = await checkRateLimit(env, callerIp(request), 120, "public-name-exact-search-ip");
  if (!rlIp.ok) return tooMany(rlIp.retryAfter);
  let body: Record<string, unknown>;
  try { body = await request.json() as Record<string, unknown>; }
  catch { return badRequest("malformed JSON body"); }
  const keys = Object.keys(body).sort().join(",");
  if (keys !== "name") return badRequest("public name exact search must contain exactly name");
  if (!validNormalizedUsername(body.name)) return badRequest(USERNAME_RULES_MESSAGE);
  const row = await env.DB.prepare(
    `SELECT name, identity_fingerprint
       FROM public_name_directory
      WHERE name = ?`,
  ).bind(body.name).first<PublicNameDirectoryRow>();
  if (!row) return notFound("public name not found");
  return json({
    name: row.name,
    identity_fingerprint: row.identity_fingerprint,
  }, { status: 200 });
}

/// Every lookup answer is padded to exactly this many bytes of JSON.
///
/// The ceiling has to clear the largest answer this route can produce: a
/// `friend_code` is capped at 8199 characters by `handleUsernameClaim`, a
/// username at 30, and the surrounding JSON scaffolding is under 100. Both
/// fields are drawn from alphabets (`base64url` + `OSLFR1.`, and
/// `[a-z0-9_]`) with no JSON escapes, so the encoded length is exactly the
/// character count and this bound is not an estimate.
export const USERNAME_LOOKUP_RESPONSE_BYTES = 9216;

const MAX_PUBLIC_NAME_PROOF_LIFETIME_SECONDS = 5 * 60;
const PUBLIC_NAME_PROOF_DOMAIN = "OSL-PUBLIC-NAME-PROOF-v1\u0000";
const textEncoder = new TextEncoder();

interface ChallengeRow {
  binding_sha256: string;
  issued_at_unix_seconds: number;
  expires_at_unix_seconds: number;
  spent_at_unix_seconds: number | null;
}

interface SavedNameRow {
  public_identity_key: string;
}

interface UsernameDirectoryOwnerRow {
  user_id: string;
}

type PublicNameProofCheck =
  | {
      ok: true;
      nonceSha256: string;
      bindingSha256: string;
      expiresAtUnixSeconds: number;
    }
  | { ok: false; response: Response };

interface PublicNameProofEnvelope {
  public_name: string;
  account_proof: AccountOwnershipProof;
  signature_b64: string;
}

function concat(parts: readonly Uint8Array[]): Uint8Array {
  const output = new Uint8Array(parts.reduce((total, part) => total + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    output.set(part, offset);
    offset += part.length;
  }
  return output;
}

function u32be(value: number): Uint8Array {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffff_ffff) {
    throw new Error("canonical u32 is out of range");
  }
  const bytes = new Uint8Array(4);
  new DataView(bytes.buffer).setUint32(0, value, false);
  return bytes;
}

function lp(bytes: Uint8Array): Uint8Array {
  return concat([u32be(bytes.length), bytes]);
}

function lpText(value: string): Uint8Array {
  return lp(textEncoder.encode(value));
}

function canonicalPublicNameProofBytes(args: {
  publicName: string;
  accountProofBytes: Uint8Array;
}): Uint8Array {
  return concat([
    lpText(PUBLIC_NAME_PROOF_DOMAIN),
    lpText(args.publicName),
    lp(args.accountProofBytes),
  ]);
}

function publicNameProofEnvelope(value: unknown): PublicNameProofEnvelope | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const proof = value as Record<string, unknown>;
  const keys = Object.keys(proof).sort();
  if (
    keys.length !== 3 ||
    keys[0] !== "account_proof" ||
    keys[1] !== "public_name" ||
    keys[2] !== "signature_b64" ||
    typeof proof.public_name !== "string" ||
    typeof proof.signature_b64 !== "string" ||
    !proof.account_proof ||
    typeof proof.account_proof !== "object" ||
    Array.isArray(proof.account_proof)
  ) {
    return null;
  }
  return {
    public_name: proof.public_name,
    account_proof: proof.account_proof as AccountOwnershipProof,
    signature_b64: proof.signature_b64,
  };
}

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
    return badRequest(USERNAME_RULES_MESSAGE);
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
  if (!validNormalizedUsername(body.username)) return badRequest(USERNAME_RULES_MESSAGE);
  if (!isProtocolId(body.user_id)) return badRequest("user_id invalid");
  if (typeof body.service_account_id !== "string" || !isDiscordSnowflake(body.service_account_id)) return badRequest("service_account_id must be a Discord snowflake");
  if (typeof body.friend_code !== "string" || body.friend_code.length < 24 || body.friend_code.length > 8199) return badRequest("friend_code invalid");
  if (!isHighEntropyRequestId(body.request_id)) return badRequest("request_id invalid");
  if (!isNonEmptyBase64(body.signature_b64)) return badRequest("signature_b64 invalid");
  if (typeof body.timestamp_ms !== "number" || !Number.isSafeInteger(body.timestamp_ms) || body.timestamp_ms <= 0) return badRequest("timestamp_ms invalid");
  if (Math.abs(Date.now() - body.timestamp_ms) > USERNAME_FRESHNESS_MS) return badRequest("timestamp_ms stale");
  if (body.service !== "discord") return badRequest("unsupported public-name proof service");
  if (
    typeof body.service_account_id !== "string" ||
    !isDiscordSnowflake(body.service_account_id)
  ) {
    return badRequest("service_account_id must be a Discord snowflake");
  }

  const username = body.username;
  const userId = body.user_id;
  const current = await getUserForVerify(env.DB, userId);
  if (!current) return unauthorized("registered identity required");
  const publicIdentityFingerprint = await identityFingerprint(current.ik_ed25519_pub);
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
  const proofCheck = await verifyPublicNameProof({
    env,
    username,
    userId,
    serviceAccountId: body.service_account_id,
    ownerEd25519PubB64: current.ik_ed25519_pub,
    proof: body.public_name_proof,
  });
  if (!proofCheck.ok) return proofCheck.response;

  const savedName = await env.DB.prepare(
    "SELECT public_identity_key FROM saved_names WHERE public_name = ?",
  ).bind(username).first<SavedNameRow>();
  const directoryOwner = await env.DB.prepare(
    "SELECT user_id FROM username_directory WHERE username = ?",
  ).bind(username).first<UsernameDirectoryOwnerRow>();
  const moveApproval = await verifySavedNameMoveApproval({
    savedName,
    directoryOwner,
    body,
    username,
    userId,
    currentEd25519PubB64: current.ik_ed25519_pub,
  });
  if (!moveApproval.ok) return moveApproval.response;

  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", message));
  const now = new Date().toISOString();
  const nowUnixSeconds = Math.floor(Date.now() / 1000);
  const serviceAccountSha256 = await sha256Hex(new TextEncoder().encode(body.service_account_id));
  const proofRecord = JSON.stringify({
    service: "discord",
    service_account_sha256: serviceAccountSha256,
    nonce_sha256: proofCheck.nonceSha256,
    binding_sha256: proofCheck.bindingSha256,
    expires_at_unix_seconds: proofCheck.expiresAtUnixSeconds,
  });
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
    result = await env.DB.batch([
      env.DB.prepare("DELETE FROM username_claim_receipts WHERE expires_at < ?").bind(nowUnixSeconds),
      env.DB.prepare("DELETE FROM public_name_proofs WHERE expires_at_unix_seconds <= ?").bind(nowUnixSeconds),
      env.DB.prepare(
        `INSERT INTO username_claim_receipts (user_id, request_digest, expires_at)
         SELECT ?1, ?2, ?3 WHERE EXISTS (
           SELECT 1 FROM users WHERE user_id = ?1 AND ik_ed25519_pub = ?4
         )`,
      ).bind(userId, digest, nowUnixSeconds + 10 * 60, current.ik_ed25519_pub),
      env.DB.prepare(
        `INSERT INTO public_name_proofs (
           nonce_sha256, binding_sha256, owner_user_id, service, service_account_sha256,
           username, claimed_at_unix_seconds, expires_at_unix_seconds
         )
         SELECT ?1, ?2, ?3, 'discord', ?4, ?5, ?6, ?7
          WHERE EXISTS (SELECT 1 FROM username_claim_receipts WHERE user_id = ?3 AND request_digest = ?8)
            AND EXISTS (
              SELECT 1 FROM account_ownership_challenges
               WHERE nonce_sha256 = ?1
                 AND binding_sha256 = ?2
                 AND service = 'discord'
                 AND spent_at_unix_seconds IS NULL
                 AND expires_at_unix_seconds > ?6
            )
            AND NOT EXISTS (
              SELECT 1 FROM username_directory
               WHERE username = ?5 AND user_id <> ?3
            )`,
      ).bind(
        proofCheck.nonceSha256,
        proofCheck.bindingSha256,
        userId,
        serviceAccountSha256,
        username,
        nowUnixSeconds,
        proofCheck.expiresAtUnixSeconds,
        digest,
      ),
      env.DB.prepare(
        `UPDATE account_ownership_challenges
            SET spent_at_unix_seconds = ?1
          WHERE nonce_sha256 = ?2
            AND binding_sha256 = ?3
            AND spent_at_unix_seconds IS NULL
            AND EXISTS (
              SELECT 1 FROM public_name_proofs
               WHERE nonce_sha256 = ?2
                 AND binding_sha256 = ?3
                 AND owner_user_id = ?4
                 AND username = ?5
            )`,
      ).bind(nowUnixSeconds, proofCheck.nonceSha256, proofCheck.bindingSha256, userId, username),
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
        `DELETE FROM saved_names
          WHERE public_identity_key = ?1
            AND public_name <> ?2`,
      ).bind(current.ik_ed25519_pub, username),
      env.DB.prepare(
        `INSERT INTO saved_names
           (public_name, public_identity_key, proof_record, claimed_at)
         SELECT ?1, ?2, ?3, ?4
          WHERE EXISTS (
            SELECT 1 FROM public_name_proofs
             WHERE nonce_sha256 = ?7
               AND binding_sha256 = ?8
               AND owner_user_id = ?9
               AND username = ?1
          )
            AND (
              NOT EXISTS (SELECT 1 FROM saved_names WHERE public_name = ?1)
              OR EXISTS (
                SELECT 1 FROM saved_names
                 WHERE public_name = ?1 AND public_identity_key = ?2
              )
              OR (?5 = 1 AND EXISTS (
                SELECT 1 FROM saved_names
                 WHERE public_name = ?1 AND public_identity_key = ?6
              ))
            )
         ON CONFLICT(public_name) DO UPDATE SET
           public_identity_key = excluded.public_identity_key,
           proof_record = excluded.proof_record,
           claimed_at = excluded.claimed_at
         WHERE saved_names.public_identity_key = excluded.public_identity_key
            OR (?5 = 1 AND saved_names.public_identity_key = ?6)`,
      ).bind(
        username,
        current.ik_ed25519_pub,
        proofRecord,
        now,
        moveApproval.approved ? 1 : 0,
        moveApproval.previousKey ?? "",
        proofCheck.nonceSha256,
        proofCheck.bindingSha256,
        userId,
      ),
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
            AND EXISTS (
              SELECT 1 FROM public_name_proofs
               WHERE nonce_sha256 = ?7
                 AND binding_sha256 = ?8
                 AND owner_user_id = ?2
                 AND username = ?1
            )
            AND EXISTS (
              SELECT 1 FROM saved_names
               WHERE public_name = ?1
                 AND public_identity_key = ?9
            )
         ON CONFLICT(username) DO UPDATE SET
           username = excluded.username, username_skeleton = excluded.username_skeleton,
           display_username = excluded.display_username,
           friend_code = excluded.friend_code, updated_at = excluded.updated_at
         WHERE username_directory.user_id = excluded.user_id`,
      ).bind(username, userId, body.friend_code, now, digest, skeleton, proofCheck.nonceSha256, proofCheck.bindingSha256, current.ik_ed25519_pub),
      env.DB.prepare(
        `INSERT INTO public_name_directory
           (name, identity_fingerprint, claimed_at, updated_at)
         SELECT ?1, ?2, ?3, ?3
          WHERE EXISTS (
            SELECT 1 FROM username_directory
             WHERE username = ?1 AND user_id = ?4
          )
         ON CONFLICT(name) DO UPDATE SET
           identity_fingerprint = excluded.identity_fingerprint,
           updated_at = excluded.updated_at
         WHERE public_name_directory.identity_fingerprint = excluded.identity_fingerprint`,
      ).bind(username, publicIdentityFingerprint, now, userId),
    ]);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (/username_directory\.username|username is retired|UNIQUE|PRIMARY/i.test(message)) {
      return conflict("username is unavailable");
    }
    if (/public_name_proofs/i.test(message)) {
      return conflict("public-name proof already claimed a name");
    }
    throw error;
  }
  if ((result[2]?.meta?.changes ?? 0) !== 1) return conflict("username claim replayed or identity changed");
  if ((result[3]?.meta?.changes ?? 0) !== 1) return conflict("public-name proof replayed, stale, or not bound to this name");
  if ((result[4]?.meta?.changes ?? 0) !== 1) return conflict("public-name proof already consumed");
  if ((result[7]?.meta?.changes ?? 0) !== 1) return conflict("public name is bound to a different identity key");
  if ((result[8]?.meta?.changes ?? 0) !== 1) return conflict("username is unavailable");
  if ((result[9]?.meta?.changes ?? 0) !== 1) return conflict("public name is unavailable");
  return json({ username, user_id: userId }, { status: 200 });
}

export async function handleUsernameRelease(request: Request, env: Env): Promise<Response> {
  const rlIp = await checkRateLimit(env, callerIp(request), 10, "username-release-ip");
  if (!rlIp.ok) return tooMany(rlIp.retryAfter);
  let body: Record<string, unknown>;
  try { body = await request.json() as Record<string, unknown>; }
  catch { return badRequest("malformed JSON body"); }
  const keys = Object.keys(body).sort().join(",");
  if (keys !== "request_id,signature_b64,timestamp_ms,user_id,username") {
    return badRequest("username release must contain exactly username, user_id, request_id, timestamp_ms, signature_b64");
  }
  if (!validNormalizedUsername(body.username)) return badRequest("username must already be normalized");
  if (!isProtocolId(body.user_id)) return badRequest("user_id invalid");
  if (!isHighEntropyRequestId(body.request_id)) return badRequest("request_id invalid");
  if (!isNonEmptyBase64(body.signature_b64)) return badRequest("signature_b64 invalid");
  if (typeof body.timestamp_ms !== "number" || !Number.isSafeInteger(body.timestamp_ms) || body.timestamp_ms <= 0) return badRequest("timestamp_ms invalid");
  if (Math.abs(Date.now() - body.timestamp_ms) > USERNAME_FRESHNESS_MS) return badRequest("timestamp_ms stale");

  const username = body.username;
  const userId = body.user_id;
  const current = await getUserForVerify(env.DB, userId);
  if (!current) return unauthorized("registered identity required");
  const message = usernameReleaseMessage({
    username,
    user_id: userId,
    request_id: body.request_id,
    timestamp_ms: body.timestamp_ms,
  });
  if (!await verifySignedRequest(current.ik_ed25519_pub, message, body.signature_b64)) {
    return unauthorized("username release signature invalid");
  }

  const result = await env.DB.batch([
    env.DB.prepare(
      `DELETE FROM public_name_directory
        WHERE name = ?1
          AND EXISTS (
            SELECT 1 FROM username_directory
             WHERE username = ?1 AND user_id = ?2
          )`,
    ).bind(username, userId),
    env.DB.prepare(
      `DELETE FROM saved_names
        WHERE public_name = ?1
          AND public_identity_key = ?2`,
    ).bind(username, current.ik_ed25519_pub),
    env.DB.prepare(
      `DELETE FROM username_directory
        WHERE username = ?1 AND user_id = ?2`,
    ).bind(username, userId),
  ]);
  return json({
    username,
    user_id: userId,
    released: (result[0]?.meta?.changes ?? 0) + (result[1]?.meta?.changes ?? 0) + (result[2]?.meta?.changes ?? 0) > 0,
  }, { status: 200 });
}

type SavedNameMoveApproval =
  | { ok: true; approved: boolean; previousKey: string | null }
  | { ok: false; response: Response };

async function verifySavedNameMoveApproval(args: {
  savedName: SavedNameRow | null;
  directoryOwner: UsernameDirectoryOwnerRow | null;
  body: Record<string, unknown>;
  username: string;
  userId: string;
  currentEd25519PubB64: string;
}): Promise<SavedNameMoveApproval> {
  if (args.directoryOwner && args.directoryOwner.user_id !== args.userId) {
    return {
      ok: false,
      response: conflict("username is unavailable"),
    };
  }
  const previousKey = args.savedName?.public_identity_key ?? null;
  if (!previousKey || previousKey === args.currentEd25519PubB64) {
    return { ok: true, approved: false, previousKey };
  }
  const approval = args.body.move_approval;
  if (!approval || typeof approval !== "object" || Array.isArray(approval)) {
    return {
      ok: false,
      response: forbidden("public name move requires old-key approval"),
    };
  }
  const move = approval as Record<string, unknown>;
  if (
    typeof move.prev_ik_ed25519_pub !== "string" ||
    move.prev_ik_ed25519_pub !== previousKey ||
    !isNonEmptyBase64(move.prev_sig)
  ) {
    return {
      ok: false,
      response: forbidden("public name move requires old-key approval"),
    };
  }
  const message = usernameMoveMessage({
    username: args.username,
    user_id: args.userId,
    prev_ik_ed25519_pub: previousKey,
    new_ik_ed25519_pub: args.currentEd25519PubB64,
    request_id: args.body.request_id as string,
    timestamp_ms: args.body.timestamp_ms as number,
  });
  if (!await verifySignedRequest(previousKey, message, move.prev_sig)) {
    return {
      ok: false,
      response: forbidden("public name move requires old-key approval"),
    };
  }
  return { ok: true, approved: true, previousKey };
}

async function verifyPublicNameProof(args: {
  env: Env;
  username: string;
  userId: string;
  serviceAccountId: string;
  ownerEd25519PubB64: string;
  proof: unknown;
}): Promise<PublicNameProofCheck> {
  const envelope = publicNameProofEnvelope(args.proof);
  if (!envelope) {
    return refusePublicNameProof("no_proof_presented");
  }
  const submitted = envelope.account_proof as unknown as Record<string, unknown>;
  const evidence = submitted.e;
  if (!evidence || typeof evidence !== "object" || Array.isArray(evidence)) {
    return refusePublicNameProof("proof_malformed");
  }
  const e = evidence as Record<string, unknown>;
  if (
    typeof e.nonce_b64 !== "string" ||
    !Number.isSafeInteger(e.issued_at_unix_seconds) ||
    !Number.isSafeInteger(e.expires_at_unix_seconds) ||
    (e.issued_at_unix_seconds as number) <= 0 ||
    (e.expires_at_unix_seconds as number) <= (e.issued_at_unix_seconds as number) ||
    (e.expires_at_unix_seconds as number) - (e.issued_at_unix_seconds as number) >
      MAX_PUBLIC_NAME_PROOF_LIFETIME_SECONDS
  ) {
    return refusePublicNameProof("proof_malformed");
  }

  let nonceSha256: string;
  let bindingSha256: string;
  try {
    const nonceBytes = decodeCanonicalBase64(
      e.nonce_b64,
      32,
      "public-name proof nonce",
    );
    const challenge: IssuedAccountOwnershipChallenge = {
      challenge_version: 1,
      service: "discord",
      service_account_id: args.serviceAccountId,
      owner_user_id: args.userId,
      nonce: e.nonce_b64,
      issued_at_unix_seconds: e.issued_at_unix_seconds as number,
      expires_at_unix_seconds: e.expires_at_unix_seconds as number,
      spent: false,
    };
    [nonceSha256, bindingSha256] = await Promise.all([
      sha256Hex(nonceBytes),
      sha256Hex(canonicalChallengeBindingBytes(challenge)),
    ]);
  } catch {
    return refusePublicNameProof("proof_malformed");
  }

  const row = await args.env.DB.prepare(
    `SELECT binding_sha256, issued_at_unix_seconds, expires_at_unix_seconds,
            spent_at_unix_seconds
       FROM account_ownership_challenges
      WHERE nonce_sha256 = ?`,
  ).bind(nonceSha256).first<ChallengeRow>();
  if (!row) {
    return { ok: false, response: forbidden("public-name proof answers no issued challenge") };
  }
  if (
    row.binding_sha256 !== bindingSha256 ||
    row.issued_at_unix_seconds !== e.issued_at_unix_seconds ||
    row.expires_at_unix_seconds !== e.expires_at_unix_seconds
  ) {
    return refusePublicNameProof("proof_for_different_account");
  }

  const account: Account = {
    platform_id: args.serviceAccountId,
    owner_user_id: args.userId,
    owner_ed25519_pub_b64: args.ownerEd25519PubB64,
    proof_challenge: {
      platform_id: args.serviceAccountId,
      owner_user_id: args.userId,
      nonce_b64: e.nonce_b64,
      issued_at_unix_seconds: row.issued_at_unix_seconds,
      expires_at_unix_seconds: row.expires_at_unix_seconds,
      spent: row.spent_at_unix_seconds !== null,
    },
    ownership_proof: submitted as unknown as Account["ownership_proof"],
  };
  const verified = await verify_ownership_proof(account, Math.floor(Date.now() / 1000));
  if (!verified.ok) return refusePublicNameProof(verified.error);

  let publicNameSignature: Uint8Array;
  let publicKey: Uint8Array;
  let publicNameProofBytes: Uint8Array;
  try {
    const accountProofBytes = canonicalAccountOwnershipProofBytes({
      proof_type: envelope.account_proof.proof_type,
      platform_id: envelope.account_proof.platform_id,
      owner_user_id: envelope.account_proof.e.owner_user_id,
      nonce_b64: envelope.account_proof.e.nonce_b64,
      issued_at_unix_seconds: envelope.account_proof.e.issued_at_unix_seconds,
      expires_at_unix_seconds: envelope.account_proof.e.expires_at_unix_seconds,
    });
    publicNameSignature = decodeCanonicalEd25519SignatureBytes(
      envelope.signature_b64,
      "public-name proof signature",
    );
    publicKey = decodeCanonicalBase64(
      args.ownerEd25519PubB64,
      32,
      "public-name proof owner key",
    );
    publicNameProofBytes = canonicalPublicNameProofBytes({
      publicName: envelope.public_name,
      accountProofBytes,
    });
  } catch {
    return refusePublicNameProof("proof_malformed");
  }
  if (!(await verifyEd25519(publicKey, publicNameProofBytes, publicNameSignature))) {
    return refusePublicNameProof("proof_for_different_public_name");
  }
  if (envelope.public_name !== args.username) {
    return refusePublicNameProof("proof_for_different_public_name");
  }

  return {
    ok: true,
    nonceSha256,
    bindingSha256,
    expiresAtUnixSeconds: row.expires_at_unix_seconds,
  };
}

function refusePublicNameProof(reason: AccountOwnershipError): PublicNameProofCheck {
  const message = `public-name proof rejected: ${reason}`;
  if (reason === "proof_replayed") return { ok: false, response: conflict(message) };
  if (reason === "no_proof_presented" || reason === "proof_malformed" || reason === "unsupported_service") {
    return { ok: false, response: error(400, message) };
  }
  return { ok: false, response: forbidden(message) };
}
