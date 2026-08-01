/// Behavioural proof for POST /v1/account-ownership/proof.
///
/// Every assertion here is on observable behaviour: HTTP status, and the D1
/// rows the Worker did or did not write. Nothing asserts on source text.

import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
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
  generateEd25519Pair,
  registerTestUser,
  signEd25519,
} from "./helpers.js";

const testDb = (env as unknown as { DB: D1Database }).DB;

let seq = 0;
function nextSnowflake(): string {
  return `90000000000000${String(1000 + seq++)}`;
}
function nextOwnerId(): string {
  return `osl-owner-${crypto.randomUUID()}`;
}

type SigningPair = Awaited<ReturnType<typeof generateEd25519Pair>>;

interface Redeemable {
  challenge: IssuedAccountOwnershipChallenge;
  serviceAccountId: string;
  ownerUserId: string;
  pair: SigningPair;
}

async function issueChallenge(
  serviceAccountId: string,
  ownerUserId: string,
): Promise<IssuedAccountOwnershipChallenge> {
  const res = await SELF.fetch("http://test/v1/account-ownership/challenge", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "cf-connecting-ip": "198.51.100.200",
    },
    body: JSON.stringify({
      service: "discord",
      service_account_id: serviceAccountId,
      owner_user_id: ownerUserId,
      consent: true,
    }),
  });
  if (res.status !== 201) {
    throw new Error(`challenge issue failed: ${res.status} ${await res.text()}`);
  }
  return (await res.json()) as IssuedAccountOwnershipChallenge;
}

/** Register an owner identity and obtain a live challenge for it. */
async function prepare(): Promise<Redeemable> {
  const serviceAccountId = nextSnowflake();
  const ownerUserId = nextOwnerId();
  const pair = await registerTestUser(SELF, ownerUserId);
  const challenge = await issueChallenge(serviceAccountId, ownerUserId);
  return { challenge, serviceAccountId, ownerUserId, pair };
}

async function signProof(
  args: {
    challenge: IssuedAccountOwnershipChallenge;
    platformId?: string;
    ownerUserId?: string;
    signingKey: CryptoKey;
  },
): Promise<Record<string, unknown>> {
  const platformId = args.platformId ?? args.challenge.service_account_id;
  const ownerUserId = args.ownerUserId ?? args.challenge.owner_user_id;
  const signature = await signEd25519(
    args.signingKey,
    canonicalAccountOwnershipProofBytes({
      proof_type: ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1,
      platform_id: platformId,
      owner_user_id: ownerUserId,
      nonce_b64: args.challenge.nonce,
      issued_at_unix_seconds: args.challenge.issued_at_unix_seconds,
      expires_at_unix_seconds: args.challenge.expires_at_unix_seconds,
    }),
  );
  return {
    platform_id: platformId,
    proof_type: ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1,
    e: {
      owner_user_id: ownerUserId,
      nonce_b64: args.challenge.nonce,
      issued_at_unix_seconds: args.challenge.issued_at_unix_seconds,
      expires_at_unix_seconds: args.challenge.expires_at_unix_seconds,
      signature_b64: signature,
    },
  };
}

async function submit(body: unknown): Promise<Response> {
  return SELF.fetch("http://test/v1/account-ownership/proof", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "cf-connecting-ip": "198.51.100.201",
    },
    body: JSON.stringify(body),
  });
}

async function challengeRow(
  challenge: IssuedAccountOwnershipChallenge,
): Promise<{ spent_at_unix_seconds: number | null } | null> {
  return await testDb
    .prepare(
      `SELECT spent_at_unix_seconds FROM account_ownership_challenges
        WHERE nonce_sha256 = ?`,
    )
    .bind(await sha256Hex(base64Decode(challenge.nonce)))
    .first<{ spent_at_unix_seconds: number | null }>();
}

