/// IP-based rate limiting via Workers KV.
///
/// Phase 1 stopgap until Phase 6 lands Privacy Pass. Keys are
/// SHA-256 hashes of the client IP + route bucket, so even the KV
/// dump never contains a raw IP. KV entries auto-expire after the
/// window so there's no retained log of who hit us.
///
/// Budgets (per IP, per rolling window):
///   uploads:  60 / hour
///   fetches:  600 / hour
///   deletes:  60 / hour
///
/// Returns true when the action is allowed and the counter has
/// been incremented; false when the cap has been hit (caller
/// returns HTTP 429).

import type { Env } from "../env.js";

const HOUR_SECONDS = 60 * 60;

export type Bucket = "upload" | "fetch" | "delete";

const BUDGETS: Record<Bucket, number> = {
  upload: 60,
  fetch: 600,
  delete: 60,
};

async function bucketKey(ip: string, bucket: Bucket): Promise<string> {
  const buf = new TextEncoder().encode(`${bucket}|${ip}`);
  const hash = await crypto.subtle.digest("SHA-256", buf);
  const bytes = new Uint8Array(hash);
  let hex = "";
  for (const b of bytes) hex += b.toString(16).padStart(2, "0");
  // Truncate to 32 hex chars (128 bits) -- collision-safe for the
  // rate-limit purpose; we don't need full SHA-256 width.
  return `rl:${bucket}:${hex.slice(0, 32)}`;
}

export async function rateLimit(
  env: Env,
  ip: string,
  bucket: Bucket
): Promise<{ allowed: boolean; remaining: number }> {
  const key = await bucketKey(ip, bucket);
  const cur = await env.RATE_LIMIT.get(key);
  const used = cur ? parseInt(cur, 10) || 0 : 0;
  const budget = BUDGETS[bucket];
  if (used >= budget) {
    return { allowed: false, remaining: 0 };
  }
  // KV writes are eventually consistent (~60s convergence); for
  // rate limiting this is acceptable — a bad actor across multiple
  // edge POPs can briefly exceed the cap, but the absolute ceiling
  // is still bounded by `budget * pop_count` per hour, which is
  // small enough to not threaten the service.
  await env.RATE_LIMIT.put(key, String(used + 1), {
    expirationTtl: HOUR_SECONDS,
  });
  return { allowed: true, remaining: budget - used - 1 };
}
