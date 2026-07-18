/// Stripe webhook → subscription state machine.
///
/// 6 states (PENDING, ACTIVE, CANCELLED, GRACE, REVOKED, EXPIRED).
/// Transitions driven by these Stripe events:
///
///   checkout.session.completed       → lifetime ACTIVE for paid one-time Pro
///                                      or legacy subscription fulfillment
///   customer.subscription.created    → ACTIVE (or whatever the
///                                       initial Stripe status maps to)
///   customer.subscription.updated    → derive new state from
///                                       sub.status + cancel_at_period_end
///   customer.subscription.deleted    → EXPIRED (period over) OR
///                                       CANCELLED-pending-period-end
///   invoice.payment_failed           → GRACE
///   invoice.paid                     → ACTIVE (return from GRACE)
///   charge.dispute.created           → REVOKED + revoke license
///
/// The webhook handler owns dedup (INSERT OR IGNORE on event.id)
/// — by the time these functions run, we know we're processing a
/// novel event.

import type { Env } from "../env.js";
import {
  completeOneTimeStripeCheckoutClaim,
  completeStripeCheckoutClaim,
} from "./stripe-checkout-claims.js";
import {
  applyLatestSubscriptionObservation,
  recordAndApplySubscriptionObservation,
} from "./stripe-subscription-observations.js";
import { type StripeEvent } from "./stripe.js";
import {
  revokeLicensesForSubscription,
  type SubscriptionStatus,
} from "./subscriptions.js";

/** Map a raw Stripe subscription status string + cancel flag to our
 *  6-state model. */
export function deriveStatus(
  stripeStatus: string,
  cancelAtPeriodEnd: boolean,
): SubscriptionStatus {
  // Order matters: cancel_at_period_end overlays on top of active.
  if (cancelAtPeriodEnd && (stripeStatus === "active" || stripeStatus === "trialing")) {
    return "CANCELLED";
  }
  switch (stripeStatus) {
    case "active":
    case "trialing":
      return "ACTIVE";
    case "past_due":
      return "GRACE";
    case "unpaid":
    case "incomplete_expired":
    case "canceled":
      return "EXPIRED";
    case "incomplete":
    default:
      // Bias toward not-yet-paid for unknown / new statuses.
      return "PENDING";
  }
}

interface StripeSubObj {
  id: string;
  customer: string;
  status: string;
  /** Pre-2025-03-31 API: canonical billing period end on the
   *  subscription itself. Newer API versions move it under
   *  `items.data[i].current_period_end`; see `readCurrentPeriodEnd`. */
  current_period_end?: number;
  cancel_at_period_end?: boolean;
  /** 2025-03-31 API: per-item billing periods. Different items on
   *  the same subscription can be billed at different cadences, so
   *  Stripe deprecated the top-level field. Our model assumes a
   *  single item per subscription (the F1.2 plan is one-product);
   *  we read item[0]. */
  items?: {
    data?: Array<{
      current_period_end?: number;
      current_period_start?: number;
    }>;
  };
}

interface StripeCheckoutSessionObj {
  id: string;
  customer?: string | null;
  customer_details?: { email?: string };
  customer_email?: string;
  subscription?: string;
  payment_intent?: string;
  payment_status?: string;
  amount_total?: number;
  currency?: string;
  mode?: string;
  metadata?: Record<string, string>;
}

/** Retryable ordering gap: Stripe completed before our claim row committed. */
export class CheckoutClaimNotReadyError extends Error {}

interface StripeInvoiceObj {
  id: string;
  subscription?: string;
  customer?: string;
  /** F2.0 defence-in-depth: on a subscription invoice, the line's
   *  `period.end` equals the subscription's `current_period_end`.
   *  Reading this on `invoice.paid` means even if the
   *  `customer.subscription.created/updated` event is lost or
   *  delayed, the first paid invoice still stamps the period. */
  lines?: {
    data?: Array<{
      period?: { end?: number; start?: number };
    }>;
  };
}

