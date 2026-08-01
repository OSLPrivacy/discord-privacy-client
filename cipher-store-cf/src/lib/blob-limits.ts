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

/// Return whether `bytes` is a valid Padmé object length.
///
/// Padmé rounds a length up to a multiple whose granularity depends on the
/// length's binary exponent. The server checks that the final ciphertext
/// object already has that shape so an unpadded client cannot opt out.
export function isPadmeLength(bytes: number): boolean {
  if (!Number.isSafeInteger(bytes) || bytes <= 0) return false;

  const exponent = Math.floor(Math.log2(bytes));
  if (exponent === 0) return true;

  const significantBits = Math.floor(Math.log2(exponent)) + 1;
  const granularity = 2 ** (exponent - significantBits);
  return bytes % granularity === 0;
}
