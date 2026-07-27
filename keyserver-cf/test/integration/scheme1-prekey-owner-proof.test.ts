import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  canonicalIdentityBundleBytes,
  deriveCanonicalOslIdentityId,
  type CanonicalIdentityBundle,
} from "../../src/lib/identity-authority.js";
import {
  canonicalOpkBatchCommitmentBytes,
  canonicalOpkOwnerProofBytes,
  canonicalReplenishV2Bytes,
  identityBundleCommitmentB64,
  scheme1PrekeyContractSha256,
  type OpkOwnerProof,
  type ReplenishOpkV2,
  type ReplenishSpkV2,
  type Scheme1IdentityAuthority,
} from "../../src/lib/prekey-owner-proof.js";
import {
  canonicalPrekeyBundleGetBytes,
  canonicalReplenishBytes,
} from "../../src/lib/canonical.js";
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
let requestSequence = 1;
let registerIp = 210;

function requestId(): string {
  return base64Encode(
    new Uint8Array(32).fill((requestSequence++ % 250) + 1),
  ).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
}

function concat(parts: readonly Uint8Array[]): Uint8Array {
  const output = new Uint8Array(
    parts.reduce((sum, part) => sum + part.length, 0),
  );
  let offset = 0;
  for (const part of parts) {
    output.set(part, offset);
    offset += part.length;
  }
  return output;
}

function u8(value: number): Uint8Array {
  return new Uint8Array([value]);
}

function u32be(value: number): Uint8Array {
  const output = new Uint8Array(4);
  new DataView(output.buffer).setUint32(0, value, false);
  return output;
}

function lp(value: string): Uint8Array {
  const bytes = new TextEncoder().encode(value);
  return concat([u32be(bytes.length), bytes]);
}

function nonCanonicalEd25519Signature(signatureB64: string): string {
  const bytes = base64Decode(signatureB64);
  let order =
    (1n << 252n) + 27742317777372353535851937790883648493n;
  for (let index = 32; index < 64; index += 1) {
    bytes[index] = Number(order & 0xffn);
    order >>= 8n;
  }
  return base64Encode(bytes);
}

function sha256B64(bytes: Uint8Array): Promise<string> {
  return crypto.subtle.digest("SHA-256", bytes).then(
    (digest) => base64Encode(new Uint8Array(digest)),
  );
}

/**
 * Unsafe-by-design test encoder. Unlike the production helper, it permits
 * duplicate/mixed/swapped enclosing entries, then signs those exact bytes.
 * Thus a mutation rejection cannot pass merely because the outer signature
 * was invalid.
 */
function unsafeCanonicalReplenishV2(args: {
  user_id: string;
  timestamp_ms: number;
  request_id: string;
  spk: ReplenishSpkV2 | null;
  opks: readonly ReplenishOpkV2[];
}): Uint8Array {
  const parts: Uint8Array[] = [
    lp("discord-privacy-client/prekey-replenish/v2"),
    lp(args.user_id),
    lp(String(args.timestamp_ms)),
    lp(args.request_id),
    u8(args.spk ? 1 : 0),
  ];
  if (args.spk) {
    parts.push(
      lp(args.spk.pub_b64),
      lp(args.spk.signature_b64),
      lp(args.spk.rotated_at),
    );
  }
  parts.push(u32be(args.opks.length));
  for (const opk of args.opks) {
    const proofBytes = canonicalOpkOwnerProofBytes(opk.owner_proof);
    parts.push(
      u32be(opk.id),
      lp(opk.pub_b64),
      u32be(proofBytes.length),
      proofBytes,
      base64Decode(opk.owner_proof.signature_b64),
    );
  }
  return concat(parts);
}

async function createScheme1Identity(): Promise<{
  identity: Scheme1IdentityAuthority;
  rootSigningKey: CryptoKey;
  currentSigningKey: CryptoKey;
}> {
  const root = await generateEd25519Pair();
  const current = await generateEd25519Pair();
  const bundle: CanonicalIdentityBundle = {
    user_id: await deriveCanonicalOslIdentityId(root.publicKeyB64),
    identity_scheme: 1,
    identity_revision: 1,
    ik_root_ed25519_pub: root.publicKeyB64,
    ik_x25519_pub: STUB_X25519_PUB_B64,
    ik_ed25519_pub: current.publicKeyB64,
    ik_mlkem768_pub: STUB_MLKEM_PUB_B64,
    ik_ratchet_initial_pub: STUB_RATCHET_PUB_B64,
    rn_capabilities: 1,
  };
  const canonical = canonicalIdentityBundleBytes(bundle);
  const identity: Scheme1IdentityAuthority = {
    ...bundle,
    identity_bundle_proof_sig: await signEd25519(
      root.signingKey,
      canonical,
    ),
    registration_sig: await signEd25519(
      current.signingKey,
      canonical,
    ),
  };
  const response = await SELF.fetch("http://test/v1/register", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-forwarded-for": `192.0.2.${registerIp++}`,
    },
    body: JSON.stringify(identity),
  });
  expect(response.status).toBe(201);
  return {
    identity,
    rootSigningKey: root.signingKey,
    currentSigningKey: current.signingKey,
  };
}

