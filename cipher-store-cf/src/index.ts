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
/// Data-minimisation posture:
///   * No identity binding. No user_id or account credential. A per-upload
///     opaque token gates fetch/delete but does not authenticate a person.
///   * No variable app-level request logging. Fixed failure event names carry
///     no identifiers, URLs, sizes, row counts, or timing details.
///   * Short blob TTLs (1h / 24h / 72h / 7d) enforced server-side.
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
    } catch {
      console.error("[fetch] unhandled failure");
      return serverError();
    }
  },

  async scheduled(
    _event: ScheduledEvent,
    env: Env,
    _ctx: ExecutionContext
  ): Promise<void> {
    try {
      await sweepExpired(env);
    } catch {
      console.error("[sweep] failed");
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