interface StripeDisputeObj {
  id: string;
  payment_intent?: string;
  charge?: string;
  /** Stripe surfaces the related sub via `metadata.subscription_id`
   *  when we set it at checkout time. We use that as the dedup key. */
  metadata?: Record<string, string>;
}

interface StripeChargeObj {
  id: string;
  payment_intent?: string;
  amount_refunded?: number;
}

/**
 * F2.0: resolve the subscription's `current_period_end` across
 * Stripe API versions.
 *
 * - Stripe API 2025-03-31+ moved `current_period_end` from the
 *   Subscription to its SubscriptionItem (`items.data[i].
 *   current_period_end`). The top-level field is dropped on the
 *   wire under newer API versions.
 * - Pre-2025-03-31 payloads still carry the top-level field and no
 *   `items.data[*].current_period_end`.
 *
 * Read both paths; prefer the new one when present. Returns `null`
 * when neither path has a value (which is itself a signal — the
 * subscription is PENDING or hasn't materialised yet).
 */
export function readCurrentPeriodEnd(obj: {
  current_period_end?: number;
  items?: { data?: Array<{ current_period_end?: number }> };
}): number | null {
  const fromItems = obj.items?.data?.[0]?.current_period_end;
  if (typeof fromItems === "number") return fromItems;
  if (typeof obj.current_period_end === "number") return obj.current_period_end;
  return null;
}

/** F2.0 defence-in-depth: pull `current_period_end` from a paid
 *  invoice's first line's `period.end`. For a subscription invoice
 *  this equals the subscription's `current_period_end`. */
function readPeriodEndFromInvoice(obj: {
  lines?: { data?: Array<{ period?: { end?: number } }> };
}): number | null {
  const v = obj.lines?.data?.[0]?.period?.end;
  return typeof v === "number" ? v : null;
}

export type HandlerResult =
  | { kind: "noop"; reason: string }
  | { kind: "applied"; summary: string };

/** Dispatch a verified, deduped Stripe event. */
export async function applyEvent(
  env: Env,
  event: StripeEvent,
  fetcher: typeof fetch = fetch,
): Promise<HandlerResult> {
  const obj = event.data.object as unknown;
  const eventCreated = event.created ?? Math.floor(Date.now() / 1000);
  switch (event.type) {
    case "checkout.session.completed":
    case "checkout.session.async_payment_succeeded":
      return await onCheckoutCompleted(env, obj as StripeCheckoutSessionObj, fetcher);
    case "customer.subscription.created":
    case "customer.subscription.updated":
      return await onSubscriptionWritten(env, obj as StripeSubObj, eventCreated, event.type);
    case "customer.subscription.deleted":
      return await onSubscriptionDeleted(env, obj as StripeSubObj, eventCreated, event.type);
    case "invoice.payment_failed":
      return await onInvoiceFailed(env, obj as StripeInvoiceObj, eventCreated, event.type);
    case "invoice.paid":
      return await onInvoicePaid(env, obj as StripeInvoiceObj, eventCreated, event.type);
    case "charge.dispute.created":
      return await onDisputeOpened(env, obj as StripeDisputeObj, eventCreated, event.type);
    case "charge.refunded":
      return await onChargeRefunded(env, obj as StripeChargeObj, eventCreated, event.type);
    default:
      return { kind: "noop", reason: `unhandled event type: ${event.type}` };
  }
}

