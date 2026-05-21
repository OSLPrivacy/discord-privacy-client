/// OSL cipher-store — Cloudflare Workers entry.
///
/// Routes:
///   GET    /v1/healthz
///   POST   /v1/blob                    body: ciphertext bytes
///                                      header: X-OSL-TTL-Seconds
///   GET    /v1/blob/:id_hex
///   DELETE /v1/blob/:id_hex
///
/// scheduled() handler: every 5 minutes, sweep expired rows.
///
/// Subpoena-resistance posture:
///   * No identity binding. No user_id, no auth header (Phase 1).
///   * No app-level request logging beyond the aggregate sweep
///     count emitted by scheduled().
///   * Short blob TTLs (24h / 72h / 7d) enforced server-side.
///   * IP-based rate limit lives in KV with short TTL — never
///     persisted to D1.

import type { Env } from "./env.js";
import {
  handleDelete,
  handleFetch,
  handleUpload,
} from "./endpoints/blob.js";
import { handleHealthz } from "./endpoints/healthz.js";
import { clientIp, error, notFound, serverError } from "./lib/http.js";
import { rateLimit } from "./lib/rate-limit.js";
import { sweepExpired } from "./lib/sweep.js";

export default {
  async fetch(
    request: Request,
    env: Env,
    ctx: ExecutionContext
  ): Promise<Response> {
    void ctx;
    try {
      return await dispatch(request, env);
    } catch (err) {
      console.error("[fetch] unhandled error:", err);
      return serverError();
    }
  },

  async scheduled(
    _event: ScheduledEvent,
    env: Env,
    _ctx: ExecutionContext
  ): Promise<void> {
    try {
      const deleted = await sweepExpired(env);
      // Aggregate only -- never per-row.
      console.log(`[sweep] deleted ${deleted} expired blobs`);
    } catch (err) {
      console.error("[sweep] error:", err);
    }
  },
};

async function dispatch(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);
  const path = url.pathname;

  if (path === "/v1/healthz" && request.method === "GET") {
    return handleHealthz();
  }

  if (path === "/v1/blob" && request.method === "POST") {
    const rl = await rateLimit(env, clientIp(request), "upload");
    if (!rl.allowed) {
      return error(429, "rate_limited", "upload rate limit hit");
    }
    return handleUpload(request, env);
  }

  const blobMatch = /^\/v1\/blob\/([0-9a-fA-F]+)$/.exec(path);
  if (blobMatch) {
    const idHex = blobMatch[1]!;
    if (request.method === "GET") {
      const rl = await rateLimit(env, clientIp(request), "fetch");
      if (!rl.allowed) {
        return error(429, "rate_limited", "fetch rate limit hit");
      }
      return handleFetch(request, env, idHex);
    }
    if (request.method === "DELETE") {
      const rl = await rateLimit(env, clientIp(request), "delete");
      if (!rl.allowed) {
        return error(429, "rate_limited", "delete rate limit hit");
      }
      return handleDelete(request, env, idHex);
    }
  }

  return notFound();
}
