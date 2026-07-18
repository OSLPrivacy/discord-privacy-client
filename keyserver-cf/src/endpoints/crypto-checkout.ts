/// Anonymous crypto invoice creation. No email, transaction id, IP address,
/// or service-account identifier is accepted or persisted.

import type { Env } from "../env.js";
import {
  createWatcherInvoice,
  insertAnonymousInvoice,
  newClaimToken,
  usdCentsToAtomic,
  validateDeliveryPublicKey,
} from "../lib/anonymous-crypto.js";
import { getLatestSnapshot } from "../lib/crypto-prices.js";
import { badRequest, json, serviceUnavailable, tooMany } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";

const QUOTE_LIFETIME_SECONDS = 30 * 60;
const INVOICE_RETENTION_SECONDS = 7 * 24 * 60 * 60;
const PRO_LIFETIME_USD_CENTS = 500;

export async function handleCryptoQuote(
  request: Request,
  env: Env,
  fetcher: typeof fetch = fetch,
): Promise<Response> {
  const limit = await checkRateLimit(env, callerIp(request), 5, "crypto-quote-v2");
  if (!limit.ok) return tooMany(limit.retryAfter);

  let body: {
    plan?: unknown;
    payment_method?: unknown;
    delivery_public_key_spki?: unknown;
  };
  try {
    body = (await request.json()) as typeof body;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (body.plan !== "pro") {
    return badRequest('plan must be "pro"');
  }
  const allowedKeys = new Set(["plan", "payment_method", "delivery_public_key_spki"]);
  if (Object.keys(body).some((key) => !allowedKeys.has(key))) {
    return badRequest("unexpected checkout field");
  }
  if (body.payment_method !== "btc" && body.payment_method !== "xmr") {
    return badRequest('payment_method must be "btc" or "xmr"');
  }
  if (typeof body.delivery_public_key_spki !== "string") {
    return badRequest("delivery_public_key_spki required");
  }
  try {
    await validateDeliveryPublicKey(body.delivery_public_key_spki);
  } catch {
    return badRequest("delivery_public_key_spki must be an RSA-OAEP SHA-256 SPKI key");
  }

  const configured = env.CRYPTO_PRO_USD_CENTS;
  const usdCents = Number.parseInt(configured ?? "", 10);
  if (configured !== String(PRO_LIFETIME_USD_CENTS) || usdCents !== PRO_LIFETIME_USD_CENTS) {
    return serviceUnavailable("crypto plan pricing is unavailable");
  }
  const snapshot = await getLatestSnapshot(env.DB, body.payment_method);
  if (!snapshot) return serviceUnavailable("no recent price snapshot");

  let amountNative: string;
  let amountAtomic: string;
  try {
    ({ amountNative, amountAtomic } = usdCentsToAtomic(
      usdCents,
      snapshot.price_usd,
      body.payment_method,
    ));
  } catch {
    return serviceUnavailable("price conversion failed");
  }

  const now = Math.floor(Date.now() / 1000);
  const invoiceId = `cpay_${crypto.randomUUID().replace(/-/g, "")}`;
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
    await insertAnonymousInvoice(env.DB, {
      invoice_id: invoiceId,
      claim_token: claimToken,
      payment_method: body.payment_method,
      plan: body.plan,
      amount_usd_cents: usdCents,
      amount_atomic: amountAtomic,
      delivery_public_key_spki: body.delivery_public_key_spki,
      expires_at: expiresAt,
      cleanup_at: expiresAt + INVOICE_RETENTION_SECONDS,
    });
  } catch {
    console.error("[crypto-invoice] creation failed");
    return serviceUnavailable("crypto invoice service is temporarily unavailable");
  }

  return json({
    invoice_id: invoiceId,
    claim_token: claimToken,
    payment_method: body.payment_method,
    address,
    amount_native: amountNative,
    amount_atomic: amountAtomic,
    amount_usd_cents: usdCents,
    price_locked_at: snapshot.snapshot_date,
    expires_at: expiresAt,
    confirmations_required: confirmationsRequired,
  });
}
