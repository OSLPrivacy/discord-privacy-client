/// POST /v1/register — OPEN, Ed25519-self-signed registration.
///
/// SECURITY-CRITICAL. There is NO admin token and NO allowlist on
/// this route (any OSL client self-registers an opaque OSL identity on first
/// launch). Safety
/// currently comes from:
///   P1  proof-of-key-control: the request is Ed25519-signed by the
///       identity key being registered (verified against the
///       submitted ik_ed25519_pub).
///   P2  first-write-wins + owner-authenticated rotation: once a
///       user_id has a row, only a signature under the CURRENTLY
///       STORED ik_ed25519_pub can change it.
/// plus strict decoded-length validation (kills the "AAAA" poison
/// class) and defense-in-depth per-IP + per-user_id rate limiting.
///
/// The admin token / allowlist were removed FROM THIS ROUTE ONLY.
/// Separate client/admin bearers still gate legacy wrapped-key and
/// operator routes, but registration authentication does not depend
/// on either secret.
/// `OSL_KEYSERVER_ALLOWED_USERS` is retired (register was its only
/// consumer).

import type { Env } from "../env.js";
import {
  getSignedIdentity,
  getUserForVerify,
  insertUser,
  raiseRnCapabilities,
  rotateUserKeys,
} from "../lib/db.js";
import { badRequest, conflict, forbidden, json } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import { tooMany } from "../lib/http.js";
import {
  buildRegMsg,
  buildRotMsg,
  ed25519SelfTest,
  parseRnCapabilities,
  RN_CAP_MAX,
  verifySignedRequest,
} from "../lib/signed-request.js";
import { decodeBase64, isProtocolId, isPlainString } from "../lib/validation.js";

/** Per-IP register attempts per minute. Legit clients register ~once. */
const REGISTER_IP_MAX = 5;
/** Per-user_id register attempts per minute — blunts distributed
 *  squat-spray that rotates source IPs against ONE target. */
const REGISTER_USER_MAX = 5;

/** Required decoded byte lengths. */
const LEN_X25519 = 32;
const LEN_ED25519 = 32;
const LEN_MLKEM768 = 1184;
const LEN_RATCHET = 32;
const LEN_SIG = 64;

/** Decode + exact-length check. Returns an error message or null. */
function lenError(
  field: string,
  value: unknown,
  want: number,
): string | null {
  if (typeof value !== "string" || value.length === 0) {
    return `${field} must be valid base64`;
  }
  let bytes: Uint8Array;
  try {
    bytes = decodeBase64(value);
  } catch {
    return `${field} must be valid base64`;
  }
  if (bytes.length !== want) return `${field} wrong length`;
  return null;
}

