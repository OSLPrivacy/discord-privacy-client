import {
  decodeCanonicalBase64,
  decodeCanonicalEd25519SignatureBytes,
  type CanonicalIdentityBundle,
} from "./identity-authority.js";
import { buildRegMsg } from "./signed-request.js";
import { verifyEd25519 } from "./crypto.js";

export const REPLENISH_V2_DOMAIN =
  "discord-privacy-client/prekey-replenish/v2";
export const OPK_BATCH_COMMITMENT_DOMAIN =
  "discord-privacy-client/opk-owner-batch/v1";
export const OPK_OWNER_PROOF_DOMAIN =
  "discord-privacy-client/opk-owner-proof/v1";
export const OPK_OWNER_PROOF_VERSION = 1;
export const OPK_LIFECYCLE_VERSION = 2;
export const IDENTITY_BLOB_VERSION = 3;

/**
 * Normative, hashable scheme-1 prekey contract descriptor. The field order is
 * the byte order below; LP means u32-big-endian UTF-8 length followed by bytes.
 */
export const SCHEME1_PREKEY_CONTRACT_DESCRIPTOR = [
  "osl.keyserver.scheme1-prekey-owner-proof.v1",
  `replenish_domain=${REPLENISH_V2_DOMAIN}`,
  `batch_domain=${OPK_BATCH_COMMITMENT_DOMAIN}`,
  `proof_domain=${OPK_OWNER_PROOF_DOMAIN}`,
  `proof_version=${OPK_OWNER_PROOF_VERSION}`,
  `lifecycle_version=${OPK_LIFECYCLE_VERSION}`,
  `identity_blob_version=${IDENTITY_BLOB_VERSION}`,
  "identity_commitment=sha256(OSL-REGISTER-v1\\n||user_id||\\n||x25519_b64||\\n||ed25519_b64||\\n||mlkem768_b64||\\n||ratchet_b64_or_empty||\\n||rn_capabilities_decimal)",
  "batch=LP(domain)||u32(version)||LP(owner)||identity_commitment_32||LP(spk_pub_b64)||LP(spk_sig_b64)||LP(spk_rotated_at)||u64(generation)||u32(count)||sorted(u32(opk_id)||opk_pub_32)",
  "proof=LP(domain)||u32(version)||LP(owner)||identity_commitment_32||u32(identity_blob_version)||u8(caps_present)||u32(caps_if_present)||u32(lifecycle_version)||LP(spk_pub_b64)||LP(spk_sig_b64)||LP(spk_rotated_at)||u64(generation)||u32(batch_size)||batch_commitment_32||u32(opk_id)||LP(opk_pub_b64)",
  "replenish=LP(domain)||LP(user_id)||LP(timestamp_decimal)||LP(request_id)||u8(spk_present)||optional(LP(spk_pub_b64)||LP(spk_sig_b64)||LP(spk_rotated_at))||u32(count)||request_order(u32(opk_id)||LP(opk_pub_b64)||u32(proof_len)||proof||proof_signature_64)",
].join("\n");

const encoder = new TextEncoder();

function concat(parts: readonly Uint8Array[]): Uint8Array {
  const output = new Uint8Array(
    parts.reduce((total, part) => total + part.length, 0),
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
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffff_ffff) {
    throw new Error("canonical u32 is out of range");
  }
  const bytes = new Uint8Array(4);
  new DataView(bytes.buffer).setUint32(0, value, false);
  return bytes;
}

function u64be(value: number): Uint8Array {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error("canonical lifecycle generation is invalid");
  }
  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setBigUint64(0, BigInt(value), false);
  return bytes;
}

function lpText(value: string): Uint8Array {
  const bytes = encoder.encode(value);
  return concat([u32be(bytes.length), bytes]);
}

function base64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

export async function scheme1PrekeyContractSha256(): Promise<string> {
  const digest = new Uint8Array(
    await crypto.subtle.digest(
      "SHA-256",
      encoder.encode(SCHEME1_PREKEY_CONTRACT_DESCRIPTOR),
    ),
  );
  return Array.from(
    digest,
    (byte) => byte.toString(16).padStart(2, "0"),
  ).join("");
}

export interface Scheme1IdentityAuthority extends CanonicalIdentityBundle {
  identity_bundle_proof_sig: string;
  registration_sig: string;
}

export interface ReplenishSpkV2 {
  pub_b64: string;
  signature_b64: string;
  rotated_at: string;
}

export interface OpkOwnerProof {
  version: number;
  owner_user_id: string;
  identity_bundle_commitment_b64: string;
  identity_blob_version: number;
  rn_capabilities: number;
  lifecycle_version: number;
  spk_pub_b64: string;
  spk_signature_b64: string;
  spk_rotated_at: string;
  lifecycle_generation: number;
  batch_size: number;
  batch_commitment_b64: string;
  opk_id: number;
  opk_pub_b64: string;
  signature_b64: string;
}

