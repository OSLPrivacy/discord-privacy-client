import {
  decodeCanonicalBase64,
  decodeCanonicalEd25519SignatureBytes,
} from "./identity-authority.js";
import { verifyEd25519 } from "./crypto.js";
import { isProtocolId } from "./validation.js";

export const ACCOUNT_OWNERSHIP_PROOF_DOMAIN =
  "OSL-ACCOUNT-OWNERSHIP-PROOF-v1\u0000";
export const ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1 =
  "ed25519_identity_challenge_v1";

export type AccountOwnershipError =
  | "no_proof_presented"
  | "proof_for_different_account"
  | "proof_for_different_owner"
  | "proof_stale"
  | "proof_replayed"
  | "proof_malformed"
  | "unsupported_service";

export interface AccountOwnershipEvidence {
  owner_user_id: string;
  nonce_b64: string;
  issued_at_unix_seconds: number;
  expires_at_unix_seconds: number;
  signature_b64: string;
}

export interface AccountOwnershipProof {
  platform_id: string;
  proof_type: string;
  e: AccountOwnershipEvidence;
}

export interface ProofChallengeRecord {
  platform_id: string;
  owner_user_id: string;
  nonce_b64: string;
  issued_at_unix_seconds: number;
  expires_at_unix_seconds: number;
  spent: boolean;
}

export interface Account {
  platform_id: string;
  owner_user_id: string;
  owner_ed25519_pub_b64: string;
  proof_challenge: ProofChallengeRecord;
  ownership_proof?: AccountOwnershipProof | null;
}

export type AccountOwnershipProofResult =
  | { ok: true }
  | { ok: false; error: AccountOwnershipError };

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

function u64be(value: number): Uint8Array {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error("canonical u64 is out of range");
  }
  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setBigUint64(0, BigInt(value), false);
  return bytes;
}

function lpText(value: string): Uint8Array {
  const bytes = encoder.encode(value);
  return concat([u32be(bytes.length), bytes]);
}

function exactObject(
  value: unknown,
  keys: readonly string[],
): value is Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return (
    actual.length === expected.length &&
    actual.every((key, index) => key === expected[index])
  );
}

function positiveUnix(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) > 0;
}

function parseProof(value: unknown): AccountOwnershipProof | null {
  if (
    !exactObject(value, ["platform_id", "proof_type", "e"]) ||
    typeof value.platform_id !== "string" ||
    typeof value.proof_type !== "string" ||
    !exactObject(value.e, [
      "owner_user_id",
      "nonce_b64",
      "issued_at_unix_seconds",
      "expires_at_unix_seconds",
      "signature_b64",
    ]) ||
    typeof value.e.owner_user_id !== "string" ||
    typeof value.e.nonce_b64 !== "string" ||
    typeof value.e.signature_b64 !== "string" ||
    !positiveUnix(value.e.issued_at_unix_seconds) ||
    !positiveUnix(value.e.expires_at_unix_seconds) ||
    value.e.issued_at_unix_seconds >= value.e.expires_at_unix_seconds
  ) {
    return null;
  }
  return {
    platform_id: value.platform_id,
    proof_type: value.proof_type,
    e: {
      owner_user_id: value.e.owner_user_id,
      nonce_b64: value.e.nonce_b64,
      issued_at_unix_seconds: value.e.issued_at_unix_seconds,
      expires_at_unix_seconds: value.e.expires_at_unix_seconds,
      signature_b64: value.e.signature_b64,
    },
  };
}

export function canonicalAccountOwnershipProofBytes(args: {
  proof_type: string;
  platform_id: string;
  owner_user_id: string;
  nonce_b64: string;
  issued_at_unix_seconds: number;
  expires_at_unix_seconds: number;
}): Uint8Array {
  if (
    args.proof_type !==
      ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1 ||
    !isProtocolId(args.platform_id) ||
    !isProtocolId(args.owner_user_id) ||
    !positiveUnix(args.issued_at_unix_seconds) ||
    !positiveUnix(args.expires_at_unix_seconds) ||
    args.issued_at_unix_seconds >= args.expires_at_unix_seconds
  ) {
    throw new Error("account ownership proof context is malformed");
  }
  const nonce = decodeCanonicalBase64(
    args.nonce_b64,
    32,
    "account ownership proof nonce",
  );
  return concat([
    lpText(ACCOUNT_OWNERSHIP_PROOF_DOMAIN),
    lpText(args.proof_type),
    lpText(args.platform_id),
    lpText(args.owner_user_id),
    nonce,
    u64be(args.issued_at_unix_seconds),
    u64be(args.expires_at_unix_seconds),
  ]);
}

export async function verify_ownership_proof(
  account: Account,
  now_unix_seconds: number,
): Promise<AccountOwnershipProofResult> {
  const proof = parseProof(account.ownership_proof);
  if (!proof) return { ok: false, error: "no_proof_presented" };
  if (
    proof.proof_type !==
      ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1
  ) {
    return { ok: false, error: "unsupported_service" };
  }
  if (
    proof.platform_id !== account.platform_id ||
    account.proof_challenge.platform_id !== account.platform_id ||
    proof.e.nonce_b64 !== account.proof_challenge.nonce_b64 ||
    proof.e.issued_at_unix_seconds !==
      account.proof_challenge.issued_at_unix_seconds ||
    proof.e.expires_at_unix_seconds !==
      account.proof_challenge.expires_at_unix_seconds
  ) {
    return { ok: false, error: "proof_for_different_account" };
  }
  if (
    proof.e.owner_user_id !== account.owner_user_id ||
    account.proof_challenge.owner_user_id !== account.owner_user_id
  ) {
    return { ok: false, error: "proof_for_different_owner" };
  }
  if (!Number.isSafeInteger(now_unix_seconds) || now_unix_seconds < 0) {
    return { ok: false, error: "proof_malformed" };
  }
  if (now_unix_seconds >= proof.e.expires_at_unix_seconds) {
    return { ok: false, error: "proof_stale" };
  }
  if (account.proof_challenge.spent) {
    return { ok: false, error: "proof_replayed" };
  }

  let publicKey: Uint8Array;
  let signature: Uint8Array;
  let message: Uint8Array;
  try {
    publicKey = decodeCanonicalBase64(
      account.owner_ed25519_pub_b64,
      32,
      "account owner Ed25519 key",
    );
    signature = decodeCanonicalEd25519SignatureBytes(
      proof.e.signature_b64,
      "account ownership proof signature",
    );
    message = canonicalAccountOwnershipProofBytes({
      proof_type: proof.proof_type,
      platform_id: proof.platform_id,
      owner_user_id: proof.e.owner_user_id,
      nonce_b64: proof.e.nonce_b64,
      issued_at_unix_seconds: proof.e.issued_at_unix_seconds,
      expires_at_unix_seconds: proof.e.expires_at_unix_seconds,
    });
  } catch {
    return { ok: false, error: "proof_malformed" };
  }
  if (!(await verifyEd25519(publicKey, message, signature))) {
    return { ok: false, error: "proof_for_different_owner" };
  }
  account.proof_challenge.spent = true;
  return { ok: true };
}