async function makeSpk(
  signingKey: CryptoKey,
  fill: number,
  offsetMs = 0,
): Promise<ReplenishSpkV2> {
  const publicKey = new Uint8Array(32).fill(fill);
  return {
    pub_b64: base64Encode(publicKey),
    signature_b64: await signEd25519(signingKey, publicKey),
    rotated_at: new Date(Date.now() + offsetMs).toISOString(),
  };
}

async function makeProofBatch(args: {
  identity: Scheme1IdentityAuthority;
  signingKey: CryptoKey;
  spk: ReplenishSpkV2;
  generation: number;
  entries: readonly { id: number; fill: number }[];
}): Promise<ReplenishOpkV2[]> {
  const identityCommitment = await identityBundleCommitmentB64(args.identity);
  const bare = args.entries.map((entry) => ({
    id: entry.id,
    pub_b64: base64Encode(new Uint8Array(32).fill(entry.fill)),
  }));
  const batchCommitment = await sha256B64(
    canonicalOpkBatchCommitmentBytes({
      owner_user_id: args.identity.user_id,
      identity_bundle_commitment_b64: identityCommitment,
      spk: args.spk,
      lifecycle_generation: args.generation,
      opks: bare,
    }),
  );
  const output: ReplenishOpkV2[] = [];
  for (const opk of bare) {
    const unsigned: OpkOwnerProof = {
      version: 1,
      owner_user_id: args.identity.user_id,
      identity_bundle_commitment_b64: identityCommitment,
      identity_blob_version: 3,
      rn_capabilities: args.identity.rn_capabilities,
      lifecycle_version: 2,
      spk_pub_b64: args.spk.pub_b64,
      spk_signature_b64: args.spk.signature_b64,
      spk_rotated_at: args.spk.rotated_at,
      lifecycle_generation: args.generation,
      batch_size: bare.length,
      batch_commitment_b64: batchCommitment,
      opk_id: opk.id,
      opk_pub_b64: opk.pub_b64,
      signature_b64: "",
    };
    const signature = await signEd25519(
      args.signingKey,
      canonicalOpkOwnerProofBytes(unsigned),
    );
    output.push({
      ...opk,
      owner_proof: { ...unsigned, signature_b64: signature },
    });
  }
  return output;
}

async function replenishBody(args: {
  identity: Scheme1IdentityAuthority;
  signingKey: CryptoKey;
  spk: ReplenishSpkV2 | null;
  opks: ReplenishOpkV2[];
  unsafe?: boolean;
}): Promise<Record<string, unknown>> {
  const core = {
    user_id: args.identity.user_id,
    timestamp_ms: Date.now(),
    request_id: requestId(),
    spk: args.spk,
    opks: args.opks,
  };
  const canonical = args.unsafe
    ? unsafeCanonicalReplenishV2(core)
    : canonicalReplenishV2Bytes(core);
  return {
    protocol_version: 2,
    ...core,
    batch_signature_b64: await signEd25519(args.signingKey, canonical),
  };
}

