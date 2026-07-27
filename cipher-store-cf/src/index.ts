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
///   * Rate-limit state is keyed by an opaque HMAC of (bucket, client address)
///     under a server-only secret, and is discarded when its window closes.
///     Read buckets keep it in KV; mutation buckets keep it in the D1
///     `rate_counters` table, because KV's read/modify/write could not enforce
///     a ceiling (2026-07-26 audit). This line previously said "never persisted
///     to D1" — see wrangler.toml for why that changed and why the
///     minimisation property is unchanged.

import type { Env } from "./env.js";
import {
  handleDelete,
  handleFetch,
  handleUpload,
} from "./endpoints/blob.js";
import {
  handleAttachmentComplete,
  handleAttachmentDelete,
  handleAttachmentFetch,
  handleAttachmentPartUpload,
  handleAttachmentSessionCreate,
  handleAttachmentUpload,
} from "./endpoints/attachment.js";
import {
  handleLinkBurn,
  handleLinkCreate,
  handleLinkFetch,
  handleLinkRevoke,
  handleLinkStatus,
} from "./endpoints/link.js";
import { handleHealthz } from "./endpoints/healthz.js";
import { handleLanding, handleRobots } from "./lib/landing.js";
import { clientIp, error, notFound, serverError } from "./lib/http.js";
import { rateLimit, sweepRateCounters } from "./lib/rate-limit.js";
import {
  sweepExpired,
  sweepExpiredAttachments,
  sweepExpiredLinkGrantConsumptions,
  sweepExpiredLinks,
} from "./lib/sweep.js";

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
      console.error("[blob-sweep] failed");
    }
    try {
      await sweepExpiredAttachments(env);
    } catch {
      console.error("[attachment-sweep] failed");
    }
    try {
      await sweepExpiredLinks(env);
    } catch {
      console.error("[link-sweep] failed");
    }
    try {
      await sweepExpiredLinkGrantConsumptions(env);
    } catch {
      console.error("[link-grant-sweep] failed");
    }
    try {
      await sweepRateCounters(env);
    } catch {
      console.error("[rate-counter-sweep] failed");
    }
  },
};

async function dispatch(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);
  const path = url.pathname;

  if (path === "/v1/healthz" && request.method === "GET") {
    return handleHealthz();
  }

  if (path === "/robots.txt" && request.method === "GET") {
    return handleRobots();
  }

  // ---- View-once link lane (Wave A3) -------------------------------
  //
  // Order matters: the /fetch and /burn sub-routes are matched BEFORE
  // the landing route, and the landing route matches ANY id shape so a
  // malformed, expired, burned or never-existent id is indistinguishable
  // from a live one. Always 200, always the same bytes. No oracle.

  const linkFetchMatch = /^\/v\/([^/]{1,128})\/fetch$/.exec(path);
  if (linkFetchMatch && request.method === "POST") {
    const rl = await rateLimit(env, clientIp(request), "link-fetch");
    if (!rl.allowed) return error(429, "rate_limited", "fetch rate limit hit");
    return handleLinkFetch(request, env, linkFetchMatch[1]!);
  }

  const linkBurnMatch = /^\/v\/([^/]{1,128})\/burn$/.exec(path);
  if (linkBurnMatch && request.method === "POST") {
    const rl = await rateLimit(env, clientIp(request), "link-fetch");
    if (!rl.allowed) return error(429, "rate_limited", "fetch rate limit hit");
    return handleLinkBurn(request, env, linkBurnMatch[1]!);
  }

  if (/^\/v\/[^/]{1,128}\/?$/.test(path) && (request.method === "GET" || request.method === "HEAD")) {
    return handleLanding();
  }

  if (path === "/v1/link" && request.method === "POST") {
    const rl = await rateLimit(env, clientIp(request), "link-create");
    if (!rl.allowed) return error(429, "rate_limited", "link rate limit hit");
    return handleLinkCreate(request, env);
  }

  const linkStatusMatch = /^\/v1\/link\/([0-9a-f]{32})\/status$/.exec(path);
  if (linkStatusMatch && request.method === "POST") {
    const rl = await rateLimit(env, clientIp(request), "link-create");
    if (!rl.allowed) return error(429, "rate_limited", "link rate limit hit");
    return handleLinkStatus(request, env, linkStatusMatch[1]!);
  }

  const linkRevokeMatch = /^\/v1\/link\/([0-9a-f]{32})$/.exec(path);
  if (linkRevokeMatch && request.method === "DELETE") {
    const rl = await rateLimit(env, clientIp(request), "link-create");
    if (!rl.allowed) return error(429, "rate_limited", "link rate limit hit");
    return handleLinkRevoke(request, env, linkRevokeMatch[1]!);
  }
  // ------------------------------------------------------------------

  if (path === "/v1/blob" && request.method === "POST") {
    const rl = await rateLimit(env, clientIp(request), "upload");
    if (!rl.allowed) {
      return error(429, "rate_limited", "upload rate limit hit");
    }
    return handleUpload(request, env);
  }

  if (path === "/v1/attachment" && request.method === "POST") {
    const rl = await rateLimit(env, clientIp(request), "attachment-upload");
    if (!rl.allowed) return error(429, "rate_limited", "upload rate limit hit");
    return handleAttachmentUpload(request, env);
  }

  // Session creation has its own small budget: it reserves storage before any
  // ciphertext exists, so it must not share the allowance that parts and
  // completion draw on (audit HIGH-1).
  if (path === "/v1/attachment/session" && request.method === "POST") {
    const rl = await rateLimit(env, clientIp(request), "attachment-session");
    if (!rl.allowed) return error(429, "rate_limited", "upload rate limit hit");
    return handleAttachmentSessionCreate(request, env);
  }

  const attachmentPartMatch = /^\/v1\/attachment\/([0-9a-f]{32})\/part\/(\d+)$/.exec(path);
  if (attachmentPartMatch && request.method === "PUT") {
    const rl = await rateLimit(env, clientIp(request), "attachment-upload");
    if (!rl.allowed) return error(429, "rate_limited", "upload rate limit hit");
    return handleAttachmentPartUpload(
      request,
      env,
      attachmentPartMatch[1]!,
      Number(attachmentPartMatch[2]),
    );
  }

  const attachmentCompleteMatch = /^\/v1\/attachment\/([0-9a-f]{32})\/complete$/.exec(path);
  if (attachmentCompleteMatch && request.method === "POST") {
    const rl = await rateLimit(env, clientIp(request), "attachment-upload");
    if (!rl.allowed) return error(429, "rate_limited", "upload rate limit hit");
    return handleAttachmentComplete(request, env, attachmentCompleteMatch[1]!);
  }

  const attachmentMatch = /^\/v1\/attachment\/([0-9a-f]+)$/.exec(path);
  if (attachmentMatch) {
    const id = attachmentMatch[1]!;
    if (request.method === "GET") {
      const rl = await rateLimit(env, clientIp(request), "attachment-fetch");
      if (!rl.allowed) return error(429, "rate_limited", "fetch rate limit hit");
      return handleAttachmentFetch(request, env, id);
    }
    if (request.method === "DELETE") {
      const rl = await rateLimit(env, clientIp(request), "attachment-delete");
      if (!rl.allowed) return error(429, "rate_limited", "delete rate limit hit");
      return handleAttachmentDelete(request, env, id);
    }
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
