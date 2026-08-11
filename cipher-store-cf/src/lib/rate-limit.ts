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
/// **Every ciphertext release uses one D1 allowance.** Blob, attachment and
/// view-once retrieval used to have three independent 120/hour buckets. Two of
/// those were KV read/modify/write counters, so concurrency could exceed even
/// their separate ceilings. TASK 6330 moves all three callers behind one
/// atomic, fail-closed address-wide gate at this deployed Worker boundary.
///
/// The stored address key is an opaque truncated HMAC under a server-only key,
/// so D1 cannot be dumped to enumerate likely addresses. Events are removed
/// once their rolling window closes.
///
/// Budgets (per IP, per rolling window):
///   uploads:  600 / hour
///   all ciphertext fetches, together: 120 / rolling hour / address
///   deletes:  600 / hour
///   attachment upload requests/deletes: 260 / 60 per hour
///   multipart session creation: 24 / hour
///   view-once link create: 120 / hour
///
/// Returns true when the action is allowed and the counter has
/// been incremented; false when the cap has been hit (caller
/// returns HTTP 429).
///
/// Phase 6.3 bump: the prior 60/hr upload cap was tight enough that
/// an active GC user hit it in under two minutes. The fail-closed
/// V2 send-gate turned that into "messages grey out, never send"
/// for the user, with no graceful degradation path. New cap allows
/// 600 uploads/hr (10/min sustained). Blob fetches are intentionally
/// lower: 120/hour is ample for eager receipt plus retries, while making
/// unauthenticated miss enumeration expensive. SKDMs are also
/// migrating to a keyserver inbox path (Phase 6.4) which should
/// further reduce per-send upload pressure.

import type { Env } from "../env.js";

export const FETCH_BUDGET = 120;
export const FETCH_LIMITING_WINDOW_SECONDS = 60 * 60;
const FETCH_LIMITING_WINDOW_MS = FETCH_LIMITING_WINDOW_SECONDS * 1000;
const HOUR_SECONDS = FETCH_LIMITING_WINDOW_SECONDS;

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

const FETCH_BUCKETS: ReadonlySet<Bucket> = new Set<Bucket>([
  "fetch",
  "attachment-fetch",
  "link-fetch",
]);

const BUDGETS: Record<Bucket, number> = {
  upload: 600,
  fetch: FETCH_BUDGET,
  delete: 600,
  // A 1 GiB upload uses up to 128 bounded multipart requests plus completion.
  // This still permits two full-size attempts/hour; session creation has its
  // own smaller budget below.
  "attachment-upload": 260,
  // Session creation gets its own, far smaller budget (audit HIGH-1). A session
  // reserves capacity before any ciphertext exists, so it is the expensive
  // request in the flow; keeping it out of the shared 260 also means a flood of
  // creations cannot consume the part-upload budget a caller needs to finish an
  // upload already in progress. Twelve full-size uploads an hour is far beyond
  // ordinary use, and holding the 64-slot reservation pool full now costs an
  // attacker at least eleven distinct addresses.
  "attachment-session": 24,
  "attachment-fetch": FETCH_BUDGET,
  "attachment-delete": 60,
  // View-once links. Creation is already grant-gated; this is the second
  // line. Retrieval uses the same 120/hr rate as anonymous blob fetch:
  // a link is meant to be opened once by one person from one browser, so
  // a legitimate IP never approaches 120/hr, while a scraper hammering
  // /v/<id>/fetch is stopped early.
  "link-create": 120,
  "link-fetch": FETCH_BUDGET,
};

export interface FetchBoundaryContext {
  /** Cloudflare Ray id when available; otherwise a server-generated id. */
  requestId?: string;
}

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
  bucket: Bucket,
  context: FetchBoundaryContext = {},
): Promise<{ allowed: boolean; remaining: number }> {
  if (FETCH_BUCKETS.has(bucket)) {
    try {
      return await admitAddressWideFetch(env, ip, bucket, context.requestId);
    } catch {
      console.error("[rate-limit] address-wide fetch limiter unavailable");
      return { allowed: false, remaining: 0 };
    }
  }
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
    return await admitAtomically(env, key, windowStart, budget);
  } catch {
    // Every bucket is fail closed so an outage cannot become an unbounded
    // storage, deletion-abuse, or capability-enumeration window.
    console.error("[rate-limit] limiter unavailable");
    return { allowed: false, remaining: 0 };
  }
}

