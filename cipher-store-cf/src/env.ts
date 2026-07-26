/// Environment bindings for the cipher-store Worker.
///
/// Kept minimal on purpose -- no Stripe, no admin token, no
/// keyserver-style secrets. The store accepts uploads from anyone
/// (best-effort rate-limited by IP via KV); its data-minimisation
/// posture rests on E2E ciphertext, short TTLs, and no per-blob app logs.

export interface Env {
  DB: D1Database;
  /** Opaque, end-to-end encrypted attachment bodies. No object metadata. */
  ATTACHMENTS: R2Bucket;
  RATE_LIMIT: KVNamespace;
  /** Server-only key used to make short-lived rate-limit identifiers opaque. */
  RATE_LIMIT_HASH_KEY: string;
  /**
   * Base64 (standard or URL-safe) Ed25519 public key of the keyserver's
   * link-grant issuer. Gates `POST /v1/link` to authenticated OSL
   * clients. Absent => link creation is refused outright; an
   * unconfigured verifier must never degrade into an open, logless,
   * self-deleting file host. See `lib/link-grant.ts`.
   */
  LINK_GRANT_PUBKEY_B64?: string;
}
