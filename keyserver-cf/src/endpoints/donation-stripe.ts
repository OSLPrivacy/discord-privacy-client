/// Privacy-minimal one-time Stripe donation checkout. The browser supplies an
/// integer-cent amount within the server-owned bounds and a random request
/// capability used to make session creation idempotent. OSL stores no donor
/// profile or entitlement.

import type { Env } from "../env.js";
import {
  donationIdempotencyKey,
  DONATION_MAX_USD_CENTS,
  DONATION_MIN_USD_CENTS,
  donationMetadata,
  isDonationAmount,
  validDonationRequestToken,
  type DonationAmountUsdCents,
} from "../lib/donations.js";
import { badRequest, json, serverError, serviceUnavailable, tooMany } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import { createCheckoutSession, isLiveStripeSecretKey } from "../lib/stripe.js";

export async function handleStripeDonationSession(
  request: Request,
  env: Env,
  fetcher: typeof fetch = fetch,
): Promise<Response> {
  const limit = await checkRateLimit(env, callerIp(request), 5, "stripe-donation");
  if (!limit.ok) return tooMany(limit.retryAfter);

  if (
    !env.STRIPE_SECRET_KEY ||
    !env.DONATION_SUCCESS_URL ||
    !env.DONATION_CANCEL_URL ||
    !isLiveStripeSecretKey(env.STRIPE_SECRET_KEY)
  ) {
    return serviceUnavailable("live donation checkout is not configured on this deployment");
  }

  let body: { amount_usd_cents?: unknown; request_token?: unknown };
  try {
    const parsed = await request.json();
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return badRequest("donation body must be an object");
    }
    body = parsed as typeof body;
  } catch {
    return badRequest("malformed JSON body");
  }
  const allowedKeys = new Set(["amount_usd_cents", "request_token"]);
  if (Object.keys(body).some((key) => !allowedKeys.has(key))) {
    return badRequest("unexpected donation field");
  }
  if (!isDonationAmount(body.amount_usd_cents)) {
    return badRequest(
      `amount_usd_cents must be an integer from ${DONATION_MIN_USD_CENTS} to ${DONATION_MAX_USD_CENTS}`,
    );
  }
  if (!validDonationRequestToken(body.request_token)) {
    return badRequest("request_token malformed");
  }
  const metadata = donationMetadata(body.amount_usd_cents);
  try {
    const session = await createCheckoutSession(env.STRIPE_SECRET_KEY, {
      inlinePrice: {
        currency: "usd",
        unitAmount: body.amount_usd_cents,
        productName: "Support OSL open-source privacy work",
      },
      successUrl: env.DONATION_SUCCESS_URL,
      cancelUrl: env.DONATION_CANCEL_URL,
      metadata,
      paymentIntentMetadata: metadata,
      idempotencyKey: await donationIdempotencyKey(
        body.request_token,
        body.amount_usd_cents,
      ),
    }, fetcher);
    return json({ url: session.url, session_id: session.id });
  } catch {
    console.error("[donation] Stripe Checkout session creation failed");
    return serverError("donation checkout creation failed");
  }
}