async function onCheckoutCompleted(
  env: Env,
  obj: StripeCheckoutSessionObj,
  fetcher: typeof fetch,
): Promise<HandlerResult> {
  if (obj.mode === "payment") {
    if (
      obj.metadata?.osl_plan !== "pro" ||
      obj.metadata?.osl_purchase !== "one-time" ||
      obj.metadata?.osl_fulfillment !== "instant-v1"
    ) {
      return { kind: "noop", reason: "one-time checkout is not an OSL instant claim" };
    }
    if (obj.payment_status !== "paid") {
      return { kind: "noop", reason: "one-time checkout is not paid" };
    }
    if (!obj.payment_intent) {
      return { kind: "noop", reason: "paid checkout without payment intent" };
    }
    if (obj.amount_total !== 500 || obj.currency !== "usd") {
      return { kind: "noop", reason: "one-time checkout amount does not match $5 USD" };
    }
    const completion = await completeOneTimeStripeCheckoutClaim(env.DB, {
      sessionId: obj.id,
      paymentIntentId: obj.payment_intent,
    });
    if (completion === "missing") {
      console.error("[checkout] verified payment has no delivery claim");
      // Checkout Session creation and D1 claim insertion cannot be one atomic
      // transaction. Returning a non-2xx response makes Stripe retry; the
      // event lease is released by the webhook handler, so the claim row can
      // arrive before the next attempt instead of a paid buyer being stranded.
      throw new CheckoutClaimNotReadyError("checkout delivery claim not committed yet");
    }
    return {
      kind: "applied",
      summary: `lifetime Pro activation ready for payment=${obj.payment_intent}`,
    };
  }

  // Preserve fulfillment for subscription sessions already created before
  // the public endpoint switched to one-time Pro.
  if (obj.mode && obj.mode !== "subscription") {
    return { kind: "noop", reason: `unsupported checkout mode: ${obj.mode}` };
  }
  const subscriptionId = obj.subscription;
  if (!subscriptionId) {
    return { kind: "noop", reason: "checkout.completed without subscription id" };
  }
  const email = obj.customer_details?.email ?? obj.customer_email;
  if (!obj.customer || !email) {
    return { kind: "noop", reason: "legacy subscription checkout lacks customer data" };
  }

  void fetcher;
  const completion = await completeStripeCheckoutClaim(env.DB, {
    sessionId: obj.id,
    subscriptionId,
    customerId: obj.customer,
    customerEmail: email,
  });
  if (completion === "missing") {
    // Never mint an unclaimable plaintext license. A missing claim means the
    // session predates instant fulfillment or was not created by this Worker.
    console.error("[checkout] verified session has no delivery claim");
    return { kind: "noop", reason: "checkout completed without delivery claim" };
  }
  await applyLatestSubscriptionObservation(env.DB, subscriptionId);

  return {
    kind: "applied",
    summary: `PENDING + instant activation ready for sub=${subscriptionId}`,
  };
}

async function onSubscriptionWritten(
  env: Env,
  obj: StripeSubObj,
  eventCreated: number,
  eventType: string,
): Promise<HandlerResult> {
  const newStatus = deriveStatus(obj.status, !!obj.cancel_at_period_end);
  const accepted = await recordAndApplySubscriptionObservation(env.DB, {
    subscriptionId: obj.id,
    customerId: obj.customer,
    status: newStatus,
    currentPeriodEnd: readCurrentPeriodEnd(obj),
    cancelAtPeriodEnd: !!obj.cancel_at_period_end,
    eventCreated,
    eventType,
  });
  if (!accepted) return { kind: "noop", reason: `stale subscription event: ${eventType}` };
  return { kind: "applied", summary: `sub=${obj.id} → ${newStatus}` };
}

async function onSubscriptionDeleted(
  env: Env,
  obj: StripeSubObj,
  eventCreated: number,
  eventType: string,
): Promise<HandlerResult> {
  // Stripe fires this when the subscription ends — either at the
  // close of cancel_at_period_end OR on hard-cancel. Treat as
  // EXPIRED regardless; the cron sweep also covers stragglers.
  const accepted = await recordAndApplySubscriptionObservation(env.DB, {
    subscriptionId: obj.id,
    customerId: obj.customer,
    status: "EXPIRED",
    eventCreated,
    eventType,
  });
  if (!accepted) return { kind: "noop", reason: `stale subscription event: ${eventType}` };
  return { kind: "applied", summary: `sub=${obj.id} → EXPIRED (deleted)` };
}

