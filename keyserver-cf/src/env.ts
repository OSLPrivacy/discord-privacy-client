/// Environment bindings for the worker. Wrangler injects these on
/// every fetch / scheduled invocation. Defaults match local-dev
/// (no admin token, no allowlist, no rate limit) so `wrangler dev`
/// works without setting secrets.

export interface Env {
  DB: D1Database;
  RATE_LIMIT_KV: KVNamespace;
  /**
   * Pre-shared bearer token for the still-gated mutation routes
   * (wrapped-keys, prekey-bundle/replenish, crypto-admin). Unset =
   * open dev mode. NOTE: also the on/off switch for `checkRateLimit`
   * — `/v1/register` is now open + signed (no token check) but MUST
   * stay rate-limited, so this secret MUST remain set in production
   * even though register no longer reads it.
   */
  OSL_KEYSERVER_ADMIN_TOKEN?: string;
  // REGISTER-FIX: OSL_KEYSERVER_ALLOWED_USERS retired — open signed
  // registration replaced the allowlist (register was its sole
  // consumer). Intentionally absent from the Env surface.
  /** Optional signed selector-manifest envelope JSON. */
  SELECTOR_MANIFEST_JSON?: string;

  // ---- F1.2 (Stripe + licenses + email) ----

  /** Stripe secret key (sk_live_... or sk_test_...). */
  STRIPE_SECRET_KEY?: string;
  /** Stripe webhook signing secret (whsec_...). */
  STRIPE_WEBHOOK_SECRET?: string;
  /** Stripe Price ID for the $5/mo plan (price_...). */
  STRIPE_PRICE_ID_MONTHLY?: string;
  /** Stripe Price ID for the $50/yr plan (price_...). */
  STRIPE_PRICE_ID_YEARLY?: string;
  /** Where Stripe Checkout returns the user after success. */
  CHECKOUT_SUCCESS_URL?: string;
  /** Where Stripe Checkout returns the user on cancel. */
  CHECKOUT_CANCEL_URL?: string;
  /** Where the Stripe Customer Portal returns the user. */
  BILLING_PORTAL_RETURN_URL?: string;

  /** Resend API key (re_...). */
  RESEND_API_KEY?: string;
  /** Verified sender, e.g. "OSL <licenses@oslprivacy.com>". */
  RESEND_FROM?: string;
  /** Support inbox surfaced in the license email body. */
  SUPPORT_EMAIL?: string;

  /** HMAC secret for license-key checksum. Per-deployment static value. */
  LICENSE_HMAC_SECRET?: string;

  // ---- F1.3 (crypto payments) ----

  /** BTC payment address (single static address; rotate via secret update). */
  CRYPTO_BTC_ADDRESS?: string;
  /** XMR payment address (single static integrated address). */
  CRYPTO_XMR_ADDRESS?: string;
  /** USD cents for the monthly plan, e.g. 500 for $5/mo. */
  CRYPTO_MONTHLY_USD_CENTS?: string;
  /** USD cents for the yearly plan, e.g. 5000 for $50/yr. */
  CRYPTO_YEARLY_USD_CENTS?: string;
}
