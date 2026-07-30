import { describe, expect, it } from "vitest";
import vectors from "../fixtures/scheme1-contract-vectors.json";
import {
  canonicalIdentityBundleBytes,
  decodeCanonicalBase64,
  deriveCanonicalOslIdentityId,
  sha256Hex,
  validateCanonicalIdentityBundle,
} from "../../src/lib/identity-authority.js";
import {
  canonicalOpkBatchCommitmentBytes,
  canonicalOpkOwnerProofBytes,
  canonicalReplenishV2Bytes,
  identityBundleCommitmentB64,
  SCHEME1_PREKEY_CONTRACT_DESCRIPTOR,
  scheme1PrekeyContractSha256,
  scheme1ReplenishResponse,
  validateScheme1OwnerProofBatch,
  type ReplenishOpkV2,
  type ReplenishSpkV2,
  type Scheme1IdentityAuthority,
} from "../../src/lib/prekey-owner-proof.js";
import { canonicalPrekeyBundleGetBytes } from "../../src/lib/canonical.js";
import { verifyEd25519 } from "../../src/lib/crypto.js";
import { base64Decode, base64Encode } from "./helpers.js";

const identity = vectors.identity_registration
  .request as unknown as Scheme1IdentityAuthority;
const spk = vectors.signed_prekey.wire as ReplenishSpkV2;
const opks = vectors.replenish.request.opks as unknown as ReplenishOpkV2[];