export async function handleRegister(request: Request, env: Env): Promise<Response> {
  // Fail LOUD at the auth boundary if the runtime can't verify
  // Ed25519 — never silently 403 every registration.
  await ed25519SelfTest();

  // --- rate limit FIRST (cheap), before any crypto work ---
  const rlIp = await checkRateLimit(env, callerIp(request), REGISTER_IP_MAX, "register-ip");
  if (!rlIp.ok) return tooMany(rlIp.retryAfter);

  let body: Record<string, unknown>;
  try {
    body = (await request.json()) as Record<string, unknown>;
  } catch {
    return badRequest("malformed JSON body");
  }

  // --- presence ---
  const required = [
    "user_id",
    "ik_x25519_pub",
    "ik_ed25519_pub",
    "ik_mlkem768_pub",
    "registration_sig",
  ] as const;
  for (const field of required) {
    if (!(field in body)) return badRequest(`missing field: ${field}`);
  }
  if (!isProtocolId(body.user_id)) {
    return badRequest("user_id must be a bounded identifier without control characters");
  }
  const userId = body.user_id;

  // Per-user_id throttle (separate native bucket so it cannot collide
  // with the per-IP counter).
  const rlUser = await checkRateLimit(
    env,
    userId,
    REGISTER_USER_MAX,
    "rlreg",
  );
  if (!rlUser.ok) return tooMany(rlUser.retryAfter);

  // --- decoded-length validation (BEFORE signature work) ---
  const ratchet =
    body.ik_ratchet_initial_pub === undefined ||
    body.ik_ratchet_initial_pub === null
      ? null
      : body.ik_ratchet_initial_pub;
  const lenChecks: Array<[string, unknown, number]> = [
    ["ik_x25519_pub", body.ik_x25519_pub, LEN_X25519],
    ["ik_ed25519_pub", body.ik_ed25519_pub, LEN_ED25519],
    ["ik_mlkem768_pub", body.ik_mlkem768_pub, LEN_MLKEM768],
    ["registration_sig", body.registration_sig, LEN_SIG],
  ];
  if (ratchet !== null) {
    lenChecks.push(["ik_ratchet_initial_pub", ratchet, LEN_RATCHET]);
  }
  for (const [f, v, want] of lenChecks) {
    const e = lenError(f, v, want);
    if (e) return badRequest(e);
  }

  // --- signed protocol-capability advertisement (migration 0026) ---
  //
  // A malformed value is REFUSED, never coerced to 0. Coercion would
  // mean an attacker who corrupts one byte of this field downgrades the
  // record instead of breaking the signature, which is precisely the
  // attack the advertisement exists to stop.
  const caps = parseRnCapabilities(body.rn_capabilities);
  if (caps === null) {
    return badRequest(
      `rn_capabilities must be an integer in 0..${RN_CAP_MAX} when present`,
    );
  }

  const fields = {
    user_id: userId,
    ik_x25519_pub: body.ik_x25519_pub as string,
    ik_ed25519_pub: body.ik_ed25519_pub as string,
    ik_mlkem768_pub: body.ik_mlkem768_pub as string,
    ik_ratchet_initial_pub: ratchet as string | null,
    // `undefined` when the client did not send the field, which makes
    // `buildRegMsg` reconstruct the byte-identical legacy message every
    // currently-deployed client signs.
    rn_capabilities: caps.present ? caps.value : undefined,
  };
  // Persist the registration signature into the existing
  // ik_x25519_signature column (audit/debug — no new column /
  // migration). Its historical name is a misnomer.
  const regInput = {
    user_id: userId,
    ik_x25519_pub: fields.ik_x25519_pub,
    ik_ed25519_pub: fields.ik_ed25519_pub,
    ik_mlkem768_pub: fields.ik_mlkem768_pub,
    ik_x25519_signature: body.registration_sig as string,
    ik_ratchet_initial_pub: fields.ik_ratchet_initial_pub,
    rn_capabilities: caps.value,
  };

  // REG_MSG reconstructed from the PARSED fields (never raw bytes).
  const regMsg = buildRegMsg(fields);

  const existing = await getUserForVerify(env.DB, userId);

  // ---------- Case A: brand-new user_id ----------
  if (!existing) {
    const ok = await verifySignedRequest(
      fields.ik_ed25519_pub,
      regMsg,
      body.registration_sig as string,
    );
    if (!ok) return badRequest("registration_sig invalid");
    const { registered_at } = await insertUser(env.DB, regInput);
    return json(
      { user_id: userId, registered_at, rn_capabilities: caps.value },
      { status: 201 },
    );
  }

  // ---------- Case B: exists, same ik_ed25519_pub ----------
  if (existing.ik_ed25519_pub === fields.ik_ed25519_pub) {
    // Still prove key control (the stored == submitted key).
    const ok = await verifySignedRequest(
      fields.ik_ed25519_pub,
      regMsg,
      body.registration_sig as string,
    );
    if (!ok) return badRequest("registration_sig invalid");

    // Write-FREE no-op: ed25519 is the identity anchor. Any change
    // to the other keys MUST go through Case C (authenticated
    // rotation), never a silent Case-B update. Keeping this branch
    // write-free means the every-unlock re-register the client does
    // generates ZERO D1 writes and is fully replay-inert.
    //
    // The ONE exception is a capability RAISE. Advertising OSL-RN for
    // the first time does not rotate any key, so requiring Case C for
    // it would mean an identity could never gain a capability without
    // also replacing its keys. The raise keeps every property of this
    // branch that matters:
    //
    // - Still replay-inert. The update is `rn_capabilities < ?`, so a
    //   captured body applies at most once and every replay is a no-op.
    // - Still zero writes in the steady state. Once the bitmap matches,
    //   the every-unlock re-register writes nothing again.
    // - Still no key change. The raise is refused unless every
    //   submitted key field byte-equals the stored one, which keeps
    //   `(keys, bitmap, signature)` a triple that verifies together —
    //   the property a reader depends on.
    // - Never lowers. See `raiseRnCapabilities`.
    if (!caps.present) {
      return json({ user_id: userId, status: "noop" }, { status: 200 });
    }
    const stored = await getSignedIdentity(env.DB, userId);
    if (!stored) return json({ user_id: userId, status: "noop" }, { status: 200 });
    if (caps.value <= stored.rn_capabilities) {
      // Equal: nothing to do. Lower: deliberately ignored rather than
      // applied, and reported so the caller can see that the record
      // still advertises more than this build claims.
      return json(
        {
          user_id: userId,
          status: "noop",
          rn_capabilities: stored.rn_capabilities,
        },
        { status: 200 },
      );
    }
    const sameKeys =
      stored.ik_x25519_pub === fields.ik_x25519_pub &&
      stored.ik_mlkem768_pub === fields.ik_mlkem768_pub &&
      (stored.ik_ratchet_initial_pub ?? null) ===
        (fields.ik_ratchet_initial_pub ?? null);
    if (!sameKeys) {
      // A key change plus a capability change is a rotation. Refusing
      // here rather than raising keeps the stored signature covering
      // the stored record.
      return forbidden(
        "changing keys and rn_capabilities together requires an authenticated rotation",
      );
    }
    const raised = await raiseRnCapabilities(
      env.DB,
      userId,
      stored.ik_ed25519_pub,
      caps.value,
      body.registration_sig as string,
    );
    if (!raised) {
      // Lost a race with a concurrent raise or a rotation. Either way
      // the durable value is at least as high as ours.
      const now = await getSignedIdentity(env.DB, userId);
      return json(
        {
          user_id: userId,
          status: "noop",
          rn_capabilities: now?.rn_capabilities ?? stored.rn_capabilities,
        },
        { status: 200 },
      );
    }
    return json(
      {
        user_id: userId,
        status: "capabilities_raised",
        rn_capabilities: caps.value,
      },
      { status: 200 },
    );
  }

  // ---------- Case C: exists, DIFFERENT ik_ed25519_pub ----------
  // Rotation OR attack. Authorised ONLY by the currently-stored key.
  const rotation = body.rotation;
  const REJECT = forbidden(
    "user_id registered to a different key; rotation not authorized",
  );
  if (typeof rotation !== "object" || rotation === null) return REJECT;
  const rot = rotation as Record<string, unknown>;
  if (!isPlainString(rot.prev_ik_ed25519_pub)) return REJECT;
  if (!isPlainString(rot.prev_sig)) return REJECT;
  if (lenError("rotation.prev_sig", rot.prev_sig, LEN_SIG)) return REJECT;

  // (a) the rotation must be bound to the CURRENT stored key. Once a
  //     rotation lands the stored key changes, so a replayed old
  //     rotation message no longer matches → replay defeated.
  if (rot.prev_ik_ed25519_pub !== existing.ik_ed25519_pub) return REJECT;

  const rotMsg = buildRotMsg({
    user_id: userId,
    prev_ik_ed25519_pub: rot.prev_ik_ed25519_pub,
    new_ik_x25519_pub: fields.ik_x25519_pub,
    new_ik_ed25519_pub: fields.ik_ed25519_pub,
    new_ik_mlkem768_pub: fields.ik_mlkem768_pub,
    new_ik_ratchet_initial_pub: fields.ik_ratchet_initial_pub,
    // Same optional component as REG_MSG, so the outgoing key must
    // authorise the new bitmap as well as the new keys.
    rn_capabilities: fields.rn_capabilities,
  });

  // (b) old key authorises the change.
  const oldOk = await verifySignedRequest(
    existing.ik_ed25519_pub,
    rotMsg,
    rot.prev_sig,
  );
  if (!oldOk) return REJECT;

  // (c) new key proves possession (anti-griefing: can't rotate a
  //     victim onto a key you don't control).
  const newOk = await verifySignedRequest(
    fields.ik_ed25519_pub,
    regMsg,
    body.registration_sig as string,
  );
  if (!newOk) return REJECT;

  // A rotation re-states the whole record, including the bitmap, so it
  // is the one place a fully authorised request could LOWER an
  // advertised capability. Refuse instead.
  //
  // This is not hypothetical and does not need a key compromise: a
  // rolled-back build sends no `rn_capabilities` at all, so its
  // rotation would write 0 and silently strip a capability that peers
  // have already pinned. Refusing costs that build the ability to
  // rotate keys until it is rolled forward — loud, bounded, and
  // recoverable. The alternative is a silent authenticated downgrade.
  //
  // Raising through a rotation is fine and is allowed.
  const priorRecord = await getSignedIdentity(env.DB, userId);
  if (priorRecord && caps.value < priorRecord.rn_capabilities) {
    return conflict(
      "rotation would lower rn_capabilities; re-register with at least " +
        `${priorRecord.rn_capabilities} (capability advertisements never lower)`,
    );
  }

  const rotated = await rotateUserKeys(
    env.DB,
    regInput,
    existing.ik_ed25519_pub,
  );
  if (!rotated) {
    return conflict("identity key changed during rotation; retry");
  }
  const { last_rotated_at } = rotated;
  return json(
    {
      user_id: userId,
      status: "rotated",
      last_rotated_at,
      rn_capabilities: caps.value,
    },
    { status: 200 },
  );
}
