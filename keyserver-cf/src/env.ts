/// Environment bindings for the worker. Wrangler injects these on
/// every fetch / scheduled invocation. Native rate-limit bindings are
/// present in both local tests and production. Mutation bearer secrets
/// are optional in the type only so public-only deployments can start;
/// their protected routes fail closed when a token is absent.

export interface Env {
  DB: D1Database;
  RATE_LIMIT_5: RateLimit;
  RATE_LIMIT_10: RateLimit;
  RATE_LIMIT_120: RateLimit;
  RATE_LIMIT_1200: RateLimit;
  RATE_LIMIT_3600: RateLimit;
  /**
   * Operator-only bearer for crypto administration. When unset, the
   * protected route returns 503 rather than opening access.
   */
  OSL_KEYSERVER_ADMIN_TOKEN?: string;
  // REGISTER-FIX: OSL_KEYSERVER_ALLOWED_USERS retired — open signed
  // registration replaced the allowlist (register was its sole
  // consumer). Intentionally absent from the Env surface.
  /** Optional signed selector-manifest envelope JSON. */
  SELECTOR_MANIFEST_JSON?: string;

  // ---- F1.2 (Stripe + licenses + email) ----

  /** Production Stripe restricted or secret key (rk_live_... or sk_live_...). */
  STRIPE_SECRET_KEY?: string;
  /** Stripe webhook signing secret (whsec_...). */
  STRIPE_WEBHOOK_SECRET?: string;
  /** One-time $5 OSL Pro Price ID (price_...). */
  STRIPE_PRICE_ID_PRO?: string;
  /** Legacy recurring Price ID. Not used by public checkout. */
  STRIPE_PRICE_ID_MONTHLY?: string;
  /** Legacy recurring Price ID. Not used by public checkout. */
  STRIPE_PRICE_ID_YEARLY?: string;
  /** Where Stripe Checkout returns the user after success. */
  CHECKOUT_SUCCESS_URL?: string;
  /** Where Stripe Checkout returns the user on cancel. */
  CHECKOUT_CANCEL_URL?: string;
  /** Where fixed-tier donation Checkout returns after success or cancel. */
  DONATION_SUCCESS_URL?: string;
  DONATION_CANCEL_URL?: string;
  /** Where the Stripe Customer Portal returns the user. */
  BILLING_PORTAL_RETURN_URL?: string;

  /** Resend API key (re_...). */
  RESEND_API_KEY?: string;
  /** Verified sender, e.g. "OSL <licenses@oslprivacy.com>". */
  RESEND_FROM?: string;
  /** Support inbox surfaced in the license email body. */
  SUPPORT_EMAIL?: string;

  /** Telegram Bot API token, stored only as a Worker secret. */
  TELEGRAM_BOT_TOKEN?: string;
  /** Telegram webhook secret-token header value. */
  TELEGRAM_WEBHOOK_SECRET?: string;
  /**
   * Comma-separated private chat IDs plus, optionally, one group chat ID.
   * This explicit allowlist supersedes TELEGRAM_ADMIN_CHAT_ID when present.
   */
  TELEGRAM_OPERATOR_CHAT_IDS?: string;
  /**
   * Additional comma-separated private chat IDs that may read aggregate bot
   * reports. Kept separate so adding a coworker cannot replace operators.
   */
  TELEGRAM_VIEWER_CHAT_IDS?: string;
  /** Legacy single-chat setting retained only for deployment migration. */
  TELEGRAM_ADMIN_CHAT_ID?: string;
  /** Public, immutable installer URL used by the tracked redirect. */
  WINDOWS_INSTALLER_URL?: string;

  /** HMAC secret for license-key checksum. Per-deployment static value. */
  LICENSE_HMAC_SECRET?: string;
  /** Explicit non-production issuer. Unset and "production" both reject QA codes. */
  DEPLOYMENT_ENV?: "production" | "qa";
  /** QA-only checksum root. Must never equal LICENSE_HMAC_SECRET. */
  QA_LICENSE_HMAC_SECRET?: string;
  /** Independent second factor for the owner comp-code operator route. */
  OSL_COMP_ADMIN_TOKEN?: string;
  /** Independent HMAC root for purpose/request audit commitments. */
  COMP_AUDIT_HMAC_SECRET?: string;

  // ---- F1.3 (crypto payments) ----

  /** HTTPS base URL of the isolated self-hosted watch-only node watcher. */
  CRYPTO_WATCHER_URL?: string;
  /** HMAC secret used only to authenticate Worker invoice requests to the watcher. */
  CRYPTO_WATCHER_REQUEST_SECRET?: string;
  /** Base64 SPKI-DER Ed25519 public key used only to verify watcher settlements. */
  CRYPTO_WATCHER_SETTLEMENT_PUBLIC_KEY?: string;
  /** Minimum node-verified Bitcoin confirmations (defaults to 2). */
  CRYPTO_BTC_CONFIRMATIONS?: string;
  /** Exact "true" enables new Bitcoin invoices; absent/other values fail closed. */
  CRYPTO_BTC_ENABLED?: string;
  /** Minimum node-verified Monero confirmations (defaults to 10). */
  CRYPTO_XMR_CONFIRMATIONS?: string;
  /** Exact "true" enables new Monero invoices; absent/other values fail closed. */
  CRYPTO_XMR_ENABLED?: string;
  /** Exact one-time lifetime Pro price. Production must be the literal "500". */
  CRYPTO_PRO_USD_CENTS?: string;
}