async function post(body: Record<string, unknown>): Promise<Response> {
  return await SELF.fetch("http://test/v1/prekey-bundle/replenish", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

async function signedGet(
  recipientId: string,
): Promise<Response> {
  const requesterId = `scheme1-requester-${requestSequence++}`;
  const requester = await registerTestUser(SELF, requesterId);
  const timestamp = Date.now();
  const canonical = canonicalPrekeyBundleGetBytes({
    requester_id: requesterId,
    recipient_id: recipientId,
    timestamp_ms: timestamp,
  });
  const query = new URLSearchParams({
    requester_id: requesterId,
    recipient_id: recipientId,
    ts: String(timestamp),
    sig: await signEd25519(requester.signingKey, canonical),
  });
  return await SELF.fetch(
    `http://test/v1/prekey-bundle/${encodeURIComponent(recipientId)}?${query}`,
  );
}

describe("scheme-1 prekey owner proofs through the shipping Worker and D1", () => {
  it("pins the exact cross-language scheme-1 contract descriptor", async () => {
    expect(await scheme1PrekeyContractSha256()).toBe(
      "2febeb312152c66a3c79aae75b0ac3d0cd6d4c77b4b779de49123b0c9b839048",
    );
  });

  it("persists and atomically returns registration/capability/root evidence with the exact OPK proof", async () => {
    const owner = await createScheme1Identity();
    const spk = await makeSpk(owner.currentSigningKey, 0x51);
    const opks = await makeProofBatch({
      identity: owner.identity,
      signingKey: owner.currentSigningKey,
      spk,
      generation: 1,
      entries: [{ id: 9, fill: 0x61 }, { id: 4, fill: 0x62 }],
    });
    const body = await replenishBody({
      identity: owner.identity,
      signingKey: owner.currentSigningKey,
      spk,
      opks,
    });
    const accepted = await post(body);
    expect(accepted.status).toBe(200);
    expect(await accepted.json()).toMatchObject({
      protocol_version: 2,
      lifecycle_generation: 1,
      opks_added: 2,
    });
    expect(await testDb.prepare(
      `SELECT highest_generation, identity_bundle_commitment_b64,
              batch_commitment_b64
         FROM prekey_lifecycle_authority
        WHERE user_id = ?`,
    ).bind(owner.identity.user_id).first()).toMatchObject({
      highest_generation: 1,
      identity_bundle_commitment_b64:
        opks[0]!.owner_proof.identity_bundle_commitment_b64,
      batch_commitment_b64: opks[0]!.owner_proof.batch_commitment_b64,
    });

    const response = await signedGet(owner.identity.user_id);
    expect(response.status).toBe(200);
    const bundle = await response.json() as Record<string, any>;
    expect(bundle).toMatchObject({
      user_id: owner.identity.user_id,
      identity_scheme: 1,
      identity_revision: 1,
      ik_root_ed25519_pub: owner.identity.ik_root_ed25519_pub,
      identity_bundle_proof_sig:
        owner.identity.identity_bundle_proof_sig,
      registration_sig: owner.identity.registration_sig,
      rn_capabilities: 1,
      spk_pub: spk.pub_b64,
      remaining_opk_count: 1,
    });
    // Generation first, then id: both are generation 1, so id 4 is selected.
    expect(bundle.opk).toEqual({
      id: 4,
      pub_b64: opks[1]!.pub_b64,
      owner_proof: opks[1]!.owner_proof,
    });
  });

  it("refuses proof omission and a proofless v1 downgrade for a scheme-1 owner", async () => {
    const owner = await createScheme1Identity();
    const spk = await makeSpk(owner.currentSigningKey, 0x52);
    const opks = await makeProofBatch({
      identity: owner.identity,
      signingKey: owner.currentSigningKey,
      spk,
      generation: 1,
      entries: [{ id: 1, fill: 0x63 }],
    });
    const missing = await replenishBody({
      identity: owner.identity,
      signingKey: owner.currentSigningKey,
      spk,
      opks,
    });
    delete (missing.opks as Record<string, unknown>[])[0]!.owner_proof;
    expect((await post(missing)).status).toBe(400);

    const downgradeCore = {
      user_id: owner.identity.user_id,
      timestamp_ms: Date.now(),
      request_id: requestId(),
      spk,
      opks: [{ id: 1, pub_b64: opks[0]!.pub_b64 }],
    };
    const downgraded = {
      protocol_version: 1,
      ...downgradeCore,
      batch_signature_b64: await signEd25519(
        owner.currentSigningKey,
        canonicalReplenishBytes(downgradeCore),
      ),
    };
    expect((await post(downgraded)).status).toBe(400);
    await expect(testDb.prepare(
      `INSERT INTO opk_pool (user_id, opk_id, opk_pub)
       VALUES (?, 99, ?)`,
    ).bind(
      owner.identity.user_id,
      base64Encode(new Uint8Array(32).fill(0x7e)),
    ).run()).rejects.toThrow(/owner proof row is invalid/);
    expect(await testDb.prepare(
      "SELECT COUNT(*) AS count FROM opk_pool WHERE user_id = ?",
    ).bind(owner.identity.user_id).first()).toEqual({ count: 0 });
  });

  it("refuses duplicate, swapped, cross-owner, cross-SPK and mixed-batch proofs even under a valid outer signature", async () => {
    const owner = await createScheme1Identity();
    const other = await createScheme1Identity();
    const spk = await makeSpk(owner.currentSigningKey, 0x53);
    const otherSpk = await makeSpk(owner.currentSigningKey, 0x54);
    const batch = await makeProofBatch({
      identity: owner.identity,
      signingKey: owner.currentSigningKey,
      spk,
      generation: 1,
      entries: [{ id: 1, fill: 0x64 }, { id: 2, fill: 0x65 }],
    });
    const otherBatch = await makeProofBatch({
      identity: other.identity,
      signingKey: other.currentSigningKey,
      spk: await makeSpk(other.currentSigningKey, 0x55),
      generation: 1,
      entries: [{ id: 3, fill: 0x66 }],
    });
    const wrongSpkBatch = await makeProofBatch({
      identity: owner.identity,
      signingKey: owner.currentSigningKey,
      spk: otherSpk,
      generation: 1,
      entries: [{ id: 4, fill: 0x67 }],
    });

    const mutations: ReplenishOpkV2[][] = [
      [batch[0]!, batch[0]!],
      [{
        ...batch[0]!,
        // Same decoded 32 bytes, different textual encoding. A width-only
        // decoder would accept it unless canonical base64 is enforced.
        pub_b64: batch[0]!.pub_b64.replace(/=+$/u, ""),
      }],
      [{
        ...batch[0]!,
        owner_proof: {
          ...batch[0]!.owner_proof,
          // Correct 64-byte width but RFC 8032-noncanonical S=L.
          signature_b64: nonCanonicalEd25519Signature(
            batch[0]!.owner_proof.signature_b64,
          ),
        },
      }],
      [
        { ...batch[0]!, pub_b64: batch[1]!.pub_b64 },
        { ...batch[1]!, pub_b64: batch[0]!.pub_b64 },
      ],
      [otherBatch[0]!],
      [wrongSpkBatch[0]!],
      [batch[0]!, otherBatch[0]!],
    ];
    for (const opks of mutations) {
      const body = await replenishBody({
        identity: owner.identity,
        signingKey: owner.currentSigningKey,
        spk,
        opks,
        unsafe: true,
      });
      expect((await post(body)).status).toBe(400);
    }
    expect(await testDb.prepare(
      "SELECT COUNT(*) AS count FROM opk_pool WHERE user_id = ?",
    ).bind(owner.identity.user_id).first()).toEqual({ count: 0 });
  });

  it("retains the monotonic lifecycle floor across replay, alternate receipts and deletion attempts", async () => {
    const owner = await createScheme1Identity();
    const spk = await makeSpk(owner.currentSigningKey, 0x56);
    const first = await makeProofBatch({
      identity: owner.identity,
      signingKey: owner.currentSigningKey,
      spk,
      generation: 1,
      entries: [{ id: 1, fill: 0x68 }],
    });
    const firstBody = await replenishBody({
      identity: owner.identity,
      signingKey: owner.currentSigningKey,
      spk,
      opks: first,
    });
    expect((await post(firstBody)).status).toBe(200);
    expect((await post(firstBody)).status).toBe(409);

    const alternateReceipt = await replenishBody({
      identity: owner.identity,
      signingKey: owner.currentSigningKey,
      spk: null,
      opks: first,
    });
    expect((await post(alternateReceipt)).status).toBe(409);
    await expect(testDb.prepare(
      "DELETE FROM prekey_lifecycle_authority WHERE user_id = ?",
    ).bind(owner.identity.user_id).run()).rejects.toThrow(
      /cannot be reset/,
    );

    const second = await makeProofBatch({
      identity: owner.identity,
      signingKey: owner.currentSigningKey,
      spk,
      generation: 2,
      entries: [{ id: 2, fill: 0x69 }],
    });
    const secondResponse = await post(await replenishBody({
      identity: owner.identity,
      signingKey: owner.currentSigningKey,
      spk: null,
      opks: second,
    }));
    expect(secondResponse.status).toBe(200);
    expect(await testDb.prepare(
      `SELECT highest_generation FROM prekey_lifecycle_authority
        WHERE user_id = ?`,
    ).bind(owner.identity.user_id).first()).toEqual({
      highest_generation: 2,
    });

    const stale = await makeProofBatch({
      identity: owner.identity,
      signingKey: owner.currentSigningKey,
      spk,
      generation: 1,
      entries: [{ id: 3, fill: 0x6a }],
    });
    expect((await post(await replenishBody({
      identity: owner.identity,
      signingKey: owner.currentSigningKey,
      spk: null,
      opks: stale,
    }))).status).toBe(409);
    expect(await testDb.prepare(
      "SELECT COUNT(*) AS count FROM opk_pool WHERE user_id = ?",
    ).bind(owner.identity.user_id).first()).toEqual({ count: 2 });
  });
});
