/// IP-based rate limiting via Workers KV.
///
/// Phase 1 stopgap until Phase 6 lands Privacy Pass. Keys are
/// SHA-256 hashes of the client IP + route bucket, so even the KV
/// dump never contains a raw IP. KV entries auto-expire after the
/// window so there's no retained log of who hit us.
///
/// Budgets (per IP, per rolling window):
///   uploads:  600 / hour
///   fetches:  3600 / hour
///   deletes:  600 / hour
///
/// Returns true when the action is allowed and the counter has
/// been incremented; false when the cap has been hit (caller
/// returns HTTP 429).
///
/// Phase 6.3 bump: the prior 60/hr upload cap was tight enough that
/// an active GC user hit it in under two minutes. The fail-closed
/// V2 send-gate turned that into "messages grey out, never send"
/// for the user, with no graceful degradation path. New cap allows
/// 600 uploads/hr (10/min sustained) and 3600 fetches/hr, both
/// generous enough for normal chat use while still bounding the
/// damage from a single bad actor at a given IP. SKDMs are also
/// migrating to a keyserver inbox path (Phase 6.4) which should
/// further reduce per-send upload pressure.

import type { Env } from "../env.js";

const HOUR_SECONDS = 60 * 60;

// Rolling-window length. The counter key embeds the window start, so
// the count resets every WINDOW_SECONDS instead of accumulating
// forever.
const WINDOW_SECONDS = HOUR_SECONDS;

export type Bucket = "upload" | "fetch" | "delete";

const BUDGETS: Record<Bucket, number> = {
  upload: 600,
  fetch: 3600,
  delete: 600,
};

async function bucketKey(
  ip: string,
  bucket: Bucket,
  windowStart: number,
): Promise<string> {
  const buf = new TextEncoder().encode(`${bucket}|${ip}`);
  const hash = await crypto.subtle.digest("SHA-256", buf);
  const bytes = new Uint8Array(hash);
  let hex = "";
  for (const b of bytes) hex += b.toString(16).padStart(2, "0");
  // Truncate to 32 hex chars (128 bits) -- collision-safe for the
  // rate-limit purpose; we don't need full SHA-256 width. The
  // windowStart segment is what makes the counter reset each window.
  return `rl:${bucket}:${windowStart}:${hex.slice(0, 32)}`;
}

export async function rateLimit(
  env: Env,
  ip: string,
  bucket: Bucket
): Promise<{ allowed: boolean; remaining: number }> {
  // BUGFIX (server grey-out): the previous key had no window segment
  // and every write refreshed the TTL to a full hour, so the counter
  // never reset while the user kept chatting — after `budget`
  // cumulative uploads they were locked out until going idle for an
  // hour, which surfaced as "messages grey out, never send." Embedding
  // an aligned windowStart makes the count reset every WINDOW_SECONDS.
  const now = Math.floor(Date.now() / 1000);
  const windowStart = now - (now % WINDOW_SECONDS);
  const key = await bucketKey(ip, bucket, windowStart);
  // FAIL OPEN on any KV error. The free-tier KV write quota is
  // 1,000/day per account; once exhausted, `get`/`put` throw and the
  // upload endpoint started 500ing on every request ("server
  // failure"), which the send gate turns into a fail-closed abort —
  // i.e. nothing sends. Rate limiting is defence-in-depth, not a
  // correctness gate, so on KV failure we allow the request.
  try {
    const cur = await env.RATE_LIMIT.get(key);
    const used = cur ? parseInt(cur, 10) || 0 : 0;
    const budget = BUDGETS[bucket];
    if (used >= budget) {
      return { allowed: false, remaining: 0 };
    }
    // KV writes are eventually consistent (~60s convergence); for
    // rate limiting this is acceptable — a bad actor across multiple
    // edge POPs can briefly exceed the cap, but the absolute ceiling
    // is still bounded by `budget * pop_count` per window, which is
    // small enough to not threaten the service. TTL is 2× the window
    // so the current window's key always outlives the window itself.
    await env.RATE_LIMIT.put(key, String(used + 1), {
      expirationTtl: WINDOW_SECONDS * 2,
    });
    return { allowed: true, remaining: budget - used - 1 };
  } catch (err) {
    console.error("[rate-limit] KV error, failing open:", err);
    return { allowed: true, remaining: 0 };
  }
}
