import { verifyEd25519 } from "./crypto.js";
import { decodeBase64 } from "./validation.js";

const ID_DOMAIN = "OSL-ID-v1\u0000";
const BUNDLE_DOMAIN = "OSL-FULL-IDENTITY-BUNDLE-v1\u0000";
const ROLLOUT_GENESIS_DOMAIN = "OSL-SENDER-FILTER-ROLLOUT-GENESIS-v1\u0000";
const ROLLOUT_ADVANCE_DOMAIN = "OSL-SENDER-FILTER-ROLLOUT-ADVANCE-v1\u0000";

export const CANONICAL_IDENTITY_SCHEME = 1;
export const CANONICAL_IDENTITY_PREFIX = "osl1_";
export const CANONICAL_IDENTITY_ID_LENGTH = 57;
export const CANONICAL_IDENTITY_FRESHNESS_MS = 5 * 60 * 1000;

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
  return lp(encoder.encode(value));
}

function base64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function base32LowerNoPad(bytes: Uint8Array): string {
  const alphabet = "abcdefghijklmnopqrstuvwxyz234567";
  let accumulator = 0;
  let bits = 0;
  let output = "";
  for (const byte of bytes) {
    accumulator = (accumulator << 8) | byte;
    bits += 8;
    while (bits >= 5) {
      bits -= 5;
      output += alphabet[(accumulator >>> bits) & 0x1f];
      accumulator &= (1 << bits) - 1;
    }
  }
  if (bits > 0) {
    output += alphabet[(accumulator << (5 - bits)) & 0x1f];
  }
  return output;
}

