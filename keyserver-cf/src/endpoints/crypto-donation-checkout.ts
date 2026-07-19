/// Anonymous crypto donation invoice creation. The request accepts only an
/// asset and integer USD cents; no donor, entitlement, account, address, or
/// delivery fields are accepted or persisted.

import type { Env } from "../env.js";
import {
  createWatcherInvoice,
  cryptoAssetEnabled,
  newClaimToken,
  usdCentsToAtomic,
} from "../lib/anonymous-crypto.js";
import { insertCryptoDonationInvoice } from "../lib/crypto-donations.js";
import { getLatestSnapshot } from "../lib/crypto-prices.js";
import {
  DONATION_MAX_USD_CENTS,
  DONATION_MIN_USD_CENTS,
  isDonationAmount,
} from "../lib/donations.js";
import { badRequest, json, serviceUnavailable, tooMany } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";

const QUOTE_LIFETIME_SECONDS = 30 * 60;
const INVOICE_RETENTION_SECONDS = 7 * 24 * 60 * 60;

export async function handleCryptoDonationQuote(
  request: Request,
  env: Env,
  fetcher: typeof fetch = fetch,
): Promise<Response> {
  const limit = await checkRateLimit(env, callerIp(request), 5, "crypto-donation-quote-v1");
  if (!limit.ok) return tooMany(limit.retryAfter);

  let parsedBody: unknown;
  try {
    parsedBody = await request.json();
  } catch {
    return badRequest("malformed JSON body");
  }
  if (!parsedBody || typeof parsedBody !== "object" || Array.isArray(parsedBody)) {
    return badRequest("JSON body must be an object");
  }
  const body = parsedBody as {
    payment_method?: unknown;
    amount_usd_cents?: unknown;
  };
  const allowedKeys = new Set(["payment_method", "amount_usd_cents"]);
  if (Object.keys(body).some((key) => !allowedKeys.has(key)) || Object.keys(body).length !== 2) {
    return badRequest("unexpected donation field");
  }
  if (body.payment_method !== "btc" && body.payment_method !== "xmr") {
    return badRequest('payment_method must be "btc" or "xmr"');
  }
  if (!isDonationAmount(body.amount_usd_cents)) {
    return badRequest(
      `amount_usd_cents must be an integer from ${DONATION_MIN_USD_CENTS} through ${DONATION_MAX_USD_CENTS}`,
    );
  }
  const donationEnabled = body.payment_method === "btc"
    ? env.CRYPTO_DONATION_BTC_ENABLED === "true"
    : env.CRYPTO_DONATION_XMR_ENABLED === "true";
  if (!cryptoAssetEnabled(env, body.payment_method) || !donationEnabled) {
    return serviceUnavailable(`${body.payment_method} payments are not enabled`);
  }

  const snapshot = await getLatestSnapshot(env.DB, body.payment_method);
  if (!snapshot) return serviceUnavailable("no recent price snapshot");
  let amountNative: string;
  let amountAtomic: string;
  try {
    ({ amountNative, amountAtomic } = usdCentsToAtomic(
      body.amount_usd_cents,
      snapshot.price_usd,
      body.payment_method,
    ));
  } catch {
    return serviceUnavailable("price conversion failed");
  }

  const now = Math.floor(Date.now() / 1000);
  const invoiceId = `cdon_${crypto.randomUUID().replace(/-/g, "")}`;
  const claimToken = newClaimToken();
  const expiresAt = now + QUOTE_LIFETIME_SECONDS;
  const confirmationsRequired = body.payment_method === "btc"
    ? Number.parseInt(env.CRYPTO_BTC_CONFIRMATIONS ?? "2", 10)
    : Number.parseInt(env.CRYPTO_XMR_CONFIRMATIONS ?? "10", 10);
  if (!Number.isSafeInteger(confirmationsRequired) || confirmationsRequired <= 0) {
    return serviceUnavailable("crypto confirmation policy is unavailable");
  }

  let address: string;
  try {
    ({ address } = await createWatcherInvoice(env, {
      invoice_id: invoiceId,
      payment_method: body.payment_method,
      amount_atomic: amountAtomic,
      expires_at: expiresAt,
    }, fetcher));
    await insertCryptoDonationInvoice(env.DB, {
      invoice_id: invoiceId,
      claim_token: claimToken,
      payment_method: body.payment_method,
      amount_usd_cents: body.amount_usd_cents,
      amount_atomic: amountAtomic,
      confirmations_required: confirmationsRequired,
      price_locked_at: snapshot.fetched_at,
      expires_at: expiresAt,
      cleanup_at: expiresAt + INVOICE_RETENTION_SECONDS,
    });
  } catch (error) {
    console.error("[crypto-donation] invoice creation failed", {
      reason: cryptoDonationFailureReason(error),
    });
    return serviceUnavailable("crypto donation service is temporarily unavailable");
  }

  return json({
    invoice_id: invoiceId,
    claim_token: claimToken,
    payment_method: body.payment_method,
    address,
    amount_native: amountNative,
    amount_atomic: amountAtomic,
    amount_usd_cents: body.amount_usd_cents,
    price_locked_at: snapshot.fetched_at,
    expires_at: expiresAt,
    confirmations_required: confirmationsRequired,
  });
}

function cryptoDonationFailureReason(error: unknown): string {
  if (!(error instanceof Error)) return "unknown";
  if (error.name === "TimeoutError" || error.name === "AbortError") return "watcher_timeout";
  if (error.message === "crypto watcher is not configured") return "watcher_not_configured";
  if (error.message === "crypto watcher URL must use HTTPS") return "watcher_url_invalid";
  if (/^crypto watcher returned [1-5][0-9]{2}$/.test(error.message)) return "watcher_http_error";
  if (error.message.startsWith("crypto watcher returned ")) return "watcher_response_invalid";
  return "internal_failure";
}
