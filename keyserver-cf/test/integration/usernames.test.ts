import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { usernameClaimMessage } from "../../src/lib/username.js";
import { canonicalUnregisterBytes } from "../../src/lib/canonical.js";
import { buildRegMsg, buildRotMsg } from "../../src/lib/signed-request.js";
import {
  ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1,
  canonicalAccountOwnershipProofBytes,
} from "../../src/lib/account-ownership-proof.js";
import {
  canonicalChallengeBindingBytes,
  sha256Hex,
  type IssuedAccountOwnershipChallenge,
} from "../../src/lib/account-ownership-challenge.js";
import {
  base64Decode,
  base64Encode,
  generateEd25519Pair,
  registerTestUser,
  signEd25519,
  STUB_MLKEM_PUB_B64,
  STUB_RATCHET_PUB_B64,
  STUB_X25519_PUB_B64,
} from "./helpers.js";

const testDb = (env as unknown as { DB: D1Database }).DB;

let sequence = 0;
const userId = () => `username-user-${Date.now()}-${sequence++}`;
const snowflakeId = () => `9000000000000${String(100000 + sequence++).padStart(6, "0")}`;

function b64url(bytes: Uint8Array): string {
  let value = "";
  for (const byte of bytes) value += String.fromCharCode(byte);
  return btoa(value).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function friendCode(
  user_id: string,
  pair: { publicKeyB64: string; signingKey: CryptoKey },
): Promise<string> {
  const payload = {
    version: 1,
    osl_user_id: user_id,
    x25519_public: base64Encode(new Uint8Array(32).fill(0x11)),
    ed25519_public: pair.publicKeyB64,
    mlkem768_public: base64Encode(new Uint8Array(1184).fill(0x22)),
    ratchet_initial_public: base64Encode(new Uint8Array(32).fill(0x33)),
  };
  const signature = await crypto.subtle.sign(
    { name: "Ed25519" },
    pair.signingKey,
    new TextEncoder().encode(JSON.stringify(payload)),
  );
  const signed = JSON.stringify({ payload, signature: b64url(new Uint8Array(signature)) });
  return `OSLFR1.${b64url(new TextEncoder().encode(signed))}`;
}

/// D81. The lookup is `POST /v1/usernames/lookup` with the handle in the
/// BODY: the old `GET /v1/usernames/:username` wrote the handle a caller was
/// interested in into the platform's request-path record, next to that
/// caller's address. It also answers 200 for a miss and pads every body to a
/// fixed length, so these assertions read `found`, never the status.
async function lookup(username: string, ip: string): Promise<Response> {
  return SELF.fetch("http://test/v1/usernames/lookup", {
    method: "POST",
    headers: { "content-type": "application/json", "cf-connecting-ip": ip },
    body: JSON.stringify({ username }),
  });
}

async function looksUp(username: string, ip: string): Promise<boolean> {
  const response = await lookup(username, ip);
  if (response.status !== 200) throw new Error(`lookup status ${response.status}`);
  return ((await response.json()) as { found: boolean }).found;
}

async function claim(
  username: string,
  uid: string,
  pair: { publicKeyB64: string; signingKey: CryptoKey },
  invite = "",
  requestId = b64url(crypto.getRandomValues(new Uint8Array(32))),
  serviceAccountId = snowflakeId(),
) {
  await proveAccountOwnership(serviceAccountId, uid, pair);
  return rawClaim(username, uid, pair, serviceAccountId, invite, requestId);
}

async function rawClaim(
  username: string,
  uid: string,
  pair: { publicKeyB64: string; signingKey: CryptoKey },
  serviceAccountId: string | null,
  invite = "",
  requestId = b64url(crypto.getRandomValues(new Uint8Array(32))),
  timestamp_ms = Date.now(),
) {
  const friend_code = invite || await friendCode(uid, pair);
  const signature_b64 = await signEd25519(pair.signingKey, usernameClaimMessage({
    username, user_id: uid, friend_code, request_id: requestId, timestamp_ms,
  }));
  const body: Record<string, unknown> = {
    username,
    user_id: uid,
    friend_code,
    request_id: requestId,
    timestamp_ms,
    signature_b64,
  };
  if (serviceAccountId !== null) body.service_account_id = serviceAccountId;
  return SELF.fetch("http://test/v1/usernames/claim", {
    method: "POST",
    headers: { "content-type": "application/json", "cf-connecting-ip": `198.51.100.${sequence % 240 + 1}` },
    body: JSON.stringify(body),
  });
}

async function issueOwnershipChallenge(
  serviceAccountId: string,
  ownerUserId: string,
): Promise<IssuedAccountOwnershipChallenge> {
  const response = await SELF.fetch("http://test/v1/account-ownership/challenge", {
    method: "POST",
    headers: { "content-type": "application/json", "cf-connecting-ip": `198.51.100.${sequence % 240 + 1}` },
    body: JSON.stringify({
      service: "discord",
      service_account_id: serviceAccountId,
      owner_user_id: ownerUserId,
      consent: true,
    }),
  });
  if (response.status !== 201) {
    throw new Error(`ownership challenge failed: ${response.status} ${await response.text()}`);
  }
  return await response.json() as IssuedAccountOwnershipChallenge;
}

async function ownershipProof(
  challenge: IssuedAccountOwnershipChallenge,
  signingKey: CryptoKey,
): Promise<Record<string, unknown>> {
  const signature_b64 = await signEd25519(
    signingKey,
    canonicalAccountOwnershipProofBytes({
      proof_type: ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1,
      platform_id: challenge.service_account_id,
      owner_user_id: challenge.owner_user_id,
      nonce_b64: challenge.nonce,
      issued_at_unix_seconds: challenge.issued_at_unix_seconds,
      expires_at_unix_seconds: challenge.expires_at_unix_seconds,
    }),
  );
  return {
    platform_id: challenge.service_account_id,
    proof_type: ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1,
    e: {
      owner_user_id: challenge.owner_user_id,
      nonce_b64: challenge.nonce,
      issued_at_unix_seconds: challenge.issued_at_unix_seconds,
      expires_at_unix_seconds: challenge.expires_at_unix_seconds,
      signature_b64,
    },
  };
}

async function submitOwnershipProof(
  challenge: IssuedAccountOwnershipChallenge,
  signingKey: CryptoKey,
): Promise<Response> {
  return SELF.fetch("http://test/v1/account-ownership/proof", {
    method: "POST",
    headers: { "content-type": "application/json", "cf-connecting-ip": `198.51.100.${sequence % 240 + 1}` },
    body: JSON.stringify({
      service: "discord",
      service_account_id: challenge.service_account_id,
      owner_user_id: challenge.owner_user_id,
      proof: await ownershipProof(challenge, signingKey),
    }),
  });
}

async function proveAccountOwnership(
  serviceAccountId: string,
  ownerUserId: string,
  pair: { signingKey: CryptoKey },
): Promise<Response> {
  const challenge = await issueOwnershipChallenge(serviceAccountId, ownerUserId);
  const response = await submitOwnershipProof(challenge, pair.signingKey);
  if (response.status !== 201) {
    throw new Error(`ownership proof failed: ${response.status} ${await response.text()}`);
  }
  return response;
}

async function expiredChallenge(
  serviceAccountId: string,
  ownerUserId: string,
): Promise<IssuedAccountOwnershipChallenge> {
  const nonceBytes = crypto.getRandomValues(new Uint8Array(32));
  const now = Math.floor(Date.now() / 1000);
  const challenge: IssuedAccountOwnershipChallenge = {
    challenge_version: 1,
    service: "discord",
    service_account_id: serviceAccountId,
    owner_user_id: ownerUserId,
    nonce: base64Encode(nonceBytes),
    issued_at_unix_seconds: now - 600,
    expires_at_unix_seconds: now - 300,
    spent: false,
  };
  await testDb.prepare(
    `INSERT INTO account_ownership_challenges (
       nonce_sha256, binding_sha256, service, issued_at_unix_seconds,
       expires_at_unix_seconds, spent_at_unix_seconds
     ) VALUES (?, ?, 'discord', ?, ?, NULL)`,
  ).bind(
    await sha256Hex(base64Decode(challenge.nonce)),
    await sha256Hex(canonicalChallengeBindingBytes(challenge)),
    challenge.issued_at_unix_seconds,
    challenge.expires_at_unix_seconds,
  ).run();
  return challenge;
}

describe("username directory", () => {
  it("claims and resolves only an exact normalized username", async () => {
    const uid = userId();
    const pair = await registerTestUser(SELF, uid);
    const invite = await friendCode(uid, pair);
    expect((await claim("alice_01", uid, pair, invite)).status).toBe(200);
    expect((await claim("alice_01", uid, pair, invite)).status).toBe(200);
    const found = await lookup("alice_01", "203.0.113.10");
    expect(found.status).toBe(200);
    expect(await found.json()).toMatchObject({ found: true, username: "alice_01", friend_code: invite });
    expect((await lookup("Alice_01", "203.0.113.11")).status).toBe(400);
  });

  it("rejects unsigned, wrong-key, and mismatched-invite claims", async () => {
    const uid = userId();
    const owner = await registerTestUser(SELF, uid);
    const attacker = await generateEd25519Pair();
    const invite = await friendCode(uid, owner);
    const serviceAccountId = snowflakeId();
    await proveAccountOwnership(serviceAccountId, uid, owner);
    const timestamp_ms = Date.now();
    const request_id = b64url(crypto.getRandomValues(new Uint8Array(32)));
    const unsigned = await SELF.fetch("http://test/v1/usernames/claim", {
      method: "POST", headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.12" },
      body: JSON.stringify({ username: "unsigned", user_id: uid, service_account_id: serviceAccountId, friend_code: invite, request_id, timestamp_ms }),
    });
    expect(unsigned.status).toBe(400);
    const wrongSig = await signEd25519(attacker.signingKey, usernameClaimMessage({ username: "wrongkey", user_id: uid, friend_code: invite, request_id, timestamp_ms }));
    const wrong = await SELF.fetch("http://test/v1/usernames/claim", {
      method: "POST", headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.13" },
      body: JSON.stringify({ username: "wrongkey", user_id: uid, service_account_id: serviceAccountId, friend_code: invite, request_id, timestamp_ms, signature_b64: wrongSig }),
    });
    expect(wrong.status).toBe(401);
    const attackerInvite = await friendCode(uid, attacker);
    expect((await claim("badinvite", uid, owner, attackerInvite)).status).toBe(400);
  });

  it("rejects replayed and stale signed claims", async () => {
    const uid = userId();
    const pair = await registerTestUser(SELF, uid);
    const friend_code = await friendCode(uid, pair);
    const serviceAccountId = snowflakeId();
    await proveAccountOwnership(serviceAccountId, uid, pair);
    const request_id = b64url(crypto.getRandomValues(new Uint8Array(32)));
    const timestamp_ms = Date.now();
    const signature_b64 = await signEd25519(pair.signingKey, usernameClaimMessage({
      username: "replay_test", user_id: uid, friend_code, request_id, timestamp_ms,
    }));
    const init = {
      method: "POST",
      headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.19" },
      body: JSON.stringify({ username: "replay_test", user_id: uid, service_account_id: serviceAccountId, friend_code, request_id, timestamp_ms, signature_b64 }),
    };
    expect((await SELF.fetch("http://test/v1/usernames/claim", init)).status).toBe(200);
    expect((await SELF.fetch("http://test/v1/usernames/claim", init)).status).toBe(409);

    const staleTs = Date.now() - 5 * 60 * 1000 - 1;
    const staleId = b64url(crypto.getRandomValues(new Uint8Array(32)));
    const staleSig = await signEd25519(pair.signingKey, usernameClaimMessage({
      username: "stale_test", user_id: uid, friend_code, request_id: staleId, timestamp_ms: staleTs,
    }));
    const stale = await SELF.fetch("http://test/v1/usernames/claim", {
      method: "POST", headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.20" },
      body: JSON.stringify({ username: "stale_test", user_id: uid, service_account_id: serviceAccountId, friend_code, request_id: staleId, timestamp_ms: staleTs, signature_b64: staleSig }),
    });
    expect(stale.status).toBe(400);
  });

  it("retires names on rename and unregister so they can never be reclaimed", async () => {
    const one = userId();
    const two = userId();
    const pairOne = await registerTestUser(SELF, one);
    const pairTwo = await registerTestUser(SELF, two);
    expect((await claim("unique_name", one, pairOne)).status).toBe(200);
    expect((await claim("second_name", two, pairTwo)).status).toBe(200);
    expect((await claim("unique_name", two, pairTwo)).status).toBe(409);
    expect(await looksUp("second_name", "203.0.113.18")).toBe(true);
    expect((await claim("renamed_user", one, pairOne)).status).toBe(200);
    expect(await looksUp("unique_name", "203.0.113.14")).toBe(false);
    expect(await looksUp("renamed_user", "203.0.113.15")).toBe(true);
    expect((await claim("unique_name", two, pairTwo)).status).toBe(409);

    const timestamp_ms = Date.now();
    const message = canonicalUnregisterBytes({ user_id: one, timestamp_ms });
    const signature_b64 = await signEd25519(pairOne.signingKey, message);
    expect((await SELF.fetch(`http://test/v1/pubkeys/${encodeURIComponent(one)}`, {
      method: "DELETE", headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.16" },
      body: JSON.stringify({ signature_b64, timestamp_ms }),
    })).status).toBe(200);
    expect(await looksUp("renamed_user", "203.0.113.17")).toBe(false);
    expect((await claim("renamed_user", two, pairTwo)).status).toBe(409);
  });

  it("task 0448 rejects missing, wrong, and expired name proofs and requires the old key for a key move", async () => {
    const uid = userId();
    const owner = await registerTestUser(SELF, uid);
    const invite = await friendCode(uid, owner);
    const provedAccount = snowflakeId();
    const wrongAccount = snowflakeId();
    const expiredAccount = snowflakeId();

    const noProof = await rawClaim("task0448_noproof", uid, owner, provedAccount, invite);

    await proveAccountOwnership(provedAccount, uid, owner);
    const wrongAccountProof = await rawClaim(
      "task0448_wrong",
      uid,
      owner,
      wrongAccount,
      invite,
    );

    const expired = await expiredChallenge(expiredAccount, uid);
    const expiredProof = await submitOwnershipProof(expired, owner.signingKey);

    const goodClaim = await rawClaim("task0448_move", uid, owner, provedAccount, invite);
    expect(goodClaim.status).toBe(200);

    const next = await generateEd25519Pair();
    const fields = {
      user_id: uid,
      ik_x25519_pub: STUB_X25519_PUB_B64,
      ik_ed25519_pub: next.publicKeyB64,
      ik_mlkem768_pub: STUB_MLKEM_PUB_B64,
      ik_ratchet_initial_pub: STUB_RATCHET_PUB_B64,
    };
    const registration_sig = await signEd25519(next.signingKey, buildRegMsg(fields));
    const noOldKeyMove = await SELF.fetch("http://test/v1/register", {
      method: "POST",
      headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.44" },
      body: JSON.stringify({ ...fields, registration_sig }),
    });

    const prev_sig = await signEd25519(owner.signingKey, buildRotMsg({
      user_id: uid,
      prev_ik_ed25519_pub: owner.publicKeyB64,
      new_ik_x25519_pub: fields.ik_x25519_pub,
      new_ik_ed25519_pub: fields.ik_ed25519_pub,
      new_ik_mlkem768_pub: fields.ik_mlkem768_pub,
      new_ik_ratchet_initial_pub: fields.ik_ratchet_initial_pub,
    }));
    const completeMove = await SELF.fetch("http://test/v1/register", {
      method: "POST",
      headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.45" },
      body: JSON.stringify({
        ...fields,
        registration_sig,
        rotation: { prev_ik_ed25519_pub: owner.publicKeyB64, prev_sig },
      }),
    });

    console.log(`TASK0448_NO_PROOF_STATUS=${noProof.status}`);
    console.log(`TASK0448_WRONG_ACCOUNT_PROOF_STATUS=${wrongAccountProof.status}`);
    console.log(`TASK0448_EXPIRED_PROOF_STATUS=${expiredProof.status}`);
    console.log(`TASK0448_NO_OLD_KEY_MOVE_STATUS=${noOldKeyMove.status}`);
    console.log(`TASK0448_COMPLETE_MOVE_STATUS=${completeMove.status}`);

    expect(noProof.status).toBe(401);
    expect(wrongAccountProof.status).toBe(401);
    expect(expiredProof.status).toBe(403);
    expect(noOldKeyMove.status).toBe(403);
    expect(completeMove.status).toBe(200);
  });

  // ── PARKED SPEC — D-162. DO NOT DELETE. ──────────────────────────────────
  // These two tests are the executable specification of T5-K2/T5-K3's
  // canonical-username pipeline: a UTS #39 skeleton index that folds
  // confusables, plus a tombstone that keeps a retired skeleton unavailable.
  // They were written against `normalizeUsername()`, which commit db4172f7b
  // imported but never defined — its sibling d161329b7 never reached this
  // branch — so the Worker could not be bundled at all from 2026-08-02.
  //
  // db4172f7b is reverted here to restore the shipping validate-don't-transform
  // endpoint. The SPEC is not reverted: it is parked, OPEN as D-162, and these
  // two `it.skip` bodies are kept verbatim so the next implementer inherits the
  // assertions rather than re-deriving them.
  //
  // ── D-248 UPDATE, 2026-08-04. Read this before assuming the skeleton half
  // of the spec below is still unimplemented. ─────────────────────────────
  // The SKELETON half IS now live and covered, by
  // `test/integration/d248-confusable-username.test.ts`: the claim path
  // computes a real UTS #39 skeleton from the pinned artifact, both bodies'
  // assertions (a live collision is refused, and a retired skeleton stays
  // refused after a rename) are asserted there against handles the shipping
  // grammar accepts, and the other two writers of the column are covered too.
  //
  // These two stay SKIPPED because of the OTHER half only: their handles are
  // `Michael`/`Michae1`, whose capital is refused with 400 by
  // validate-don't-transform. Un-skipping them still needs a real
  // `normalizeUsername()`, which still does not exist. Do not un-skip them by
  // lowercasing the literals -- that would silently delete the transform half
  // of the spec they are parked to preserve.
  //
  // THEY DO NOT RUN AND THEY PROVE NOTHING. Skipped tests are not coverage.
  // Un-skip them only together with a real `normalizeUsername()` and a
  // migration plan for the identities already registered under
  // validate-don't-transform, whose names can fold together under a skeleton
  // index. The full spec, the migration hazard, and the deploy precondition
  // this revert creates are recorded in the plan registry, which lives OUTSIDE
  // this repo: plan-repo/plan-test/DEFECTS.md, entries D-162 and
  // D-162b. (Do not go looking for a DEFECTS.md in the repo; there isn't one.)
  it.skip("[D-162 PARKED] rejects a UTS #39 skeleton collision", async () => {
    const first = userId();
    const second = userId();
    const firstPair = await registerTestUser(SELF, first);
    const secondPair = await registerTestUser(SELF, second);
    expect((await claim("Michael", first, firstPair)).status).toBe(200);
    // The intentionally over-inclusive skeleton rejects a visually distinct
    // spelling rather than allowing an impersonation-adjacent handle.
    expect((await claim("Michae1", second, secondPair)).status).toBe(409);
  });

  it.skip("[D-162 PARKED] does not reissue a retired skeleton after a rename", async () => {
    const original = userId();
    const replacement = userId();
    const originalPair = await registerTestUser(SELF, original);
    const replacementPair = await registerTestUser(SELF, replacement);
    expect((await claim("Michael", original, originalPair)).status).toBe(200);
    expect((await claim("another_handle", original, originalPair)).status).toBe(200);
    // No live Michael row remains, so this specifically proves the tombstone
    // rejects the skeleton rather than relying on the live unique index.
    expect((await claim("Michae1", replacement, replacementPair)).status).toBe(409);
  });
  // ── end PARKED SPEC ──────────────────────────────────────────────────────

  it("removes a stale signed invite when the registered identity key rotates", async () => {
    const uid = userId();
    const owner = await registerTestUser(SELF, uid);
    expect((await claim("rotate_me", uid, owner)).status).toBe(200);
    const next = await generateEd25519Pair();
    const fields = {
      user_id: uid,
      ik_x25519_pub: STUB_X25519_PUB_B64,
      ik_ed25519_pub: next.publicKeyB64,
      ik_mlkem768_pub: STUB_MLKEM_PUB_B64,
      ik_ratchet_initial_pub: STUB_RATCHET_PUB_B64,
    };
    const registration_sig = await signEd25519(next.signingKey, buildRegMsg(fields));
    const prev_sig = await signEd25519(owner.signingKey, buildRotMsg({
      user_id: uid,
      prev_ik_ed25519_pub: owner.publicKeyB64,
      new_ik_x25519_pub: fields.ik_x25519_pub,
      new_ik_ed25519_pub: fields.ik_ed25519_pub,
      new_ik_mlkem768_pub: fields.ik_mlkem768_pub,
      new_ik_ratchet_initial_pub: fields.ik_ratchet_initial_pub,
    }));
    const rotated = await SELF.fetch("http://test/v1/register", {
      method: "POST", headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.21" },
      body: JSON.stringify({
        ...fields,
        registration_sig,
        rotation: { prev_ik_ed25519_pub: owner.publicKeyB64, prev_sig },
      }),
    });
    expect(rotated.status).toBe(200);
    expect(await looksUp("rotate_me", "203.0.113.22")).toBe(false);
    const replacementId = userId();
    const replacement = await registerTestUser(SELF, replacementId);
    expect((await claim("rotate_me", replacementId, replacement)).status).toBe(409);
  });
});
