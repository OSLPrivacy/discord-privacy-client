/// IP-based rate limiting. Two stores, chosen by what the bucket protects.
///
/// **Mutation buckets are atomic (D1).** The original limiter did `KV.get` →
/// compare → `KV.put(used + 1)` across two awaits. KV is eventually consistent,
/// so concurrent requests all read the same value and collapse into one
/// increment: the ceiling this file used to claim did not hold even inside a
/// single POP (2026-07-26 audit, "KV read/modify/write does not enforce
/// mutation rate limits"). Every write path now increments through one
/// conditional D1 statement, which SQLite serialises, so the budget is a real
/// ceiling rather than an approximation.
///
/// **Read buckets stay on KV.** They bound cost and scraping, not integrity;
/// they are the highest-volume routes; and they must fail *open* so a limiter
/// outage cannot make already-stored ciphertext unreadable. Paying a D1 write
/// on every fetch to tighten an availability-only control is the wrong trade.
///
/// Either way the stored key is the same opaque value: a truncated HMAC of
/// (bucket, client address) under a server-only key, so neither store can be
/// dumped to enumerate likely addresses, and rows/entries are removed once
/// their window closes.
///
/// READ BUCKETS ARE APPROXIMATE, NOT A CEILING. KV permits only one write per
/// second to the same key and rejects the rest. A single address issuing more
/// than one read per second in the same bucket therefore makes `put()` throw,
/// the catch below fails read buckets open by design, and those requests are
/// admitted without being counted. So the read numbers below are a cost control
/// that works at steady state, not a bound that holds during a burst. Only the
/// mutation buckets, which count in D1, are a real ceiling. Do not cite a read
/// budget as an enforced limit.
///
/// Budgets (per IP, per rolling window):
///   uploads:  600 / hour
///   fetches:  3600 / hour
///   deletes:  600 / hour
///   attachment upload requests/fetches/deletes: 140 / 120 / 60 per hour
///   multipart session creation: 24 / hour
///   view-once link create / fetch: 120 / 120 per hour
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

export type Bucket =
  | "upload"
  | "fetch"
  | "delete"
  | "attachment-upload"
  | "attachment-session"
  | "attachment-fetch"
  | "attachment-delete"
  | "link-create"
  | "link-fetch";

/// Buckets that gate a write. These use the atomic D1 counter and fail closed.
const MUTATION_BUCKETS: ReadonlySet<Bucket> = new Set<Bucket>([
  "upload",
  "delete",
  "attachment-upload",
  "attachment-session",
  "attachment-delete",
  "link-create",
]);

const BUDGETS: Record<Bucket, number> = {
  upload: 600,
  fetch: 3600,
  delete: 600,
  // A 512 MiB upload uses up to 65 bounded multipart requests plus session
  // creation/completion. This still permits only two full-size attempts/hour.
  "attachment-upload": 140,
  // Session creation gets its own, far smaller budget (audit HIGH-1). A session
  // reserves capacity before any ciphertext exists, so it is the expensive
  // request in the flow; keeping it out of the shared 140 also means a flood of
  // creations cannot consume the part-upload budget a caller needs to finish an
  // upload already in progress. Twelve full-size uploads an hour is far beyond
  // ordinary use, and holding the 64-slot reservation pool full now costs an
  // attacker at least eleven distinct addresses.
  "attachment-session": 24,
  "attachment-fetch": 120,
  "attachment-delete": 60,
  // View-once links. Creation is already grant-gated; this is the second
  // line. Retrieval is deliberately far below the 3600/hr `fetch` budget:
  // a link is meant to be opened once by one person from one browser, so
  // a legitimate IP never approaches 120/hr, while a scraper hammering
  // /v/<id>/fetch is stopped early.
  "link-create": 120,
  "link-fetch": 120,
};

async function bucketKey(
  secret: string,
  ip: string,
  bucket: Bucket,
  windowStart: number,
): Promise<string> {
  if (secret.length < 32) throw new Error("rate-limit hash key unavailable");
  const encoder = new TextEncoder();
  const key = await crypto.subtle.importKey(
    "raw",
    encoder.encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const mac = await crypto.subtle.sign(
    "HMAC",
    key,
    encoder.encode(`${bucket}|${ip}`),
  );
  const bytes = new Uint8Array(mac);
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
  const budget = BUDGETS[bucket];
  try {
    const key = await bucketKey(env.RATE_LIMIT_HASH_KEY, ip, bucket, windowStart);
    return MUTATION_BUCKETS.has(bucket)
      ? await admitAtomically(env, key, windowStart, budget)
      : await admitEventually(env, key, budget);
  } catch {
    // Reads may remain available during a limiter outage. Anonymous writes
    // fail closed so an outage cannot become an unbounded D1 storage or
    // deletion-abuse window.
    console.error("[rate-limit] limiter unavailable");
    return { allowed: !MUTATION_BUCKETS.has(bucket), remaining: 0 };
  }
}

/// One conditional statement, so the check and the increment cannot be
/// separated. `used` advances only while it is below the budget; when the
/// `DO UPDATE` predicate fails no row is returned and the request is refused.
/// Concurrent callers are serialised by D1, so `budget` is an exact ceiling.
async function admitAtomically(
  env: Env,
  key: string,
  windowStart: number,
  budget: number,
): Promise<{ allowed: boolean; remaining: number }> {
  const admitted = await env.DB.prepare(
    `INSERT INTO rate_counters (bucket_key, window_start, used)
     VALUES (?, ?, 1)
     ON CONFLICT(bucket_key) DO UPDATE SET used = used + 1
       WHERE rate_counters.used < ?
     RETURNING used`,
  ).bind(key, windowStart, budget).first<{ used: number }>();
  if (!admitted) return { allowed: false, remaining: 0 };
  return { allowed: true, remaining: Math.max(0, budget - admitted.used) };
}

/// KV path, retained for read buckets only. Still eventually consistent, and
/// deliberately so: over-admitting reads costs bandwidth, while under-admitting
/// them would make stored ciphertext unfetchable.
async function admitEventually(
  env: Env,
  key: string,
  budget: number,
): Promise<{ allowed: boolean; remaining: number }> {
  const cur = await env.RATE_LIMIT.get(key);
  const used = cur ? parseInt(cur, 10) || 0 : 0;
  if (used >= budget) return { allowed: false, remaining: 0 };
  // TTL is 2x the window so the current window's key always outlives the
  // window itself.
  await env.RATE_LIMIT.put(key, String(used + 1), {
    expirationTtl: WINDOW_SECONDS * 2,
  });
  return { allowed: true, remaining: budget - used - 1 };
}

/// Delete counters for windows that have closed. Called from the existing
/// five-minute cron; keeps limiter state strictly shorter-lived than the KV
/// entries it replaces.
export async function sweepRateCounters(env: Env): Promise<number> {
  const now = Math.floor(Date.now() / 1000);
  const windowStart = now - (now % WINDOW_SECONDS);
  const swept = await env.DB.prepare(
    "DELETE FROM rate_counters WHERE window_start < ?",
  ).bind(windowStart).run();
  return swept.meta?.changes ?? 0;
}
