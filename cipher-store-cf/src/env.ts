/// Environment bindings for the cipher-store Worker.
///
/// Kept minimal on purpose -- no Stripe, no admin token, no
/// keyserver-style secrets. The store accepts uploads from anyone
/// (rate-limited by IP via KV) and the subpoena-resistance story
/// rides on (a) E2E ciphertext, (b) short TTL, (c) no app logging.

export interface Env {
  DB: D1Database;
  RATE_LIMIT: KVNamespace;
}
