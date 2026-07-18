/// Environment bindings for the cipher-store Worker.
///
/// Kept minimal on purpose -- no Stripe, no admin token, no
/// keyserver-style secrets. The store accepts uploads from anyone
/// (best-effort rate-limited by IP via KV); its data-minimisation
/// posture rests on E2E ciphertext, short TTLs, and no per-blob app logs.

export interface Env {
  DB: D1Database;
  RATE_LIMIT: KVNamespace;
  /** Server-only key used to make short-lived rate-limit identifiers opaque. */
  RATE_LIMIT_HASH_KEY: string;
}
