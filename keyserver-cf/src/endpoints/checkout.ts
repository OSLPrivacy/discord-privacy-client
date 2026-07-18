/// POST /v1/checkout-session
///
/// Body: {
///   plan?: "pro" | "one_time",
///   claim_token: base64url(32 random bytes),
///   delivery_public_key_spki: RSA-OAEP SHA-256 SPKI
/// }
/// Returns: { url, session_id }
///
/// Public — no admin token. Anyone with a valid plan can start a
/// checkout. Rate-limited so a bot can't spam Stripe sessions.

import type { Env } from "../env.js";
import { validateDeliveryPublicKey } from "../lib/anonymous-crypto.js";
import { createCheckoutSession, isLiveStripeSecretKey } from "../lib/stripe.js";
import {
  insertStripeCheckoutClaim,
  prepareStripeCheckoutClaim,
  STRIPE_CLAIM_LIFETIME_SECONDS,
  validClaimToken,
} from "../lib/stripe-checkout-claims.js";
import { badRequest, json, serverError, serviceUnavailable, tooMany } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";

export async function handleCheckout(
  request: Request,
  env: Env,
  fetcher: typeof fetch = fetch,
): Promise<Response> {
  const rl = await checkRateLimit(env, callerIp(request), 5, "checkout");
  if (!rl.ok) return tooMany(rl.retryAfter);

  if (
    !env.STRIPE_SECRET_KEY ||
    !env.STRIPE_PRICE_ID_PRO ||
    !env.CHECKOUT_SUCCESS_URL ||
    !env.CHECKOUT_CANCEL_URL ||
    !env.LICENSE_HMAC_SECRET
  ) {
    return serviceUnavailable("checkout not configured on this deployment");
  }

  // This deployment is intentionally production-only. A test-mode key must
  // never make a public checkout button appear to accept real payment.
  if (!isLiveStripeSecretKey(env.STRIPE_SECRET_KEY)) {
    return serviceUnavailable("live checkout is not configured on this deployment");
  }

  let body: {
    plan?: unknown;
    claim_token?: unknown;
    delivery_public_key_spki?: unknown;
  };
  try {
    body = (await request.json()) as typeof body;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (body.plan !== undefined && body.plan !== "pro" && body.plan !== "one_time") {
    return badRequest('plan must be "pro"');
  }
  if (!validClaimToken(body.claim_token)) return badRequest("claim_token malformed");
  if (typeof body.delivery_public_key_spki !== "string") {
    return badRequest("delivery_public_key_spki required");
  }
  try {
    await validateDeliveryPublicKey(body.delivery_public_key_spki);
  } catch {
    return badRequest("delivery_public_key_spki must be an RSA-OAEP SHA-256 SPKI key");
  }

  try {
    const prepared = await prepareStripeCheckoutClaim({
      claimToken: body.claim_token,
      deliveryPublicKeySpki: body.delivery_public_key_spki,
      licenseHmacSecret: env.LICENSE_HMAC_SECRET,
    });
    const input: Parameters<typeof createCheckoutSession>[1] = {
      priceId: env.STRIPE_PRICE_ID_PRO,
      successUrl: checkoutSuccessUrl(env.CHECKOUT_SUCCESS_URL),
      cancelUrl: env.CHECKOUT_CANCEL_URL,
      metadata: {
        osl_plan: "pro",
        osl_purchase: "one-time",
        osl_fulfillment: "instant-v1",
      },
    };
    const session = await createCheckoutSession(
      env.STRIPE_SECRET_KEY,
      input,
      fetcher,
    );
    await insertStripeCheckoutClaim(env.DB, {
      sessionId: session.id,
      ...prepared,
      deliveryPublicKeySpki: body.delivery_public_key_spki,
      expiresAt: Math.floor(Date.now() / 1000) + STRIPE_CLAIM_LIFETIME_SECONDS,
    });
    return json({ url: session.url, session_id: session.id });
  } catch {
    console.error("[checkout] creation failed");
    return serverError("checkout creation failed");
  }
}

export function checkoutSuccessUrl(configured: string): string {
  if (configured.includes("{CHECKOUT_SESSION_ID}")) return configured;
  const separator = configured.includes("?") ? "&" : "?";
  return `${configured}${separator}session_id={CHECKOUT_SESSION_ID}`;
}
