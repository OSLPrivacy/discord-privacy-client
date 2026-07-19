/// OSL keyserver — Cloudflare Workers entry point.
///
/// F1.1 baseline routes:
///   GET    /v1/healthz
///   POST   /v1/register
///   GET    /v1/pubkeys/:user_id
///   POST   /v1/wrapped-keys
///   GET    /v1/wrapped-keys/:content_id
///   DELETE /v1/wrapped-keys
///   GET    /v1/prekey-bundle/:user_id
///   POST   /v1/prekey-bundle/replenish
///   GET    /v1/selector-manifest
///
/// F1.2 (Stripe + licenses):
///   POST   /v1/checkout-session
///   POST   /v1/donations/stripe/session
///   POST   /v1/stripe/webhook        (HMAC-signed, no admin token)
///   POST   /v1/license/validate
///   POST   /v1/billing-portal-session
///
/// Anonymous, node-verified crypto payments:
///   POST   /v1/crypto/quote
///   POST   /v1/crypto/status
///   POST   /v1/internal/crypto/settle (watcher HMAC)
///
/// scheduled() handler: five-minute crypto-price refresh, hourly expiry
/// sweeps, and a daily Telegram report. Triggered by `[triggers] crons`.

import type { Env } from "./env.js";
import { handleCheckout } from "./endpoints/checkout.js";
import { handleStripeDonationSession } from "./endpoints/donation-stripe.js";
import { handleCheckoutClaim } from "./endpoints/checkout-claim.js";
import { handleCompBatchIssue, handleCompBatchRevoke } from "./endpoints/comp-batches.js";
import { handleCryptoQuote } from "./endpoints/crypto-checkout.js";
import { handleCryptoSettlement, sweepAnonymousCryptoInvoices } from "./endpoints/crypto-settlement.js";
import { handleCryptoStatus } from "./endpoints/crypto-status.js";
import { handleHealthz } from "./endpoints/healthz.js";
import { handleWindowsDownload } from "./endpoints/download.js";
import { handleLicenseValidate } from "./endpoints/license.js";
import { handleBillingPortal } from "./endpoints/portal.js";
import {
  handlePrekeyBundleGet,
  handlePrekeyBundleReplenish,
} from "./endpoints/prekey-bundle.js";
import { handlePubkeys } from "./endpoints/pubkeys.js";
import { handleRegister } from "./endpoints/register.js";
import { handleSelectorManifest } from "./endpoints/selector-manifest.js";
import { handleStripeWebhook } from "./endpoints/stripe-webhook.js";
import { handleTelegramWebhook } from "./endpoints/telegram.js";
import { handleUnregister } from "./endpoints/unregister.js";
import {
  handleControlInboxDelete,
  handleControlInboxGet,
  handleControlInboxPost,
} from "./endpoints/control-inbox.js";
import { handleUpdateManifest } from "./endpoints/update-manifest.js";
import {
  handleWrappedKeysDelete,
  handleWrappedKeysGet,
  handleWrappedKeysPost,
} from "./endpoints/wrapped-keys.js";
import { corsPreflight, error, notFound, serverError, tooMany, withCors } from "./lib/http.js";
import { callerIp, checkRateLimit } from "./lib/rate-limit.js";
import { sweepExpired } from "./lib/subscriptions.js";
import { refreshPriceSnapshots } from "./lib/crypto-prices.js";
import { sweepStripeCheckoutClaims } from "./lib/stripe-checkout-claims.js";
import { sendTelegramOperatorMessage, telegramStatsMessage } from "./lib/telegram.js";
import { sweepExpiredPrivacyRows } from "./lib/db.js";

