/// POST /v1/crypto/quote
///
/// Body: { plan: "monthly" | "yearly", payment_method: "btc" | "xmr",
///         email: string }
/// Returns: { payment_id, address, amount_native, amount_usd_cents,
///            price_locked_at, expires_at }
///
/// Locks a USD-quoted price at form-display time. The user has 30
/// minutes to send the displayed native amount. After that the
/// quote is considered stale (still recoverable via admin
/// confirm + manual price adjustment).

import type { Env } from "../env.js";
import { getLatestSnapshot, usdCentsToNative } from "../lib/crypto-prices.js";
import { insertCryptoQuote } from "../lib/crypto-payments.js";
import {
  badRequest,
  json,
  serviceUnavailable,
  tooMany,
} from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";

const QUOTE_LIFETIME_SEC = 30 * 60; // 30 min

export async function handleCryptoQuote(
  request: Request,
  env: Env,
): Promise<Response> {
  const rl = await checkRateLimit(env, callerIp(request), 5);
  if (!rl.ok) return tooMany(rl.retryAfter);

  if (!env.CRYPTO_BTC_ADDRESS || !env.CRYPTO_XMR_ADDRESS) {
    return serviceUnavailable("crypto payment addresses not configured");
  }
  if (!env.CRYPTO_MONTHLY_USD_CENTS || !env.CRYPTO_YEARLY_USD_CENTS) {
    return serviceUnavailable("crypto plan pricing not configured");
  }

  let body: {
    plan?: unknown;
    payment_method?: unknown;
    email?: unknown;
  };
  try {
    body = (await request.json()) as typeof body;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (body.plan !== "monthly" && body.plan !== "yearly") {
    return badRequest('plan must be "monthly" or "yearly"');
  }
  if (body.payment_method !== "btc" && body.payment_method !== "xmr") {
    return badRequest('payment_method must be "btc" or "xmr"');
  }
  if (
    typeof body.email !== "string" ||
    !/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(body.email)
  ) {
    return badRequest("email malformed");
  }

  const usdCentsStr =
    body.plan === "monthly"
      ? env.CRYPTO_MONTHLY_USD_CENTS
      : env.CRYPTO_YEARLY_USD_CENTS;
  const usdCents = Number.parseInt(usdCentsStr, 10);
  if (!Number.isFinite(usdCents) || usdCents <= 0) {
    return serviceUnavailable("crypto plan pricing malformed in env");
  }

  const snapshot = await getLatestSnapshot(env.DB, body.payment_method);
  if (!snapshot) {
    return serviceUnavailable(
      "no recent price snapshot — try again after the next 00:00 UTC cron tick",
    );
  }
  let amount_native: string;
  try {
    amount_native = usdCentsToNative(usdCents, snapshot.price_usd, body.payment_method);
  } catch (err) {
    console.error("[crypto-quote] price conversion failed:", err);
    return serviceUnavailable("price conversion failed");
  }

  const address =
    body.payment_method === "btc" ? env.CRYPTO_BTC_ADDRESS : env.CRYPTO_XMR_ADDRESS;
  const payment_id = newPaymentId();
  await insertCryptoQuote(env.DB, {
    payment_id,
    payment_method: body.payment_method,
    plan: body.plan,
    amount_usd_cents: usdCents,
    amount_native,
    address,
    customer_email: body.email,
  });

  const now = Math.floor(Date.now() / 1000);
  return json({
    payment_id,
    address,
    amount_native,
    amount_usd_cents: usdCents,
    price_locked_at: snapshot.snapshot_date,
    expires_at: now + QUOTE_LIFETIME_SEC,
  });
}

function newPaymentId(): string {
  // Workers expose crypto.randomUUID().
  return `cpay_${crypto.randomUUID().replace(/-/g, "")}`;
}
