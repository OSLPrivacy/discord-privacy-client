/// POST /v1/checkout-session
///
/// Body: { plan: "monthly" | "yearly", email?: string }
/// Returns: { url, session_id }
///
/// Public — no admin token. Anyone with a valid plan can start a
/// checkout. Rate-limited so a bot can't spam Stripe sessions.

import type { Env } from "../env.js";
import { createCheckoutSession } from "../lib/stripe.js";
import { badRequest, json, serverError, serviceUnavailable, tooMany } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";

export async function handleCheckout(
  request: Request,
  env: Env,
  fetcher: typeof fetch = fetch,
): Promise<Response> {
  const rl = await checkRateLimit(env, callerIp(request), 5);
  if (!rl.ok) return tooMany(rl.retryAfter);

  if (
    !env.STRIPE_SECRET_KEY ||
    !env.STRIPE_PRICE_ID_MONTHLY ||
    !env.STRIPE_PRICE_ID_YEARLY ||
    !env.CHECKOUT_SUCCESS_URL ||
    !env.CHECKOUT_CANCEL_URL
  ) {
    return serviceUnavailable("checkout not configured on this deployment");
  }

  let body: { plan?: unknown; email?: unknown };
  try {
    body = (await request.json()) as typeof body;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (body.plan !== "monthly" && body.plan !== "yearly") {
    return badRequest('plan must be "monthly" or "yearly"');
  }
  if (
    body.email !== undefined &&
    (typeof body.email !== "string" || !/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(body.email))
  ) {
    return badRequest("email malformed");
  }

  const priceId =
    body.plan === "monthly"
      ? env.STRIPE_PRICE_ID_MONTHLY
      : env.STRIPE_PRICE_ID_YEARLY;

  try {
    const input: Parameters<typeof createCheckoutSession>[1] = {
      priceId,
      successUrl: env.CHECKOUT_SUCCESS_URL,
      cancelUrl: env.CHECKOUT_CANCEL_URL,
    };
    if (typeof body.email === "string") input.customerEmail = body.email;
    const session = await createCheckoutSession(
      env.STRIPE_SECRET_KEY,
      input,
      fetcher,
    );
    return json({ url: session.url, session_id: session.id });
  } catch (err) {
    console.error("[checkout] Stripe error:", err);
    return serverError("checkout creation failed");
  }
}