describe("frozen scheme-1 cross-language vectors", () => {
  it("pins the exact descriptor and separates the public bundle version from Rust storage", async () => {
    expect(vectors.contract_descriptor.utf8).toBe(
      SCHEME1_PREKEY_CONTRACT_DESCRIPTOR,
    );
    expect(await scheme1PrekeyContractSha256()).toBe(
      vectors.contract_descriptor.sha256_hex,
    );
    expect(vectors.version_separation.identity_bundle_version).toMatchObject({
      value: 1,
    });
    expect(
      vectors.version_separation.rust_private_identity_blob_version
        .normative_for_this_fixture,
    ).toBe(false);
  });

  it("recomputes the opaque identity and verifies both exact full-bundle signatures", async () => {
    expect(await deriveCanonicalOslIdentityId(identity.ik_root_ed25519_pub))
      .toBe(identity.user_id);
    const canonical = canonicalIdentityBundleBytes(identity);
    expect(base64Encode(canonical)).toBe(
      vectors.identity_registration.canonical_full_bundle_bytes.base64,
    );
    expect(await sha256Hex(canonical)).toBe(
      vectors.identity_registration.canonical_full_bundle_bytes.sha256_hex,
    );
    expect(await identityBundleCommitmentB64(identity)).toBe(
      vectors.identity_registration.identity_bundle_commitment_b64,
    );
    const validated = await validateCanonicalIdentityBundle(
      identity,
      identity.identity_bundle_proof_sig,
      identity.registration_sig,
    );
    expect(validated.bundle_sha256).toBe(
      vectors.identity_registration.canonical_full_bundle_bytes.sha256_hex,
    );
  });

  it("recomputes the batch, every owner proof, and the outer replenish signature", async () => {
    const batchBytes = canonicalOpkBatchCommitmentBytes({
      owner_user_id: identity.user_id,
      identity_bundle_commitment_b64:
        vectors.identity_registration.identity_bundle_commitment_b64,
      spk,
      lifecycle_generation: vectors.opk_batch.lifecycle_generation,
      opks,
    });
    expect(base64Encode(batchBytes)).toBe(
      vectors.opk_batch.canonical_batch_bytes.base64,
    );
    expect(await sha256Hex(batchBytes)).toBe(
      vectors.opk_batch.canonical_batch_bytes.sha256_hex,
    );

    const currentKey = decodeCanonicalBase64(
      identity.ik_ed25519_pub,
      32,
      "fixture current key",
    );
    for (const [index, opk] of opks.entries()) {
      const proofBytes = canonicalOpkOwnerProofBytes(opk.owner_proof);
      expect(base64Encode(proofBytes)).toBe(
        vectors.opk_batch.opks[index]!.canonical_owner_proof_bytes.base64,
      );
      expect(await verifyEd25519(
        currentKey,
        proofBytes,
        decodeCanonicalBase64(
          opk.owner_proof.signature_b64,
          64,
          "fixture owner proof signature",
        ),
      )).toBe(true);
    }
    expect(await validateScheme1OwnerProofBatch({
      identity,
      spk,
      opks,
    })).toEqual({
      identity_bundle_commitment_b64:
        vectors.identity_registration.identity_bundle_commitment_b64,
      batch_commitment_b64: vectors.opk_batch.batch_commitment_b64,
      lifecycle_generation: 1,
    });

    const request = vectors.replenish.request;
    const replenishBytes = canonicalReplenishV2Bytes({
      user_id: request.user_id,
      timestamp_ms: request.timestamp_ms,
      request_id: request.request_id,
      spk: request.spk as ReplenishSpkV2,
      opks,
    });
    expect(base64Encode(replenishBytes)).toBe(
      vectors.replenish.canonical_request_bytes.base64,
    );
    expect(await verifyEd25519(
      currentKey,
      replenishBytes,
      decodeCanonicalBase64(
        request.batch_signature_b64,
        64,
        "fixture replenish signature",
      ),
    )).toBe(true);
    const stable = scheme1ReplenishResponse({
      user_id: request.user_id,
      lifecycle_generation: 1,
      batch_commitment_b64: vectors.opk_batch.batch_commitment_b64,
      opks_added: opks.length,
    });
    expect(stable).toEqual(vectors.replenish.expected_commit_response);
    expect(stable).toEqual(
      vectors.replenish.exact_request_replay_expected_response,
    );
    expect(stable).toEqual(
      vectors.replenish
        .authenticated_same_generation_same_batch_readback_expected_response,
    );
  });

  it("rejects a canonical-width but invalid owner-proof signature", async () => {
    const corrupted = structuredClone(opks) as ReplenishOpkV2[];
    corrupted[0]!.owner_proof.signature_b64 = base64Encode(
      new Uint8Array(64),
    );
    await expect(validateScheme1OwnerProofBatch({
      identity,
      spk,
      opks: corrupted,
    })).rejects.toThrow("OPK owner proof signature is invalid");
  });

  it("recomputes the consuming-fetch signature and freezes every scheme/root/lifecycle response field", async () => {
    const query = vectors.consuming_fetch.request.query;
    const fetchBytes = canonicalPrekeyBundleGetBytes({
      requester_id: query.requester_id,
      recipient_id: query.recipient_id,
      timestamp_ms: Number(query.ts),
    });
    expect(base64Encode(fetchBytes)).toBe(
      vectors.consuming_fetch.canonical_request_bytes.base64,
    );
    expect(await verifyEd25519(
      base64Decode(
        vectors.consuming_fetch.request.requester_ed25519_pub_b64,
      ),
      fetchBytes,
      base64Decode(query.sig),
    )).toBe(true);

    const response = vectors.consuming_fetch.expected_first_response;
    expect(response).toMatchObject({
      identity_scheme: 1,
      identity_bundle_version: 1,
      protocol_version: 2,
      lifecycle_version: 2,
      lifecycle_generation: 1,
      batch_commitment_b64: vectors.opk_batch.batch_commitment_b64,
      ik_root_ed25519_pub: identity.ik_root_ed25519_pub,
      identity_bundle_proof_sig: identity.identity_bundle_proof_sig,
      registration_sig: identity.registration_sig,
      rn_capabilities: identity.rn_capabilities,
      opk: {
        id: 4,
        owner_proof: opks[1]!.owner_proof,
      },
    });
    expect(vectors.consuming_fetch.exact_signed_request_replay).toEqual({
      http_status: 409,
      error: "signed prekey request already consumed",
      note:
        "the server does not replay a consuming fetch response because the OPK was consumed atomically",
    });
  });
});