async function onInvoiceFailed(
  env: Env,
  obj: StripeInvoiceObj,
  eventCreated: number,
  eventType: string,
): Promise<HandlerResult> {
  if (!obj.subscription) {
    return { kind: "noop", reason: "invoice.payment_failed without subscription id" };
  }
  const accepted = await recordAndApplySubscriptionObservation(env.DB, {
    subscriptionId: obj.subscription,
    customerId: obj.customer,
    status: "GRACE",
    eventCreated,
    eventType,
  });
  if (!accepted) return { kind: "noop", reason: `stale subscription event: ${eventType}` };
  return { kind: "applied", summary: `sub=${obj.subscription} → GRACE` };
}

async function onInvoicePaid(
  env: Env,
  obj: StripeInvoiceObj,
  eventCreated: number,
  eventType: string,
): Promise<HandlerResult> {
  if (!obj.subscription) {
    return { kind: "noop", reason: "invoice.paid without subscription id" };
  }
  // F2.0 defence-in-depth: stamp current_period_end from the
  // invoice line if available. This means a lost or out-of-order
  // `customer.subscription.created/updated` event still leaves the
  // row with the correct period end after the first paid invoice.
  // When the invoice doesn't carry the field, leave the existing
  // value alone; the observation layer preserves an existing period.
  const periodEnd = readPeriodEndFromInvoice(obj);
  const accepted = await recordAndApplySubscriptionObservation(env.DB, {
    subscriptionId: obj.subscription,
    customerId: obj.customer,
    status: "ACTIVE",
    currentPeriodEnd: periodEnd,
    eventCreated,
    eventType,
  });
  if (!accepted) return { kind: "noop", reason: `stale subscription event: ${eventType}` };
  return { kind: "applied", summary: `sub=${obj.subscription} → ACTIVE` };
}

async function onDisputeOpened(
  env: Env,
  obj: StripeDisputeObj,
  eventCreated: number,
  eventType: string,
): Promise<HandlerResult> {
  const subscriptionId = obj.metadata?.subscription_id ?? obj.payment_intent;
  if (!subscriptionId) {
    // Legacy subscriptions identify themselves through metadata. One-time
    // purchases use the PaymentIntent id directly.
    console.warn("[dispute] missing entitlement reference; skipped auto-revoke");
    return {
      kind: "noop",
      reason: "dispute without subscription_id metadata; manual review required",
    };
  }
  const accepted = await recordAndApplySubscriptionObservation(env.DB, {
    subscriptionId,
    status: "REVOKED",
    eventCreated,
    eventType,
  });
  if (!accepted) return { kind: "noop", reason: `stale subscription event: ${eventType}` };
  await revokeLicensesForSubscription(env.DB, subscriptionId, "chargeback");
  return { kind: "applied", summary: `sub=${subscriptionId} → REVOKED + license revoked` };
}

async function onChargeRefunded(
  env: Env,
  obj: StripeChargeObj,
  eventCreated: number,
  eventType: string,
): Promise<HandlerResult> {
  if (!obj.payment_intent) {
    return { kind: "noop", reason: "refund without payment intent" };
  }
  if (!Number.isSafeInteger(obj.amount_refunded) || (obj.amount_refunded ?? 0) <= 0) {
    return { kind: "noop", reason: "refund without a positive refunded amount" };
  }
  // One-time entitlements are keyed by PaymentIntent. Any verified refund,
  // including a partial refund, fails closed until an operator explicitly
  // resolves it. The legacy schema's `manual` reason is the closest accurate
  // non-fraud category and avoids a destructive migration solely for a label.
  const accepted = await recordAndApplySubscriptionObservation(env.DB, {
    subscriptionId: obj.payment_intent,
    status: "REVOKED",
    eventCreated,
    eventType,
  });
  if (!accepted) return { kind: "noop", reason: `stale refund event: ${eventType}` };
  await revokeLicensesForSubscription(env.DB, obj.payment_intent, "manual");
  return {
    kind: "applied",
    summary: `payment=${obj.payment_intent} → REVOKED after refund`,
  };
}
