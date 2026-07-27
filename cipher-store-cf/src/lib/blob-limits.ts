/// Aggregate ceilings for the generic `blobs` table.
///
/// The 2026-07-26 audit noted that generic blob insertion had no row or byte
/// quota behind the rate limiter at all: the limiter was the *only* bound, and
/// it was not atomic. Even with an atomic limiter, "budget per address" is not
/// a storage bound — it is a per-address bound that scales with the number of
/// addresses. This is the database-level backstop that does not.
///
/// Sizing: blobs are short-TTL pointers and control payloads capped at 64 KiB
/// each, with a maximum TTL of seven days. Two gibibytes is far above any
/// plausible legitimate steady state for the current user base while leaving
/// ample headroom inside D1's per-database storage limit for attachment
/// metadata, the control tables and the view-once lane. The row cap binds first
/// only for unusually small blobs.
export const MAX_LIVE_BLOB_ROWS = 100_000;
export const MAX_LIVE_BLOB_BYTES = 2 * 1024 * 1024 * 1024;