function decodeCanonicalBase64(
  value: unknown,
  expectedLength: number,
  label: string,
): Uint8Array {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} must be canonical base64`);
  }
  let bytes: Uint8Array;
  try {
    bytes = decodeBase64(value);
  } catch {
    throw new Error(`${label} must be canonical base64`);
  }
  if (bytes.length !== expectedLength || base64(bytes) !== value) {
    throw new Error(`${label} must be canonical base64`);
  }
  return bytes;
}

function littleEndianInteger(bytes: Uint8Array): bigint {
  let value = 0n;
  for (let index = bytes.length - 1; index >= 0; index -= 1) {
    value = (value << 8n) | BigInt(bytes[index] ?? 0);
  }
  return value;
}

const ED25519_FIELD_PRIME = (1n << 255n) - 19n;
const ED25519_GROUP_ORDER =
  (1n << 252n) + 27742317777372353535851937790883648493n;

function isCanonicalEd25519PointEncoding(bytes: Uint8Array): boolean {
  if (bytes.length !== 32) return false;
  const encodedY = bytes.slice();
  encodedY[31] = (encodedY[31] ?? 0) & 0x7f;
  return littleEndianInteger(encodedY) < ED25519_FIELD_PRIME;
}

function decodeCanonicalEd25519PublicKey(
  value: unknown,
  label: string,
): Uint8Array {
  const bytes = decodeCanonicalBase64(value, 32, label);
  if (!isCanonicalEd25519PointEncoding(bytes)) {
    throw new Error(`${label} must use canonical Ed25519 encoding`);
  }
  return bytes;
}

function decodeCanonicalEd25519SignatureBytes(
  value: unknown,
  label: string,
): Uint8Array {
  const bytes = decodeCanonicalBase64(value, 64, label);
  if (
    !isCanonicalEd25519PointEncoding(bytes.subarray(0, 32)) ||
    littleEndianInteger(bytes.subarray(32)) >= ED25519_GROUP_ORDER
  ) {
    throw new Error(`${label} must use canonical Ed25519 encoding`);
  }
  return bytes;
}

function requirePositiveSafeInteger(value: unknown, label: string): number {
  if (!Number.isSafeInteger(value) || (value as number) <= 0) {
    throw new Error(`${label} must be a positive safe integer`);
  }
  return value as number;
}

function requireSha256(value: unknown, label: string): string {
  if (
    typeof value !== "string" ||
    !/^[0-9a-f]{64}$/u.test(value) ||
    value === "0".repeat(64)
  ) {
    throw new Error(`${label} must be a nonzero lowercase SHA-256`);
  }
  return value;
}

export interface CanonicalIdentityBundle {
  user_id: string;
  identity_scheme: 1;
  identity_revision: number;
  ik_root_ed25519_pub: string;
  ik_x25519_pub: string;
  ik_ed25519_pub: string;
  ik_mlkem768_pub: string;
  ik_ratchet_initial_pub: string | null;
  rn_capabilities: number;
}

export interface ValidatedCanonicalIdentityBundle {
  bundle: CanonicalIdentityBundle;
  canonical_bytes: Uint8Array;
  root_public_key: Uint8Array;
  current_ed25519_public_key: Uint8Array;
  bundle_sha256: string;
}

export async function deriveCanonicalOslIdentityId(
  rootPublicKeyB64: string,
): Promise<string> {
  const root = decodeCanonicalEd25519PublicKey(
    rootPublicKeyB64,
    "identity root key",
  );
  const digest = new Uint8Array(
    await crypto.subtle.digest(
      "SHA-256",
      concat([encoder.encode(ID_DOMAIN), root]),
    ),
  );
  return `${CANONICAL_IDENTITY_PREFIX}${base32LowerNoPad(digest)}`;
}

export function canonicalIdentityBundleBytes(
  value: CanonicalIdentityBundle,
): Uint8Array {
  const root = decodeCanonicalEd25519PublicKey(
    value.ik_root_ed25519_pub,
    "identity root key",
  );
  const x25519 = decodeCanonicalBase64(
    value.ik_x25519_pub,
    32,
    "X25519 identity key",
  );
  const ed25519 = decodeCanonicalEd25519PublicKey(
    value.ik_ed25519_pub,
    "Ed25519 identity key",
  );
  const mlkem = decodeCanonicalBase64(
    value.ik_mlkem768_pub,
    1184,
    "ML-KEM-768 identity key",
  );
  const ratchet = value.ik_ratchet_initial_pub === null
    ? new Uint8Array(0)
    : decodeCanonicalBase64(
      value.ik_ratchet_initial_pub,
      32,
      "ratchet bootstrap key",
    );
  if (value.identity_scheme !== CANONICAL_IDENTITY_SCHEME) {
    throw new Error("identity scheme must be canonical scheme 1");
  }
  requirePositiveSafeInteger(value.identity_revision, "identity revision");
  if (
    !Number.isSafeInteger(value.rn_capabilities) ||
    value.rn_capabilities < 0 ||
    value.rn_capabilities > 0xffff
  ) {
    throw new Error("RN capabilities are out of range");
  }
  return concat([
    lpText(BUNDLE_DOMAIN),
    lpText(value.user_id),
    u32be(value.identity_scheme),
    lpText(String(value.identity_revision)),
    lp(root),
    lp(x25519),
    lp(ed25519),
    lp(mlkem),
    lp(ratchet),
    u32be(value.rn_capabilities),
  ]);
}

export async function validateCanonicalIdentityBundle(
  bundle: CanonicalIdentityBundle,
  rootProofSignatureB64: string,
  currentKeySignatureB64: string,
): Promise<ValidatedCanonicalIdentityBundle> {
  const expectedUserId = await deriveCanonicalOslIdentityId(
    bundle.ik_root_ed25519_pub,
  );
  if (
    bundle.user_id !== expectedUserId ||
    bundle.user_id.length !== CANONICAL_IDENTITY_ID_LENGTH
  ) {
    throw new Error("user_id is not derived from the immutable identity root");
  }
  const canonicalBytes = canonicalIdentityBundleBytes(bundle);
  const root = decodeCanonicalEd25519PublicKey(
    bundle.ik_root_ed25519_pub,
    "identity root key",
  );
  const current = decodeCanonicalEd25519PublicKey(
    bundle.ik_ed25519_pub,
    "Ed25519 identity key",
  );
  const rootSignature = decodeCanonicalEd25519SignatureBytes(
    rootProofSignatureB64,
    "full-bundle root proof",
  );
  const currentSignature = decodeCanonicalEd25519SignatureBytes(
    currentKeySignatureB64,
    "current-key bundle proof",
  );
  if (!(await verifyEd25519(root, canonicalBytes, rootSignature))) {
    throw new Error("full-bundle root proof is invalid");
  }
  if (!(await verifyEd25519(current, canonicalBytes, currentSignature))) {
    throw new Error("current Ed25519 key proof is invalid");
  }
  const digest = new Uint8Array(
    await crypto.subtle.digest("SHA-256", canonicalBytes),
  );
  return {
    bundle,
    canonical_bytes: canonicalBytes,
    root_public_key: root,
    current_ed25519_public_key: current,
    bundle_sha256: Array.from(
      digest,
      (byte) => byte.toString(16).padStart(2, "0"),
    ).join(""),
  };
}

export function canonicalRolloutGenesisBytes(args: {
  root_user_id: string;
  genesis_nonce_sha256: string;
  timestamp_ms: number;
  request_id: string;
}): Uint8Array {
  requireSha256(args.genesis_nonce_sha256, "rollout genesis nonce digest");
  requirePositiveSafeInteger(args.timestamp_ms, "rollout genesis timestamp");
  return concat([
    lpText(ROLLOUT_GENESIS_DOMAIN),
    lpText(args.root_user_id),
    lpText(args.genesis_nonce_sha256),
    lpText(String(args.timestamp_ms)),
    lpText(args.request_id),
  ]);
}

export function canonicalRolloutAdvanceBytes(args: {
  root_user_id: string;
  expected_monotonic_version: number;
  observation_sha256: string;
  timestamp_ms: number;
  request_id: string;
}): Uint8Array {
  requirePositiveSafeInteger(
    args.expected_monotonic_version,
    "expected rollout version",
  );
  requireSha256(args.observation_sha256, "rollout observation digest");
  requirePositiveSafeInteger(args.timestamp_ms, "rollout advance timestamp");
  return concat([
    lpText(ROLLOUT_ADVANCE_DOMAIN),
    lpText(args.root_user_id),
    lpText(String(args.expected_monotonic_version)),
    lpText(args.observation_sha256),
    lpText(String(args.timestamp_ms)),
    lpText(args.request_id),
  ]);
}

export function decodeCanonicalEd25519Signature(value: unknown): Uint8Array {
  return decodeCanonicalEd25519SignatureBytes(
    value,
    "Ed25519 request signature",
  );
}

export async function verifyCanonicalEd25519Request(
  publicKeyB64: string,
  canonicalBytes: Uint8Array,
  signatureB64: unknown,
): Promise<boolean> {
  const publicKey = decodeCanonicalEd25519PublicKey(
    publicKeyB64,
    "Ed25519 request public key",
  );
  const signature = decodeCanonicalEd25519Signature(signatureB64);
  return await verifyEd25519(publicKey, canonicalBytes, signature);
}

export function decodeCanonicalNonce(value: unknown): Uint8Array {
  if (
    typeof value !== "string" ||
    !/^[A-Za-z0-9_-]{43}$/u.test(value)
  ) {
    throw new Error("rollout genesis nonce must be 32-byte base64url");
  }
  const padded = value.replaceAll("-", "+").replaceAll("_", "/") + "=";
  let bytes: Uint8Array;
  try {
    bytes = decodeBase64(padded);
  } catch {
    throw new Error("rollout genesis nonce must be 32-byte base64url");
  }
  const roundTrip = base64(bytes)
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/u, "");
  if (bytes.length !== 32 || roundTrip !== value) {
    throw new Error("rollout genesis nonce must be canonical");
  }
  return bytes;
}

export async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return Array.from(
    digest,
    (byte) => byte.toString(16).padStart(2, "0"),
  ).join("");
}
