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
///   POST   /v1/stripe/webhook        (HMAC-signed, no admin token)
///   POST   /v1/license/validate
///   POST   /v1/billing-portal-session
///
/// F1.3 (crypto payments):
///   POST   /v1/crypto/quote
///   POST   /v1/crypto/submit
///   POST   /v1/admin/crypto/confirm  (admin token)
///
/// scheduled() handler: hourly EXPIRED sweep + daily crypto-price
/// snapshot. Triggered by `[triggers] crons` in wrangler.toml.

import type { Env } from "./env.js";
import { handleCheckout } from "./endpoints/checkout.js";
import { handleCryptoConfirm } from "./endpoints/crypto-admin.js";
import { handleCryptoQuote } from "./endpoints/crypto-checkout.js";
import { handleCryptoSubmit } from "./endpoints/crypto-submit.js";
import { handleHealthz } from "./endpoints/healthz.js";
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
import { handleUnregister } from "./endpoints/unregister.js";
import { handleUpdateManifest } from "./endpoints/update-manifest.js";
import {
  handleWrappedKeysDelete,
  handleWrappedKeysGet,
  handleWrappedKeysPost,
} from "./endpoints/wrapped-keys.js";
import { corsPreflight, error, notFound, serverError, withCors } from "./lib/http.js";
import { sweepExpired } from "./lib/subscriptions.js";
import { runDailyPriceSnapshot } from "./lib/crypto-prices.js";

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    void ctx;
    try {
      return await dispatch(request, env);
    } catch (err) {
      console.error("[fetch] unhandled error:", err);
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
    // Daily price snapshot fires at 00:00 UTC ("0 0 * * *"). Every
    // other cron tick is the hourly EXPIRED sweep ("0 * * * *").
    if (cron === "0 0 * * *") {
      try {
        await runDailyPriceSnapshot(env);
      } catch (err) {
        console.error("[cron] price snapshot failed:", err);
      }
    }
    try {
      const promoted = await sweepExpired(env.DB);
      if (promoted > 0) {
        console.log(`[cron] EXPIRED sweep promoted ${promoted} subscription(s)`);
      }
    } catch (err) {
      console.error("[cron] sweep failed:", err);
    }
  },
};

async function dispatch(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);
  const path = url.pathname;
  const method = request.method;

  // CORS preflight — only the two browser-callable Stripe
  // endpoints. Everything else falls through to 405.
  if (method === "OPTIONS") {
    if (
      path === "/v1/checkout-session" ||
      path === "/v1/billing-portal-session"
    ) {
      return corsPreflight();
    }
    if (/^\/v1\/update-manifest\//.test(path)) {
      return corsPreflight("GET, OPTIONS");
    }
    return error(405, `method not allowed: ${method}`);
  }

  if (method === "GET") {
    if (path === "/v1/healthz") return handleHealthz();
    if (path === "/v1/selector-manifest") return handleSelectorManifest(env);
    const pubkeysUserId = matchParam(path, /^\/v1\/pubkeys\/([^/]+)$/);
    if (pubkeysUserId !== null) return await handlePubkeys(env, pubkeysUserId);
    const wrappedContentId = matchParam(path, /^\/v1\/wrapped-keys\/([^/]+)$/);
    if (wrappedContentId !== null) {
      return await handleWrappedKeysGet(env, wrappedContentId);
    }
    const bundleUserId = matchParam(path, /^\/v1\/prekey-bundle\/([^/]+)$/);
    if (bundleUserId !== null) return await handlePrekeyBundleGet(env, bundleUserId);
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
    if (path === "/v1/wrapped-keys") return await handleWrappedKeysPost(request, env);
    if (path === "/v1/prekey-bundle/replenish") {
      return await handlePrekeyBundleReplenish(request, env);
    }
    if (path === "/v1/checkout-session") {
      return withCors(await handleCheckout(request, env));
    }
    if (path === "/v1/stripe/webhook") return await handleStripeWebhook(request, env);
    if (path === "/v1/license/validate") return await handleLicenseValidate(request, env);
    if (path === "/v1/billing-portal-session") {
      return withCors(await handleBillingPortal(request, env));
    }
    if (path === "/v1/crypto/quote") return await handleCryptoQuote(request, env);
    if (path === "/v1/crypto/submit") return await handleCryptoSubmit(request, env);
    if (path === "/v1/admin/crypto/confirm") return await handleCryptoConfirm(request, env);
    return notFound("not found");
  }

  if (method === "DELETE") {
    if (path === "/v1/wrapped-keys") return await handleWrappedKeysDelete(request, env);
    const unregUserId = matchParam(path, /^\/v1\/pubkeys\/([^/]+)$/);
    if (unregUserId) return await handleUnregister(request, env, unregUserId);
    return notFound("not found");
  }

  return error(405, `method not allowed: ${method}`);
}

function matchParam(path: string, re: RegExp): string | null {
  const m = path.match(re);
  if (!m) return null;
  const param = m[1];
  if (!param) return null;
  return decodeURIComponent(param);
}
