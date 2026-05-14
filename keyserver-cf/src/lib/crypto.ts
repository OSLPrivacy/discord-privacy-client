/// Web Crypto helpers. Workers' SubtleCrypto supports Ed25519
/// verification (mid-2023+) and SHA-256 / HMAC out of the box,
/// which covers everything keyserver/src/server.js needed
/// node:crypto for.

/**
 * Constant-time bearer-token compare. Hashes both sides to SHA-256
 * first so the length of `expected` (the secret) doesn't leak via
 * the comparator's length precondition. Mirrors the Railway server's
 * `constantTimeTokenEqual`.
 */
export async function constantTimeTokenEqual(
  provided: string,
  expected: string,
): Promise<boolean> {
  const enc = new TextEncoder();
  const [hp, he] = await Promise.all([
    crypto.subtle.digest("SHA-256", enc.encode(provided)),
    crypto.subtle.digest("SHA-256", enc.encode(expected)),
  ]);
  return constantTimeBytesEqual(new Uint8Array(hp), new Uint8Array(he));
}

/** Constant-time byte equality. Both inputs must be the same length. */
export function constantTimeBytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= (a[i] ?? 0) ^ (b[i] ?? 0);
  return diff === 0;
}

/**
 * Verify a detached Ed25519 signature. Wraps SubtleCrypto so callers
 * pass raw Uint8Arrays for everything, matching the Node
 * `crypto.verify('ed25519', message, key, signature)` shape used in
 * keyserver/src/canonical.js.
 *
 * Returns `false` on any failure (key import error, malformed sig,
 * verification fail). Never throws — keeps the per-route handlers
 * simple.
 */
export async function verifyEd25519(
  publicKey32: Uint8Array,
  message: Uint8Array,
  signature64: Uint8Array,
): Promise<boolean> {
  if (publicKey32.length !== 32) return false;
  if (signature64.length !== 64) return false;
  try {
    const key = await crypto.subtle.importKey(
      "raw",
      publicKey32,
      { name: "Ed25519" },
      false,
      ["verify"],
    );
    return await crypto.subtle.verify({ name: "Ed25519" }, key, signature64, message);
  } catch {
    return false;
  }
}