export interface ReplenishOpkV2 {
  id: number;
  pub_b64: string;
  owner_proof: OpkOwnerProof;
}

function exactObject(
  value: unknown,
  keys: readonly string[],
  label: string,
): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  const record = value as Record<string, unknown>;
  const actual = Object.keys(record).sort();
  const expected = [...keys].sort();
  if (
    actual.length !== expected.length ||
    actual.some((key, index) => key !== expected[index])
  ) {
    throw new Error(`${label} fields are noncanonical`);
  }
  return record;
}

function positiveU32(value: unknown, label: string): number {
  if (
    !Number.isSafeInteger(value) ||
    (value as number) <= 0 ||
    (value as number) > 0xffff_ffff
  ) {
    throw new Error(`${label} must be a positive u32`);
  }
  return value as number;
}

function u32(value: unknown, label: string): number {
  if (
    !Number.isSafeInteger(value) ||
    (value as number) < 0 ||
    (value as number) > 0xffff_ffff
  ) {
    throw new Error(`${label} must be u32`);
  }
  return value as number;
}

function requiredText(value: unknown, label: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} is required`);
  }
  return value;
}

export function parseOpkOwnerProof(value: unknown): OpkOwnerProof {
  const proof = exactObject(
    value,
    [
      "version",
      "owner_user_id",
      "identity_bundle_commitment_b64",
      "identity_blob_version",
      "rn_capabilities",
      "lifecycle_version",
      "spk_pub_b64",
      "spk_signature_b64",
      "spk_rotated_at",
      "lifecycle_generation",
      "batch_size",
      "batch_commitment_b64",
      "opk_id",
      "opk_pub_b64",
      "signature_b64",
    ],
    "opk.owner_proof",
  );
  if (
    proof.version !== OPK_OWNER_PROOF_VERSION ||
    proof.identity_blob_version !== IDENTITY_BLOB_VERSION ||
    proof.lifecycle_version !== OPK_LIFECYCLE_VERSION
  ) {
    throw new Error("OPK owner proof version is unsupported");
  }
  const capabilities = u32(proof.rn_capabilities, "owner proof RN capabilities");
  if (capabilities > 0xffff) {
    throw new Error("owner proof RN capabilities are out of range");
  }
  if (
    !Number.isSafeInteger(proof.lifecycle_generation) ||
    (proof.lifecycle_generation as number) <= 0
  ) {
    throw new Error("owner proof lifecycle generation is invalid");
  }
  return {
    version: OPK_OWNER_PROOF_VERSION,
    owner_user_id: requiredText(proof.owner_user_id, "owner proof user_id"),
    identity_bundle_commitment_b64: requiredText(
      proof.identity_bundle_commitment_b64,
      "owner proof identity commitment",
    ),
    identity_blob_version: IDENTITY_BLOB_VERSION,
    rn_capabilities: capabilities,
    lifecycle_version: OPK_LIFECYCLE_VERSION,
    spk_pub_b64: requiredText(proof.spk_pub_b64, "owner proof SPK"),
    spk_signature_b64: requiredText(
      proof.spk_signature_b64,
      "owner proof SPK signature",
    ),
    spk_rotated_at: requiredText(
      proof.spk_rotated_at,
      "owner proof SPK rotation time",
    ),
    lifecycle_generation: proof.lifecycle_generation as number,
    batch_size: positiveU32(proof.batch_size, "owner proof batch size"),
    batch_commitment_b64: requiredText(
      proof.batch_commitment_b64,
      "owner proof batch commitment",
    ),
    opk_id: u32(proof.opk_id, "owner proof OPK id"),
    opk_pub_b64: requiredText(proof.opk_pub_b64, "owner proof OPK"),
    signature_b64: requiredText(
      proof.signature_b64,
      "owner proof signature",
    ),
  };
}

export function canonicalIdentityBundleCommitmentBytes(
  identity: Scheme1IdentityAuthority,
): Uint8Array {
  return buildRegMsg({
    user_id: identity.user_id,
    ik_x25519_pub: identity.ik_x25519_pub,
    ik_ed25519_pub: identity.ik_ed25519_pub,
    ik_mlkem768_pub: identity.ik_mlkem768_pub,
    ik_ratchet_initial_pub: identity.ik_ratchet_initial_pub,
    rn_capabilities: identity.rn_capabilities,
  });
}

export async function identityBundleCommitmentB64(
  identity: Scheme1IdentityAuthority,
): Promise<string> {
  return base64(
    new Uint8Array(
      await crypto.subtle.digest(
        "SHA-256",
        canonicalIdentityBundleCommitmentBytes(identity),
      ),
    ),
  );
}

export function canonicalOpkBatchCommitmentBytes(args: {
  owner_user_id: string;
  identity_bundle_commitment_b64: string;
  spk: ReplenishSpkV2;
  lifecycle_generation: number;
  opks: readonly { id: number; pub_b64: string }[];
}): Uint8Array {
  if (args.owner_user_id.length === 0 || args.opks.length === 0) {
    throw new Error("OPK batch proof context is empty");
  }
  const identityCommitment = decodeCanonicalBase64(
    args.identity_bundle_commitment_b64,
    32,
    "owner proof identity commitment",
  );
  const sorted = args.opks.map((opk) => ({
    id: u32(opk.id, "OPK id"),
    publicKey: decodeCanonicalBase64(opk.pub_b64, 32, "OPK public key"),
  })).sort((left, right) => left.id - right.id);
  const ids = new Set<number>();
  const keys = new Set<string>();
  for (const opk of sorted) {
    if (ids.has(opk.id) || keys.has(base64(opk.publicKey))) {
      throw new Error("OPK batch contains duplicate ids or public keys");
    }
    ids.add(opk.id);
    keys.add(base64(opk.publicKey));
  }
  return concat([
    lpText(OPK_BATCH_COMMITMENT_DOMAIN),
    u32be(OPK_OWNER_PROOF_VERSION),
    lpText(args.owner_user_id),
    identityCommitment,
    lpText(args.spk.pub_b64),
    lpText(args.spk.signature_b64),
    lpText(args.spk.rotated_at),
    u64be(args.lifecycle_generation),
    u32be(sorted.length),
    ...sorted.flatMap((opk) => [u32be(opk.id), opk.publicKey]),
  ]);
}

export function canonicalOpkOwnerProofBytes(
  proof: OpkOwnerProof,
): Uint8Array {
  const identityCommitment = decodeCanonicalBase64(
    proof.identity_bundle_commitment_b64,
    32,
    "owner proof identity commitment",
  );
  const batchCommitment = decodeCanonicalBase64(
    proof.batch_commitment_b64,
    32,
    "owner proof batch commitment",
  );
  decodeCanonicalBase64(proof.spk_pub_b64, 32, "owner proof SPK");
  decodeCanonicalEd25519SignatureBytes(
    proof.spk_signature_b64,
    "owner proof SPK signature",
  );
  decodeCanonicalBase64(proof.opk_pub_b64, 32, "owner proof OPK");
  return concat([
    lpText(OPK_OWNER_PROOF_DOMAIN),
    u32be(proof.version),
    lpText(proof.owner_user_id),
    identityCommitment,
    u32be(proof.identity_blob_version),
    u8(1),
    u32be(proof.rn_capabilities),
    u32be(proof.lifecycle_version),
    lpText(proof.spk_pub_b64),
    lpText(proof.spk_signature_b64),
    lpText(proof.spk_rotated_at),
    u64be(proof.lifecycle_generation),
    u32be(proof.batch_size),
    batchCommitment,
    u32be(proof.opk_id),
    lpText(proof.opk_pub_b64),
  ]);
}

export function canonicalReplenishV2Bytes(args: {
  user_id: string;
  timestamp_ms: number;
  request_id: string;
  spk: ReplenishSpkV2 | null;
  opks: readonly ReplenishOpkV2[];
}): Uint8Array {
  if (
    args.user_id.length === 0 ||
    !Number.isSafeInteger(args.timestamp_ms) ||
    args.timestamp_ms <= 0 ||
    args.request_id.length === 0 ||
    args.opks.length === 0
  ) {
    throw new Error("v2 replenish canonical context is invalid");
  }
  const first = args.opks[0]?.owner_proof;
  if (!first || first.batch_size !== args.opks.length) {
    throw new Error("v2 replenish is not the complete owner-proof batch");
  }
  const ids = new Set<number>();
  const keys = new Set<string>();
  const opkParts: Uint8Array[] = [];
  for (const opk of args.opks) {
    if (ids.has(opk.id) || keys.has(opk.pub_b64)) {
      throw new Error("v2 replenish contains duplicate OPKs");
    }
    ids.add(opk.id);
    keys.add(opk.pub_b64);
    const proof = opk.owner_proof;
    if (
      proof.opk_id !== opk.id ||
      proof.opk_pub_b64 !== opk.pub_b64 ||
      proof.owner_user_id !== args.user_id ||
      proof.lifecycle_generation !== first.lifecycle_generation ||
      proof.batch_size !== first.batch_size ||
      proof.batch_commitment_b64 !== first.batch_commitment_b64 ||
      proof.identity_bundle_commitment_b64 !==
        first.identity_bundle_commitment_b64 ||
      proof.spk_pub_b64 !== first.spk_pub_b64 ||
      proof.spk_signature_b64 !== first.spk_signature_b64 ||
      proof.spk_rotated_at !== first.spk_rotated_at ||
      proof.rn_capabilities !== first.rn_capabilities
    ) {
      throw new Error("v2 replenish mixes or downgrades OPK proofs");
    }
    const proofBytes = canonicalOpkOwnerProofBytes(proof);
    const signature = decodeCanonicalEd25519SignatureBytes(
      proof.signature_b64,
      "owner proof signature",
    );
    opkParts.push(
      u32be(opk.id),
      lpText(opk.pub_b64),
      u32be(proofBytes.length),
      proofBytes,
      signature,
    );
  }
  return concat([
    lpText(REPLENISH_V2_DOMAIN),
    lpText(args.user_id),
    lpText(String(args.timestamp_ms)),
    lpText(args.request_id),
    u8(args.spk ? 1 : 0),
    ...(args.spk
      ? [
        lpText(args.spk.pub_b64),
        lpText(args.spk.signature_b64),
        lpText(args.spk.rotated_at),
      ]
      : []),
    u32be(args.opks.length),
    ...opkParts,
  ]);
}

export async function validateScheme1OwnerProofBatch(args: {
  identity: Scheme1IdentityAuthority;
  spk: ReplenishSpkV2;
  opks: readonly ReplenishOpkV2[];
}): Promise<{
  identity_bundle_commitment_b64: string;
  batch_commitment_b64: string;
  lifecycle_generation: number;
}> {
  if (args.opks.length === 0) {
    throw new Error("scheme-1 replenish requires a nonempty OPK proof batch");
  }
  const first = args.opks[0]!.owner_proof;
  const expectedIdentity = await identityBundleCommitmentB64(args.identity);
  if (
    first.owner_user_id !== args.identity.user_id ||
    first.identity_bundle_commitment_b64 !== expectedIdentity ||
    first.rn_capabilities !== args.identity.rn_capabilities ||
    first.spk_pub_b64 !== args.spk.pub_b64 ||
    first.spk_signature_b64 !== args.spk.signature_b64 ||
    first.spk_rotated_at !== args.spk.rotated_at ||
    first.batch_size !== args.opks.length
  ) {
    throw new Error("OPK owner proof does not match authoritative identity/SPK");
  }
  const batchBytes = canonicalOpkBatchCommitmentBytes({
    owner_user_id: args.identity.user_id,
    identity_bundle_commitment_b64: expectedIdentity,
    spk: args.spk,
    lifecycle_generation: first.lifecycle_generation,
    opks: args.opks,
  });
  const expectedBatch = base64(
    new Uint8Array(await crypto.subtle.digest("SHA-256", batchBytes)),
  );
  if (first.batch_commitment_b64 !== expectedBatch) {
    throw new Error("OPK batch commitment does not match complete batch");
  }
  const currentKey = decodeCanonicalBase64(
    args.identity.ik_ed25519_pub,
    32,
    "current Ed25519 identity key",
  );
  for (const opk of args.opks) {
    const proof = opk.owner_proof;
    if (
      proof.owner_user_id !== first.owner_user_id ||
      proof.identity_bundle_commitment_b64 !== expectedIdentity ||
      proof.rn_capabilities !== args.identity.rn_capabilities ||
      proof.lifecycle_generation !== first.lifecycle_generation ||
      proof.batch_size !== first.batch_size ||
      proof.batch_commitment_b64 !== expectedBatch ||
      proof.spk_pub_b64 !== args.spk.pub_b64 ||
      proof.spk_signature_b64 !== args.spk.signature_b64 ||
      proof.spk_rotated_at !== args.spk.rotated_at ||
      proof.opk_id !== opk.id ||
      proof.opk_pub_b64 !== opk.pub_b64
    ) {
      throw new Error("v2 replenish mixes owner proof contexts");
    }
    const signature = decodeCanonicalEd25519SignatureBytes(
      proof.signature_b64,
      "owner proof signature",
    );
    if (
      !(await verifyEd25519(
        currentKey,
        canonicalOpkOwnerProofBytes(proof),
        signature,
      ))
    ) {
      throw new Error("OPK owner proof signature is invalid");
    }
  }
  return {
    identity_bundle_commitment_b64: expectedIdentity,
    batch_commitment_b64: expectedBatch,
    lifecycle_generation: first.lifecycle_generation,
  };
}
