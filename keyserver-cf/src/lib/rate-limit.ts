/// Per-actor mutation throttling backed by Cloudflare's native Rate
/// Limiting binding. Unlike the former application-level KV
/// read/modify/write counter, this uses Cloudflare's purpose-built,
/// low-latency abuse-control path and does not consume KV writes.
/// Cloudflare documents native counters as permissive and eventually
/// consistent, so they are defense-in-depth rather than authorization.

import type { Env } from "../env.js";

const WINDOW_SEC = 60;

export interface RateLimitDecision {
  ok: boolean;
  /** Native bindings do not expose the exact reset time. */
  retryAfter: number;
}

function limiterFor(env: Env, max: number): RateLimit {
  switch (max) {
    case 5:
      return env.RATE_LIMIT_5;
    case 10:
      return env.RATE_LIMIT_10;
    case 120:
      return env.RATE_LIMIT_120;
    case 1200:
      return env.RATE_LIMIT_1200;
    case 3600:
      return env.RATE_LIMIT_3600;
    default:
      throw new Error(`unsupported native rate-limit threshold: ${max}`);
  }
}

export async function checkRateLimit(
  env: Env,
  actor: string,
  max: number,
  bucket = "rl",
): Promise<RateLimitDecision> {
  try {
    const { success } = await limiterFor(env, max).limit({
      key: `${bucket}:${actor}`,
    });
    return { ok: success, retryAfter: success ? 0 : WINDOW_SEC };
  } catch {
    // A missing/misconfigured security binding is not a reason to admit
    // unbounded mutations. Surface a bounded 429 until configuration is
    // repaired rather than silently disabling abuse protection.
    console.error("[rate-limit] native binding failed closed");
    return { ok: false, retryAfter: WINDOW_SEC };
  }
}

/**
 * Extract the caller IP from Cloudflare's authenticated edge header.
 *
 * Never trust X-Forwarded-For here: callers can supply it themselves. A
 * missing Cloudflare header intentionally collapses into one shared bucket,
 * which fails toward stricter throttling in local/non-edge deployments.
 */
export function callerIp(request: Request): string {
  return request.headers.get("cf-connecting-ip")?.trim() || "unknown";
}
