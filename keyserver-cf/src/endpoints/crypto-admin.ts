/// POST /v1/admin/crypto/confirm
///
/// Body: { payment_id: string, period_end_unix?: number }
/// Returns: { ok: true, license_issued: true, email_sent: boolean }
///
/// Admin-only. Marks a `crypto_pending_payments` row as `confirmed`
/// and:
///   - Creates a synthetic `subscriptions` row with status=ACTIVE
///     (no Stripe customer_id; the customer_id is synthesized as
///     "crypto:<payment_id>" so the licenses FK works).
///   - Generates a license key, stores hashed, emails plaintext.
///
/// `period_end_unix` overrides the default period end (now + 30d
/// for monthly, now + 365d for yearly). Useful for back-dating a
/// late payment or extending a grace period.

import type { Env } from "../env.js";
import { checkAdminToken } from "../lib/auth.js";
import {
  confirmCryptoPayment,
  getCryptoPayment,
} from "../lib/crypto-payments.js";
import { sendLicenseEmail } from "../lib/email.js";
import { generateLicenseKey } from "../lib/license.js";
import {
  insertLicense,
  upsertSubscription,
} from "../lib/subscriptions.js";
import {
  badRequest,
  conflict,
  json,
  notFound,
  serverError,
} from "../lib/http.js";

const MONTHLY_PERIOD = 30 * 24 * 60 * 60;
const YEARLY_PERIOD = 365 * 24 * 60 * 60;

export async function handleCryptoConfirm(
  request: Request,
  env: Env,
  fetcher: typeof fetch = fetch,
): Promise<Response> {
  const authErr = await checkAdminToken(request, env);
  if (authErr) return authErr;

  let body: { payment_id?: unknown; period_end_unix?: unknown };
  try {
    body = (await request.json()) as typeof body;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (typeof body.payment_id !== "string" || body.payment_id.length === 0) {
    return badRequest("payment_id required");
  }
  if (
    body.period_end_unix !== undefined &&
    (typeof body.period_end_unix !== "number" || body.period_end_unix < 0)
  ) {
    return badRequest("period_end_unix must be a non-negative unix timestamp");
  }

  const payment = await getCryptoPayment(env.DB, body.payment_id);
  if (!payment) return notFound("unknown payment_id");
  if (payment.status === "confirmed" || payment.status === "manually_resolved") {
    return conflict("payment already resolved");
  }

  const ok = await confirmCryptoPayment(env.DB, body.payment_id);
  if (!ok) {
    return conflict(`payment in non-confirmable state: ${payment.status}`);
  }

  const now = Math.floor(Date.now() / 1000);
  const periodEnd =
    typeof body.period_end_unix === "number"
      ? body.period_end_unix
      : now + (payment.plan === "yearly" ? YEARLY_PERIOD : MONTHLY_PERIOD);

  // Synthesize subscription + license.
  const synthSubId = `crypto_${payment.payment_id}`;
  const synthCustomerId = `crypto:${payment.payment_id}`;
  try {
    await upsertSubscription(env.DB, {
      subscription_id: synthSubId,
      customer_id: synthCustomerId,
      customer_email: payment.customer_email,
      status: "ACTIVE",
      current_period_end: periodEnd,
      cancel_at_period_end: 0,
    });
    const hmacSecret = env.LICENSE_HMAC_SECRET ?? "osl-license-default-v1";
    const license = await generateLicenseKey(hmacSecret);
    await insertLicense(env.DB, {
      license_hash: license.hash,
      subscription_id: synthSubId,
    });

    let emailSent = false;
    if (env.RESEND_API_KEY && env.RESEND_FROM) {
      const send = await sendLicenseEmail(
        env.RESEND_API_KEY,
        {
          to: payment.customer_email,
          licensePlaintext: license.plaintext,
          supportEmail: env.SUPPORT_EMAIL ?? "support@oslprivacy.com",
          from: env.RESEND_FROM,
        },
        fetcher,
      );
      emailSent = !send.error;
      if (send.error) {
        console.warn(`[crypto-confirm] email failed: ${send.error}`);
      }
    }
    return json({
      ok: true,
      license_issued: true,
      email_sent: emailSent,
      subscription_id: synthSubId,
    });
  } catch (err) {
    console.error("[crypto-confirm] issuance failed:", err);
    return serverError("license issuance failed");
  }
}
