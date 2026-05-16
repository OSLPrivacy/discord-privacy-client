/// Per-IP rate limit on mutation routes, backed by Workers KV.
///
/// Algorithm: fixed-window counter at minute granularity. Each
/// mutation request `GET`s `rl:<ip>:<minute>` from KV, increments,
/// and `PUT`s back with TTL=120s (window covers current + next
/// minute so the cleanup is automatic). Above `max`, returns the
/// number of seconds until the window flips.
///
/// This is intentionally weaker than the Fastify rate-limit plugin
/// (which used in-memory state and had per-route precision). KV's
/// global write consistency is eventual, so a brief burst above the
/// limit is possible during edge-replication. For the closed-beta
/// threat model — defence-in-depth on top of admin-token gating —
/// this is plenty.
///
/// Skipped entirely when no admin token is configured (dev mode).

import type { Env } from "../env.js";

const WINDOW_SEC = 60;
const KV_TTL_SEC = 120;

export interface RateLimitDecision {
  ok: boolean;
  /** Seconds until the window flips (only meaningful when !ok). */
  retryAfter: number;
}

export async function checkRateLimit(
  env: Env,
  ip: string,
  max: number,
  bucket = "rl",
): Promise<RateLimitDecision> {
  // No-op when there's no token configured. Mirrors the Railway
  // server: rate-limit plugin only registers when ADMIN_TOKEN is set.
  //
  // LOAD-BEARING COUPLING: open `/v1/register` (no admin token sent
  // by the V2 client) relies on this throttle. `OSL_KEYSERVER_ADMIN_TOKEN`
  // MUST stay set as a deployed secret — even though register no
  // longer *checks* it — or open registration becomes unthrottled.
  if (!env.OSL_KEYSERVER_ADMIN_TOKEN) {
    return { ok: true, retryAfter: 0 };
  }
  const now = Math.floor(Date.now() / 1000);
  const windowStart = now - (now % WINDOW_SEC);
  // `bucket` lets one route enforce multiple independent dimensions
  // (e.g. register: per-IP "rl" AND per-user_id "rlreg") without the
  // counters colliding. Existing callers keep the default "rl".
  const key = `${bucket}:${ip}:${windowStart}`;
  const raw = await env.RATE_LIMIT_KV.get(key);
  const current = raw ? Number.parseInt(raw, 10) : 0;
  if (current >= max) {
    const retryAfter = WINDOW_SEC - (now - windowStart);
    return { ok: false, retryAfter: Math.max(retryAfter, 1) };
  }
  // expirationTtl bounded by KV minimum (60s); 120 gives a clean
  // cushion past the window flip.
  await env.RATE_LIMIT_KV.put(key, String(current + 1), {
    expirationTtl: KV_TTL_SEC,
  });
  return { ok: true, retryAfter: 0 };
}

/** Extract the caller IP. Cloudflare provides CF-Connecting-IP. */
export function callerIp(request: Request): string {
  return (
    request.headers.get("cf-connecting-ip") ??
    request.headers.get("x-forwarded-for")?.split(",")[0]?.trim() ??
    "unknown"
  );
}
