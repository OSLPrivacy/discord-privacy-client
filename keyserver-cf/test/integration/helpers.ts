/// Shared test helpers for integration tests. Generates Ed25519
/// keypairs (matching the real client's IK_Ed25519), b64-encodes
/// pubkeys, signs canonical byte strings.
///
/// All tests register against this admin token (set in
/// vitest.config.ts `bindings`).

export const TEST_ADMIN_TOKEN = "test-admin-token-do-not-ship";

/** Stable bytes for the non-Ed25519 fields the API requires. */
export const STUB_X25519_PUB_B64 = base64Encode(new Uint8Array(32).fill(0x11));
export const STUB_MLKEM_PUB_B64 = base64Encode(new Uint8Array(1184).fill(0x22));
export const STUB_RATCHET_PUB_B64 = base64Encode(new Uint8Array(32).fill(0x33));
export const STUB_SIGNATURE_B64 = base64Encode(new Uint8Array(64).fill(0x44));

export function base64Encode(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}

export function base64Decode(s: string): Uint8Array {
  const bin = atob(s);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/** Generate a fresh Ed25519 keypair and return both the raw public-key
 *  bytes (32) and a signing handle. */
export async function generateEd25519Pair(): Promise<{
  publicKey: Uint8Array;
  publicKeyB64: string;
  signingKey: CryptoKey;
}> {
  const pair = (await crypto.subtle.generateKey(
    { name: "Ed25519" },
    true,
    ["sign", "verify"],
  )) as CryptoKeyPair;
  const rawPub = (await crypto.subtle.exportKey("raw", pair.publicKey)) as ArrayBuffer;
  const pubBytes = new Uint8Array(rawPub);
  return {
    publicKey: pubBytes,
    publicKeyB64: base64Encode(pubBytes),
    signingKey: pair.privateKey,
  };
}

/** Sign arbitrary bytes with an Ed25519 private key. Returns the
 *  64-byte detached signature, b64-encoded for the wire. */
export async function signEd25519(
  signingKey: CryptoKey,
  message: Uint8Array,
): Promise<string> {
  const sigBuf = await crypto.subtle.sign({ name: "Ed25519" }, signingKey, message);
  return base64Encode(new Uint8Array(sigBuf));
}

/** Register a user against `SELF` with stub keys but a real Ed25519
 *  pub. Returns the keypair so subsequent burn / replenish requests
 *  can sign. */
export async function registerTestUser(
  self: { fetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response> },
  userId: string,
): Promise<{ publicKey: Uint8Array; publicKeyB64: string; signingKey: CryptoKey }> {
  const pair = await generateEd25519Pair();
  const res = await self.fetch("http://test/v1/register", {
    method: "POST",
    headers: {
      authorization: `Bearer ${TEST_ADMIN_TOKEN}`,
      "content-type": "application/json",
    },
    body: JSON.stringify({
      user_id: userId,
      ik_x25519_pub: STUB_X25519_PUB_B64,
      ik_ed25519_pub: pair.publicKeyB64,
      ik_mlkem768_pub: STUB_MLKEM_PUB_B64,
      ik_x25519_signature: STUB_SIGNATURE_B64,
      ik_ratchet_initial_pub: STUB_RATCHET_PUB_B64,
    }),
  });
  if (res.status !== 201 && res.status !== 200) {
    throw new Error(
      `registerTestUser(${userId}) failed: ${res.status} ${await res.text()}`,
    );
  }
  return pair;
}

export const ADMIN_HEADERS = {
  authorization: `Bearer ${TEST_ADMIN_TOKEN}`,
  "content-type": "application/json",
} as const;