const MAX_MUTATION_BODY_BYTES = 1024 * 1024;

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    void ctx;
    try {
      return await dispatch(request, env, ctx);
    } catch {
      console.error("[fetch] unhandled failure");
      return serverError("internal error");
    }
  },

  async scheduled(
    controller: ScheduledController,
    env: Env,
    ctx: ExecutionContext,
  ): Promise<void> {
    void ctx;
    const cron = controller.cron;
    // Keep price refresh isolated from the slower housekeeping/report jobs.
    if (cron === "*/5 * * * *") {
      try {
        await refreshPriceSnapshots(env);
      } catch {
        console.error("[cron] price snapshot failed");
      }
      return;
    }
    // The commerce report is intentionally sent only by the midnight cron.
    if (cron === "0 0 * * *") {
      if (
        env.TELEGRAM_BOT_TOKEN &&
        (env.TELEGRAM_OPERATOR_CHAT_IDS !== undefined || env.TELEGRAM_ADMIN_CHAT_ID !== undefined)
      ) {
        try {
          const report = await telegramStatsMessage(env, true);
          await sendTelegramOperatorMessage(env, report);
        } catch {
          console.error("[cron] Telegram commerce report failed");
        }
      }
      return;
    }
    try {
      const promoted = await sweepExpired(env.DB);
      if (promoted > 0) {
        console.log(`[cron] EXPIRED sweep promoted ${promoted} subscription(s)`);
      }
    } catch {
      console.error("[cron] sweep failed");
    }
    try {
      const changed = await sweepAnonymousCryptoInvoices(env.DB);
      if (changed > 0) {
        console.log(`[cron] anonymous crypto sweep changed ${changed} row(s)`);
      }
    } catch {
      console.error("[cron] anonymous crypto sweep failed");
    }
    try {
      const changed = await sweepStripeCheckoutClaims(env.DB);
      if (changed > 0) {
        console.log(`[cron] Stripe checkout claim sweep expired ${changed} row(s)`);
      }
    } catch {
      console.error("[cron] Stripe checkout claim sweep failed");
    }
    try {
      const deleted = await sweepExpiredPrivacyRows(env.DB);
      const total =
        deleted.wrappedKeys +
        deleted.consumingGetReceipts +
        deleted.wrappedKeyPostReceipts +
        deleted.prekeyReplenishReceipts +
        deleted.wrappedKeyBurnReceipts +
        deleted.unregisterReceipts;
      if (total > 0) {
        console.log(`[cron] privacy retention sweep deleted ${total} expired row(s)`);
      }
    } catch {
      console.error("[cron] privacy retention sweep failed");
    }
    // Phase 6.4: TTL-sweep expired control_inbox rows. Hourly is
    // fine -- rows expire at 7d so a 1h slack is well within
    // tolerance, and clients drain their inbox far more frequently
    // than that anyway.
    try {
      const now = Math.floor(Date.now() / 1000);
      const r = await env.DB.prepare(
        "DELETE FROM control_inbox WHERE expires_at < ?",
      )
        .bind(now)
        .run();
      const meta = r as unknown as { meta?: { changes?: number } };
      const changes = meta.meta?.changes ?? 0;
      if (changes > 0) {
        console.log(`[cron] control_inbox sweep deleted ${changes} expired row(s)`);
      }
      await env.DB.prepare(
        "DELETE FROM control_inbox_requests WHERE expires_at < ?",
      )
        .bind(now)
        .run();
    } catch {
      console.error("[cron] control_inbox sweep failed");
    }
  },
};

