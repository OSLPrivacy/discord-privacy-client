/// POST /v1/stripe/webhook
///
/// Public, but gated by Stripe's HMAC-SHA256 signature header. The
/// raw request body is signed; parsing JSON before signature
/// verification would lose the byte stream Stripe signed against
/// — so we read the body as text first, verify, THEN parse.
///
/// Idempotency: INSERT OR IGNORE on event.id. Duplicate events
/// (Stripe's smart retries) become no-ops at the storage layer
/// before any state-machine code runs.

import type { Env } from "../env.js";
import { applyEvent } from "../lib/subscription-state.js";
import { recordVerifiedStripeMetric } from "../lib/commerce-metrics.js";
import { recordVerifiedStripeDonation } from "../lib/donations.js";
import {
  claimStripeEvent,
  completeStripeEvent,
  releaseStripeEvent,
} from "../lib/stripe-event-claims.js";
import {
  notifyTelegramForDonation,
  notifyTelegramForStripeEvent,
} from "../lib/telegram.js";
import {
  parseEvent,
  verifyWebhookSignature,
} from "../lib/stripe.js";
import { badRequest, json, serviceUnavailable, unauthorized } from "../lib/http.js";

export async function handleStripeWebhook(
  request: Request,
  env: Env,
  fetcher: typeof fetch = fetch,
  ctx?: ExecutionContext,
): Promise<Response> {
  if (!env.STRIPE_WEBHOOK_SECRET) {
    return serviceUnavailable("webhook not configured on this deployment");
  }

  // CRITICAL: read raw bytes BEFORE any JSON parsing. Stripe signs
  // the literal byte stream; round-tripping through JSON.parse +
  // stringify would change whitespace and break the signature.
  const rawBody = await request.text();
  const sigHeader = request.headers.get("stripe-signature");
  const verified = await verifyWebhookSignature({
    rawBody,
    signatureHeader: sigHeader,
    secret: env.STRIPE_WEBHOOK_SECRET,
  });
  if (!verified) {
    return unauthorized("invalid stripe signature");
  }
  const event = parseEvent(rawBody);
  if (!event) return badRequest("malformed event envelope");
  if (!event.livemode) {
    return badRequest("test-mode Stripe events are disabled on this deployment");
  }

  const claim = await claimStripeEvent(env.DB, event.id, event.type);
  if (claim === "completed") return json({ received: true, deduped: true });
  if (claim === "busy") {
    return new Response(JSON.stringify({ error: "event is already processing" }), {
      status: 503,
      headers: {
        "content-type": "application/json; charset=utf-8",
        "retry-after": "2",
      },
    });
  }

  try {
    const donation = await recordVerifiedStripeDonation(env.DB, event);
    const result = await applyEvent(env, event, fetcher);
    // Donation totals live in their own privacy-minimal ledger. Keeping them
    // out of the purchase ledger avoids counting the same money twice in the
    // operator report, including malformed donation-shaped events.
    if (donation.kind === "not_donation") {
      await recordVerifiedStripeMetric(env.DB, event);
    }
    await completeStripeEvent(env.DB, event.id, event.type);
    if (ctx) {
      if (donation.kind === "inserted") {
        ctx.waitUntil(notifyTelegramForDonation(env, donation.donation, fetcher).catch(() => {
          console.error("[telegram] donation notification failed");
        }));
      } else if (donation.kind === "not_donation") {
        ctx.waitUntil(notifyTelegramForStripeEvent(env, event, fetcher).catch(() => {
          console.error("[telegram] payment notification failed");
        }));
      }
    }
    return json({ received: true, ...result });
  } catch {
    // Don't tell Stripe we processed it on uncaught error — return
    // 500 so they retry. No processed marker exists yet; idempotent
    // handlers and stable checkout claims converge on the same state.
    console.error("[webhook] handler failed");
    await releaseStripeEvent(env.DB, event.id).catch(() => {
      console.error("[webhook] failed to release event claim");
    });
    return new Response(JSON.stringify({ error: "internal error" }), {
      status: 500,
      headers: { "content-type": "application/json; charset=utf-8" },
    });
  }
}
