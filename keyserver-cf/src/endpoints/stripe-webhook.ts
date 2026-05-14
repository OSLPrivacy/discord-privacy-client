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
import { markEventProcessed } from "../lib/subscriptions.js";
import {
  parseEvent,
  verifyWebhookSignature,
} from "../lib/stripe.js";
import { badRequest, json, serviceUnavailable, unauthorized } from "../lib/http.js";

export async function handleStripeWebhook(
  request: Request,
  env: Env,
  fetcher: typeof fetch = fetch,
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

  const isNew = await markEventProcessed(env.DB, event.id, event.type);
  if (!isNew) {
    // Stripe retry. Acknowledge with 200 so they don't retry again.
    return json({ received: true, deduped: true });
  }

  try {
    const result = await applyEvent(env, event, fetcher);
    return json({ received: true, ...result });
  } catch (err) {
    // Don't tell Stripe we processed it on uncaught error — return
    // 500 so they retry. The dedup row in stripe_events will be
    // re-checked next time; if our partial state mutation is
    // already committed, the second attempt's handlers re-converge
    // (they're idempotent by design — ON CONFLICT upserts).
    console.error("[webhook] handler crashed:", err);
    return new Response(JSON.stringify({ error: "internal error" }), {
      status: 500,
      headers: { "content-type": "application/json; charset=utf-8" },
    });
  }
}
