/// Environment bindings for the cipher-store Worker.
///
/// Kept minimal on purpose -- no Stripe, no admin token, no
/// keyserver-style secrets. The store accepts uploads from anyone
/// (rate-limited by client address: atomically in D1 for writes and every
/// ciphertext read); its data-minimisation posture rests on E2E ciphertext, short
/// TTLs, and no per-blob app logs. The fetch allowance keeps only opaque
/// address keys plus request ids/caller classes for one limiting window so its
/// exact ceiling can be reconciled with independent edge and R2 observations.

export interface Env {
  /**
   * Ciphertext, attachment transport metadata, view-once links — and, since the
   * 2026-07-26 audit, the `rate_counters` table backing mutation rate limits.
   * See wrangler.toml for why that state moved here from KV.
   */
  DB: D1Database;
  /** Opaque, end-to-end encrypted attachment bodies. No object metadata. */
  ATTACHMENTS: R2Bucket;
  /** Opaque, end-to-end encrypted message payloads, addressed by capability digest. */
  PAYLOADS: R2Bucket;
  /** Legacy rollback binding; production admission no longer depends on KV. */
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
