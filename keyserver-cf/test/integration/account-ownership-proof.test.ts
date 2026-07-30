import { describe, expect, it } from "vitest";
import {
  ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1,
  canonicalAccountOwnershipProofBytes,
  type Account,
  type AccountOwnershipProof,
  type ProofChallengeRecord,
  verify_ownership_proof,
} from "../../src/lib/account-ownership-proof.js";
import {
  base64Encode,
  generateEd25519Pair,
  signEd25519,
} from "./helpers.js";

const NOW = 1_800_000_000;

function issueChallenge(args: {
  platformId: string;
  ownerUserId: string;
  nonceFill: number;
}): ProofChallengeRecord {
  return {
    platform_id: args.platformId,
    owner_user_id: args.ownerUserId,
    nonce_b64: base64Encode(new Uint8Array(32).fill(args.nonceFill)),
    issued_at_unix_seconds: NOW - 30,
    expires_at_unix_seconds: NOW + 30,
    spent: false,
  };
}

async function answerChallenge(args: {
  challenge: ProofChallengeRecord;
  signingKey: CryptoKey;
}): Promise<AccountOwnershipProof> {
  const signature = await signEd25519(
    args.signingKey,
    canonicalAccountOwnershipProofBytes({
      proof_type: ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1,
      platform_id: args.challenge.platform_id,
      owner_user_id: args.challenge.owner_user_id,
      nonce_b64: args.challenge.nonce_b64,
      issued_at_unix_seconds: args.challenge.issued_at_unix_seconds,
      expires_at_unix_seconds: args.challenge.expires_at_unix_seconds,
    }),
  );
  return {
    platform_id: args.challenge.platform_id,
    proof_type: ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1,
    e: {
      owner_user_id: args.challenge.owner_user_id,
      nonce_b64: args.challenge.nonce_b64,
      issued_at_unix_seconds: args.challenge.issued_at_unix_seconds,
      expires_at_unix_seconds: args.challenge.expires_at_unix_seconds,
      signature_b64: signature,
    },
  };
}

async function roundTripAccount(): Promise<Account> {
  const owner = await generateEd25519Pair();
  const challenge = issueChallenge({
    platformId: "platform-account-id",
    ownerUserId: "owner-osl-id",
    nonceFill: 0x7a,
  });
  return {
    platform_id: challenge.platform_id,
    owner_user_id: challenge.owner_user_id,
    owner_ed25519_pub_b64: owner.publicKeyB64,
    proof_challenge: challenge,
    ownership_proof: await answerChallenge({
      challenge,
      signingKey: owner.signingKey,
    }),
  };
}

describe("account ownership proof challenge round trip", () => {
  it("(new) combine the challenge round trip and proof verification into one", async () => {
    const account = await roundTripAccount();

    await expect(verify_ownership_proof(account, NOW)).resolves.toEqual({
      ok: true,
    });
    expect(account.proof_challenge.spent).toBe(true);
    await expect(verify_ownership_proof(account, NOW)).resolves.toEqual({
      ok: false,
      error: "proof_replayed",
    });
  });

  it("refuses account, owner and expiry mutations after the challenge is answered", async () => {
    const wrongAccount = await roundTripAccount();
    wrongAccount.platform_id = "other-platform-account";
    await expect(verify_ownership_proof(wrongAccount, NOW)).resolves.toEqual({
      ok: false,
      error: "proof_for_different_account",
    });

    const wrongOwner = await roundTripAccount();
    wrongOwner.owner_user_id = "other-owner";
    await expect(verify_ownership_proof(wrongOwner, NOW)).resolves.toEqual({
      ok: false,
      error: "proof_for_different_owner",
    });

    const stale = await roundTripAccount();
    await expect(
      verify_ownership_proof(stale, NOW + 30),
    ).resolves.toEqual({
      ok: false,
      error: "proof_stale",
    });
  });
});
