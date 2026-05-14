/// POST /v1/billing-portal-session
///
/// Body: { license_key: string }
/// Returns: { url }
///
/// Looks up the license → subscription → customer_id, then mints
/// a Stripe Customer Portal session for that customer. The portal
/// lets users update payment method, cancel, view invoices, etc.
///
/// We gate this with the license key itself rather than an admin
/// token — the license IS the user's authenticator. Knowing a
/// valid (non-revoked) license proves ownership.

import type { Env } from "../env.js";
import { hashLicense, normalizeLicense } from "../lib/license.js";
import { createBillingPortalSession } from "../lib/stripe.js";
import { getLicenseByHash, getSubscription } from "../lib/subscriptions.js";
import {
  badRequest,
  forbidden,
  json,
  serverError,
  serviceUnavailable,
  tooMany,
} from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";

export async function handleBillingPortal(
  request: Request,
  env: Env,
  fetcher: typeof fetch = fetch,
): Promise<Response> {
  const rl = await checkRateLimit(env, callerIp(request), 10);
  if (!rl.ok) return tooMany(rl.retryAfter);

  if (!env.STRIPE_SECRET_KEY || !env.BILLING_PORTAL_RETURN_URL) {
    return serviceUnavailable("billing portal not configured");
  }

  let body: { license_key?: unknown };
  try {
    body = (await request.json()) as typeof body;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (typeof body.license_key !== "string" || body.license_key.length === 0) {
    return badRequest("license_key required");
  }

  const normalized = normalizeLicense(body.license_key);
  if (!normalized) return forbidden("license invalid");
  const hash = await hashLicense(normalized);
  const license = await getLicenseByHash(env.DB, hash);
  if (!license || license.revoked_at !== null) {
    return forbidden("license invalid or revoked");
  }
  const sub = await getSubscription(env.DB, license.subscription_id);
  if (!sub) return forbidden("license has no active subscription");

  try {
    const session = await createBillingPortalSession(
      env.STRIPE_SECRET_KEY,
      { customerId: sub.customer_id, returnUrl: env.BILLING_PORTAL_RETURN_URL },
      fetcher,
    );
    return json({ url: session.url });
  } catch (err) {
    console.error("[portal] Stripe error:", err);
    return serverError("portal session creation failed");
  }
}