async function bindingRows(
  challenge: IssuedAccountOwnershipChallenge,
): Promise<Record<string, unknown>[]> {
  const result = await testDb
    .prepare(
      `SELECT binding_sha256, nonce_sha256, owner_user_id, service,
              proof_type, verified_at_unix_seconds
         FROM account_ownership_proof_bindings
        WHERE nonce_sha256 = ?`,
    )
    .bind(await sha256Hex(base64Decode(challenge.nonce)))
    .all<Record<string, unknown>>();
  return result.results ?? [];
}

describe("POST /v1/account-ownership/proof", () => {
  it("accepts a claimant self-signature for an arbitrary Discord snowflake without provider authentication", async () => {
    const serviceAccountId = nextSnowflake();
    const ownerUserId = nextOwnerId();
    const pair = await registerTestUser(SELF, ownerUserId);
    const challenge = await issueChallenge(serviceAccountId, ownerUserId);
    const proof = await signProof({
      challenge,
      signingKey: pair.signingKey,
    });

    const res = await submit({
      service: "discord",
      service_account_id: serviceAccountId,
      owner_user_id: ownerUserId,
      proof,
    });

    expect(res.status).toBe(201);
    expect(await bindingRows(challenge)).toHaveLength(1);
  });

  it("records the binding and spends the challenge for a valid proof", async () => {
    const ctx = await prepare();
    const proof = await signProof({
      challenge: ctx.challenge,
      signingKey: ctx.pair.signingKey,
    });

    const res = await submit({
      service: "discord",
      service_account_id: ctx.serviceAccountId,
      owner_user_id: ctx.ownerUserId,
      proof,
    });

    expect(res.status).toBe(201);
    const payload = (await res.json()) as Record<string, unknown>;
    expect(payload.result).toBe("account_ownership_proof_recorded");

    // spent_at_unix_seconds is written — the single-use marker exists.
    const row = await challengeRow(ctx.challenge);
    expect(row?.spent_at_unix_seconds).toEqual(expect.any(Number));

    // and the durable binding row exists.
    const bindings = await bindingRows(ctx.challenge);
    expect(bindings).toHaveLength(1);
    expect(bindings[0]).toMatchObject({
      binding_sha256: await sha256Hex(
        canonicalChallengeBindingBytes(ctx.challenge),
      ),
      owner_user_id: ctx.ownerUserId,
      service: "discord",
      proof_type: ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1,
      verified_at_unix_seconds: row?.spent_at_unix_seconds,
    });
    // D1 never sees the clear snowflake or the raw nonce.
    const stored = JSON.stringify(bindings[0]);
    expect(stored).not.toContain(ctx.serviceAccountId);
    expect(stored).not.toContain(ctx.challenge.nonce);
  });

  it("refuses a replay of a proof that already succeeded", async () => {
    const ctx = await prepare();
    const proof = await signProof({
      challenge: ctx.challenge,
      signingKey: ctx.pair.signingKey,
    });
    const body = {
      service: "discord",
      service_account_id: ctx.serviceAccountId,
      owner_user_id: ctx.ownerUserId,
      proof,
    };

    const first = await submit(body);
    expect(first.status).toBe(201);
    const afterFirst = await challengeRow(ctx.challenge);
    expect(afterFirst?.spent_at_unix_seconds).toEqual(expect.any(Number));

    // Byte-identical replay of the exact request that just worked.
    const replay = await submit(body);
    expect(replay.status).toBe(409);

    // Nothing moved: the spend stamp is unchanged and no second binding
    // was recorded.
    const afterReplay = await challengeRow(ctx.challenge);
    expect(afterReplay?.spent_at_unix_seconds).toBe(
      afterFirst?.spent_at_unix_seconds,
    );
    expect(await bindingRows(ctx.challenge)).toHaveLength(1);
  });

  it("refuses a proof signed by a key other than the registered identity", async () => {
    const ctx = await prepare();
    const forger = await generateEd25519Pair();
    const forged = await signProof({
      challenge: ctx.challenge,
      signingKey: forger.signingKey,
    });

    const res = await submit({
      service: "discord",
      service_account_id: ctx.serviceAccountId,
      owner_user_id: ctx.ownerUserId,
      proof: forged,
    });

    expect(res.status).toBe(403);
    // A refused proof burns nothing and binds nothing.
    const row = await challengeRow(ctx.challenge);
    expect(row?.spent_at_unix_seconds).toBeNull();
    expect(await bindingRows(ctx.challenge)).toHaveLength(0);

    // The honest owner can still redeem the challenge afterwards.
    const honest = await signProof({
      challenge: ctx.challenge,
      signingKey: ctx.pair.signingKey,
    });
    const ok = await submit({
      service: "discord",
      service_account_id: ctx.serviceAccountId,
      owner_user_id: ctx.ownerUserId,
      proof: honest,
    });
    expect(ok.status).toBe(201);
  });

  it("refuses a missing or malformed proof without touching the challenge", async () => {
    const ctx = await prepare();
    const base = {
      service: "discord",
      service_account_id: ctx.serviceAccountId,
      owner_user_id: ctx.ownerUserId,
    };

    expect((await submit(base)).status).toBe(400);
    expect((await submit({ ...base, proof: "not-an-object" })).status).toBe(400);
    expect((await submit({ ...base, proof: {} })).status).toBe(400);

    const valid = await signProof({
      challenge: ctx.challenge,
      signingKey: ctx.pair.signingKey,
    });
    const evidence = valid.e as Record<string, unknown>;
    // Wrong proof type: a shape the verifier must not accept.
    expect(
      (await submit({
        ...base,
        proof: { ...valid, proof_type: "totally_different_v9" },
      })).status,
    ).toBe(400);
    // Truncated signature.
    expect(
      (await submit({
        ...base,
        proof: { ...valid, e: { ...evidence, signature_b64: "AAAA" } },
      })).status,
    ).toBe(400);

    const row = await challengeRow(ctx.challenge);
    expect(row?.spent_at_unix_seconds).toBeNull();
    expect(await bindingRows(ctx.challenge)).toHaveLength(0);
  });

  it("refuses a proof bound to a different account or owner", async () => {
    const ctx = await prepare();
    const other = await prepare();

    // Real nonce, real signature over a DIFFERENT platform account.
    const swappedAccount = await signProof({
      challenge: ctx.challenge,
      platformId: other.serviceAccountId,
      signingKey: ctx.pair.signingKey,
    });
    expect(
      (await submit({
        service: "discord",
        service_account_id: other.serviceAccountId,
        owner_user_id: ctx.ownerUserId,
        proof: swappedAccount,
      })).status,
    ).toBe(403);

    // Another registered identity's key answering this owner's challenge.
    const swappedOwner = await signProof({
      challenge: ctx.challenge,
      signingKey: other.pair.signingKey,
    });
    expect(
      (await submit({
        service: "discord",
        service_account_id: ctx.serviceAccountId,
        owner_user_id: ctx.ownerUserId,
        proof: swappedOwner,
      })).status,
    ).toBe(403);

    // An unregistered owner cannot bind at all.
    const strayOwner = nextOwnerId();
    const strayChallenge = await issueChallenge(nextSnowflake(), strayOwner);
    const strayPair = await generateEd25519Pair();
    expect(
      (await submit({
        service: "discord",
        service_account_id: strayChallenge.service_account_id,
        owner_user_id: strayOwner,
        proof: await signProof({
          challenge: strayChallenge,
          signingKey: strayPair.signingKey,
        }),
      })).status,
    ).toBe(403);

    for (const challenge of [ctx.challenge, other.challenge, strayChallenge]) {
      expect((await challengeRow(challenge))?.spent_at_unix_seconds).toBeNull();
      expect(await bindingRows(challenge)).toHaveLength(0);
    }
  });
});