function fetchCaller(bucket: Bucket): "blob-fetch" | "attachment-fetch" | "link-fetch" {
  switch (bucket) {
    case "fetch": return "blob-fetch";
    case "attachment-fetch": return "attachment-fetch";
    case "link-fetch": return "link-fetch";
    default: throw new Error("non-fetch bucket reached fetch allowance");
  }
}

async function addressKey(secret: string, ip: string): Promise<string> {
  // No bucket segment: all sibling callers from this address deliberately
  // collide in one allowance.
  return bucketKey(secret, ip, "fetch", 0);
}

async function admitAddressWideFetch(
  env: Env,
  ip: string,
  bucket: Bucket,
  suppliedRequestId?: string,
): Promise<{ allowed: boolean; remaining: number }> {
  const opaqueAddress = await addressKey(env.RATE_LIMIT_HASH_KEY, ip);
  const candidateRequestId = suppliedRequestId?.trim() ?? "";
  const requestId = /^[A-Za-z0-9-]{1,128}$/.test(candidateRequestId)
    ? candidateRequestId
    : crypto.randomUUID();
  const eventId = crypto.randomUUID();

  // One statement owns both the count and insert. D1 serialises statements, so
  // concurrent callers cannot all observe slot 120 and create a 121st event.
  // The timestamp comes from SQLite at the deployed-store boundary, not from a
  // client or OSL clock.
  // `>` gives the provider's rolling definition: an event exactly one window
  // old has left the window; every younger event still counts, including
  // traffic on opposite sides of a calendar-hour boundary.
  const admitted = await env.DB.prepare(
    `WITH clock(now_ms) AS (
       VALUES(CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
     )
     INSERT INTO fetch_budget_events
       (event_id, address_key, request_id, caller, observed_at_ms)
     SELECT ?, ?, ?, ?, clock.now_ms FROM clock
     WHERE (
       SELECT COUNT(*) FROM fetch_budget_events
       WHERE address_key = ?
         AND observed_at_ms > clock.now_ms - ?
     ) < ?
     RETURNING observed_at_ms`,
  ).bind(
    eventId,
    opaqueAddress,
    requestId,
    fetchCaller(bucket),
    opaqueAddress,
    FETCH_LIMITING_WINDOW_MS,
    FETCH_BUDGET,
  ).first<{ observed_at_ms: number }>();
  if (!admitted) return { allowed: false, remaining: 0 };

  const row = await env.DB.prepare(
    `SELECT COUNT(*) AS used FROM fetch_budget_events
     WHERE address_key = ? AND observed_at_ms > ?`,
  ).bind(
    opaqueAddress,
    admitted.observed_at_ms - FETCH_LIMITING_WINDOW_MS,
  ).first<{ used: number }>();
  const used = Number(row?.used ?? FETCH_BUDGET);
  return { allowed: true, remaining: Math.max(0, FETCH_BUDGET - used) };
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

/// Delete counters for windows that have closed. Called from the existing
/// five-minute cron.
export async function sweepRateCounters(env: Env): Promise<number> {
  const now = Math.floor(Date.now() / 1000);
  const windowStart = now - (now % WINDOW_SECONDS);
  const swept = await env.DB.prepare(
    "DELETE FROM rate_counters WHERE window_start < ?",
  ).bind(windowStart).run();
  const fetchSwept = await env.DB.prepare(
    `DELETE FROM fetch_budget_events
     WHERE observed_at_ms <=
       CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER) - ?`,
  ).bind(FETCH_LIMITING_WINDOW_MS).run();
  return (swept.meta?.changes ?? 0) + (fetchSwept.meta?.changes ?? 0);
}
