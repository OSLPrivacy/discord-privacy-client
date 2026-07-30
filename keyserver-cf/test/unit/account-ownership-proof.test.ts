import { describe, expect, it } from "vitest";
import {
  ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1,
  canonicalAccountOwnershipProofBytes,
  type Account,
  type AccountOwnershipProof,
  verify_ownership_proof,
} from "../../src/lib/account-ownership-proof.js";
import {
  base64Encode,
  generateEd25519Pair,
  signEd25519,
} from "../integration/helpers.js";

const NOW = 1_800_000_000;

async function accountFixture(): Promise<{
  account: Account;
  proof: AccountOwnershipProof;
  otherPublicKeyB64: string;
}> {
  const owner = await generateEd25519Pair();
  const other = await generateEd25519Pair();
  const evidence = {
    owner_user_id: "osl-owner-id",
    nonce_b64: base64Encode(new Uint8Array(32).fill(0x41)),
    issued_at_unix_seconds: NOW - 30,
    expires_at_unix_seconds: NOW + 30,
    signature_b64: "",
  };
  const unsigned = {
    platform_id: "platform-account-id",
    proof_type: ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1,
    e: evidence,
  };
  const signature = await signEd25519(
    owner.signingKey,
    canonicalAccountOwnershipProofBytes({
      proof_type: unsigned.proof_type,
      platform_id: unsigned.platform_id,
      owner_user_id: evidence.owner_user_id,
      nonce_b64: evidence.nonce_b64,
      issued_at_unix_seconds: evidence.issued_at_unix_seconds,
      expires_at_unix_seconds: evidence.expires_at_unix_seconds,
    }),
  );
  const proof: AccountOwnershipProof = {
    ...unsigned,
    e: { ...evidence, signature_b64: signature },
  };
  return {
    proof,
    otherPublicKeyB64: other.publicKeyB64,
    account: {
      platform_id: "platform-account-id",
      owner_user_id: "osl-owner-id",
      owner_ed25519_pub_b64: owner.publicKeyB64,
      proof_challenge: {
        platform_id: "platform-account-id",
        owner_user_id: "osl-owner-id",
        nonce_b64: evidence.nonce_b64,
        issued_at_unix_seconds: evidence.issued_at_unix_seconds,
        expires_at_unix_seconds: evidence.expires_at_unix_seconds,
        spent: false,
      },
      ownership_proof: proof,
    },
  };
}

describe("account ownership proof verification", () => {
  it("verify_ownership_proof validates a submitted Account", async () => {
    const { account } = await accountFixture();

    await expect(verify_ownership_proof(account, NOW)).resolves.toEqual({
      ok: true,
    });
    expect(account.proof_challenge.spent).toBe(true);
    await expect(verify_ownership_proof(account, NOW)).resolves.toEqual({
      ok: false,
      error: "proof_replayed",
    });
  });

  it("refuses absence, account mismatch, owner mismatch, expiry and malformed evidence", async () => {
    const missing = (await accountFixture()).account;
    missing.ownership_proof = null;
    await expect(verify_ownership_proof(missing, NOW)).resolves.toEqual({
      ok: false,
      error: "no_proof_presented",
    });

    const differentAccount = (await accountFixture()).account;
    differentAccount.platform_id = "other-platform-account";
    await expect(
      verify_ownership_proof(differentAccount, NOW),
    ).resolves.toEqual({
      ok: false,
      error: "proof_for_different_account",
    });

    const differentOwner = (await accountFixture()).account;
    differentOwner.owner_user_id = "other-osl-owner";
    await expect(verify_ownership_proof(differentOwner, NOW)).resolves.toEqual({
      ok: false,
      error: "proof_for_different_owner",
    });

    const stale = (await accountFixture()).account;
    await expect(
      verify_ownership_proof(stale, NOW + 30),
    ).resolves.toEqual({
      ok: false,
      error: "proof_stale",
    });

    const malformed = (await accountFixture()).account;
    malformed.ownership_proof = {
      ...malformed.ownership_proof!,
      e: {
        ...malformed.ownership_proof!.e,
        signature_b64: "not-base64",
      },
    };
    await expect(verify_ownership_proof(malformed, NOW)).resolves.toEqual({
      ok: false,
      error: "proof_malformed",
    });
  });

  it("refuses a real signature made by a different owner key", async () => {
    const { account, otherPublicKeyB64 } = await accountFixture();
    account.owner_ed25519_pub_b64 = otherPublicKeyB64;

    await expect(verify_ownership_proof(account, NOW)).resolves.toEqual({
      ok: false,
      error: "proof_for_different_owner",
    });
    expect(account.proof_challenge.spent).toBe(false);
  });
});