async function dispatch(
  request: Request,
  env: Env,
  ctx: ExecutionContext,
): Promise<Response> {
  if (["POST", "PUT", "PATCH", "DELETE"].includes(request.method)) {
    // Reject abusive mutation floods before reading their bodies. Endpoint
    // limits below remain stricter; this is the coarse memory/CPU guard.
    const ingress = await checkRateLimit(
      env,
      callerIp(request),
      3600,
      "mutation-ingress",
    );
    if (!ingress.ok) return tooMany(ingress.retryAfter);
    const bounded = await bufferRequestBody(request, MAX_MUTATION_BODY_BYTES);
    if (bounded instanceof Response) return bounded;
    request = bounded;
  }
  const url = new URL(request.url);
  const path = url.pathname;
  const method = request.method;

  // CORS preflight — only explicitly browser-callable commerce endpoints.
  if (method === "OPTIONS") {
    if (
      path === "/v1/checkout-session" ||
      path === "/v1/donations/stripe/session" ||
      path === "/v1/checkout/claim" ||
      path === "/v1/billing-portal-session" ||
      path === "/v1/crypto/quote" ||
      path === "/v1/crypto/status"
    ) {
      return corsPreflight("POST, OPTIONS", request);
    }
    if (/^\/v1\/update-manifest\//.test(path)) {
      return corsPreflight("GET, OPTIONS");
    }
    return error(405, `method not allowed: ${method}`);
  }

  if (method === "GET") {
    if (path === "/v1/healthz") return handleHealthz();
    if (path === "/v1/download/windows") return await handleWindowsDownload(request, env);
    if (path === "/v1/selector-manifest") return handleSelectorManifest(env);
    const pubkeysUserId = matchParam(path, /^\/v1\/pubkeys\/([^/]+)$/);
    if (pubkeysUserId !== null) return await handlePubkeys(env, pubkeysUserId);
    const wrappedContentId = matchParam(path, /^\/v1\/wrapped-keys\/([^/]+)$/);
    if (wrappedContentId !== null) {
      return await handleWrappedKeysGet(request, env, wrappedContentId);
    }
    const bundleUserId = matchParam(path, /^\/v1\/prekey-bundle\/([^/]+)$/);
    if (bundleUserId !== null) {
      return await handlePrekeyBundleGet(request, env, bundleUserId);
    }
    const inboxUserId = matchParam(path, /^\/v1\/control-inbox\/([^/]+)$/);
    if (inboxUserId !== null) return await handleControlInboxGet(request, env, inboxUserId);
    const um = path.match(
      /^\/v1\/update-manifest\/([^/]+)\/([^/]+)\/([^/]+)$/,
    );
    if (um) {
      return withCors(
        await handleUpdateManifest(
          request,
          env,
          decodeURIComponent(um[1]!),
          decodeURIComponent(um[2]!),
          decodeURIComponent(um[3]!),
        ),
      );
    }
    return notFound("not found");
  }

  if (method === "POST") {
    if (path === "/v1/register") return await handleRegister(request, env);
    if (path === "/v1/control-inbox") return await handleControlInboxPost(request, env);
    if (path === "/v1/wrapped-keys") return await handleWrappedKeysPost(request, env);
    if (path === "/v1/prekey-bundle/replenish") {
      return await handlePrekeyBundleReplenish(request, env);
    }
    if (path === "/v1/checkout-session") {
      return withCors(await handleCheckout(request, env), request);
    }
    if (path === "/v1/donations/stripe/session") {
      return withCors(await handleStripeDonationSession(request, env), request);
    }
    if (path === "/v1/checkout/claim") {
      return withCors(await handleCheckoutClaim(request, env), request);
    }
    if (path === "/v1/stripe/webhook") {
      return await handleStripeWebhook(request, env, fetch, ctx);
    }
    if (path === "/v1/telegram/webhook") return await handleTelegramWebhook(request, env);
    if (path === "/v1/license/validate") return await handleLicenseValidate(request, env);
    if (path === "/v1/billing-portal-session") {
      return withCors(await handleBillingPortal(request, env), request);
    }
    if (path === "/v1/crypto/quote") {
      return withCors(await handleCryptoQuote(request, env), request);
    }
    if (path === "/v1/crypto/status") {
      return withCors(await handleCryptoStatus(request, env), request);
    }
    if (path === "/v1/internal/crypto/settle") {
      return await handleCryptoSettlement(request, env, ctx);
    }
    if (path === "/v1/internal/comp/batches") {
      return await handleCompBatchIssue(request, env);
    }
    return notFound("not found");
  }

  if (method === "DELETE") {
    const compBatchId = matchParam(path, /^\/v1\/internal\/comp\/batches\/([^/]+)$/);
    if (compBatchId) return await handleCompBatchRevoke(request, env, compBatchId);
    if (path === "/v1/wrapped-keys") return await handleWrappedKeysDelete(request, env);
    const unregUserId = matchParam(path, /^\/v1\/pubkeys\/([^/]+)$/);
    if (unregUserId) return await handleUnregister(request, env, unregUserId);
    const inboxId = matchParam(path, /^\/v1\/control-inbox\/([^/]+)$/);
    if (inboxId) return await handleControlInboxDelete(request, env, inboxId);
    return notFound("not found");
  }

  return error(405, `method not allowed: ${method}`);
}

/**
 * Buffer at most `maxBytes` before any endpoint calls `request.json()`.
 * Content-Length is only a fast rejection hint; the streaming count is
 * authoritative for chunked or dishonest requests.
 */
async function bufferRequestBody(
  request: Request,
  maxBytes: number,
): Promise<Request | Response> {
  const declared = request.headers.get("content-length");
  if (declared !== null) {
    const parsed = Number.parseInt(declared, 10);
    if (Number.isFinite(parsed) && parsed > maxBytes) {
      return error(413, "request body too large");
    }
  }
  if (!request.body) return request;

  const reader = request.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    if (!value) continue;
    total += value.byteLength;
    if (total > maxBytes) {
      await reader.cancel("request body too large");
      return error(413, "request body too large");
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return new Request(request.url, {
    method: request.method,
    headers: request.headers,
    body: bytes,
  });
}

function matchParam(path: string, re: RegExp): string | null {
  const m = path.match(re);
  if (!m) return null;
  const param = m[1];
  if (!param) return null;
  return decodeURIComponent(param);
}
