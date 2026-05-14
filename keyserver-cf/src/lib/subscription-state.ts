/// Stripe webhook → subscription state machine.
///
/// 6 states (PENDING, ACTIVE, CANCELLED, GRACE, REVOKED, EXPIRED).
/// Transitions driven by these Stripe events:
///
///   checkout.session.completed       → upsert PENDING + issue license
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
import { generateLicenseKey } from "./license.js";
import { sendLicenseEmail } from "./email.js";
import { createBillingPortalSession, type StripeEvent } from "./stripe.js";
import {
  insertLicense,
  revokeLicensesForSubscription,
  type SubscriptionStatus,
  updateSubscriptionStatus,
  upsertSubscription,
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
  current_period_end?: number;
  cancel_at_period_end?: boolean;
}

interface StripeCheckoutSessionObj {
  id: string;
  customer: string;
  customer_details?: { email?: string };
  customer_email?: string;
  subscription?: string;
  mode?: string;
}

interface StripeInvoiceObj {
  id: string;
  subscription?: string;
  customer?: string;
}

interface StripeDisputeObj {
  id: string;
  payment_intent?: string;
  charge?: string;
  /** Stripe surfaces the related sub via `metadata.subscription_id`
   *  when we set it at checkout time. We use that as the dedup key. */
  metadata?: Record<string, string>;
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
  switch (event.type) {
    case "checkout.session.completed":
      return await onCheckoutCompleted(env, obj as StripeCheckoutSessionObj, fetcher);
    case "customer.subscription.created":
    case "customer.subscription.updated":
      return await onSubscriptionWritten(env, obj as StripeSubObj);
    case "customer.subscription.deleted":
      return await onSubscriptionDeleted(env, obj as StripeSubObj);
    case "invoice.payment_failed":
      return await onInvoiceFailed(env, obj as StripeInvoiceObj);
    case "invoice.paid":
      return await onInvoicePaid(env, obj as StripeInvoiceObj);
    case "charge.dispute.created":
      return await onDisputeOpened(env, obj as StripeDisputeObj);
    default:
      return { kind: "noop", reason: `unhandled event type: ${event.type}` };
  }
}

async function onCheckoutCompleted(
  env: Env,
  obj: StripeCheckoutSessionObj,
  fetcher: typeof fetch,
): Promise<HandlerResult> {
  if (obj.mode && obj.mode !== "subscription") {
    return { kind: "noop", reason: `non-subscription mode: ${obj.mode}` };
  }
  const subscriptionId = obj.subscription;
  if (!subscriptionId) {
    return { kind: "noop", reason: "checkout.completed without subscription id" };
  }
  const email = obj.customer_details?.email ?? obj.customer_email;
  if (!email) {
    return { kind: "noop", reason: "checkout.completed without customer email" };
  }

  // Upsert PENDING — the actual ACTIVE transition lands when
  // `customer.subscription.created` arrives (often the same webhook
  // batch). Treating these as separate events keeps the state
  // machine readable.
  await upsertSubscription(env.DB, {
    subscription_id: subscriptionId,
    customer_id: obj.customer,
    customer_email: email,
    status: "PENDING",
    current_period_end: null,
    cancel_at_period_end: 0,
  });

  // Generate + persist + email the license. If email delivery
  // fails, the license still exists in D1 — user can recover via
  // Customer Portal "resend".
  const hmacSecret = env.LICENSE_HMAC_SECRET ?? "osl-license-default-v1";
  const license = await generateLicenseKey(hmacSecret);
  await insertLicense(env.DB, {
    license_hash: license.hash,
    subscription_id: subscriptionId,
  });

  if (env.RESEND_API_KEY && env.RESEND_FROM) {
    let portalUrl: string | undefined;
    if (env.STRIPE_SECRET_KEY && env.BILLING_PORTAL_RETURN_URL) {
      try {
        const portal = await createBillingPortalSession(
          env.STRIPE_SECRET_KEY,
          {
            customerId: obj.customer,
            returnUrl: env.BILLING_PORTAL_RETURN_URL,
          },
          fetcher,
        );
        portalUrl = portal.url;
      } catch (err) {
        console.warn("[checkout] portal session creation failed:", err);
      }
    }
    const email_send: { to: string; licensePlaintext: string; supportEmail: string; from: string; billingPortalUrl?: string } = {
      to: email,
      licensePlaintext: license.plaintext,
      supportEmail: env.SUPPORT_EMAIL ?? "support@oslprivacy.com",
      from: env.RESEND_FROM,
    };
    if (portalUrl) email_send.billingPortalUrl = portalUrl;
    const send = await sendLicenseEmail(
      env.RESEND_API_KEY,
      email_send,
      fetcher,
    );
    if (send.error) {
      console.warn(`[checkout] license email failed: ${send.error}`);
    }
  } else {
    console.warn("[checkout] Resend not configured; license issued but not emailed");
  }

  return {
    kind: "applied",
    summary: `PENDING + license issued for sub=${subscriptionId}`,
  };
}

async function onSubscriptionWritten(env: Env, obj: StripeSubObj): Promise<HandlerResult> {
  const newStatus = deriveStatus(obj.status, !!obj.cancel_at_period_end);
  await updateSubscriptionStatus(env.DB, obj.id, {
    status: newStatus,
    current_period_end: obj.current_period_end ?? null,
    cancel_at_period_end: obj.cancel_at_period_end ? 1 : 0,
  });
  return { kind: "applied", summary: `sub=${obj.id} → ${newStatus}` };
}

async function onSubscriptionDeleted(env: Env, obj: StripeSubObj): Promise<HandlerResult> {
  // Stripe fires this when the subscription ends — either at the
  // close of cancel_at_period_end OR on hard-cancel. Treat as
  // EXPIRED regardless; the cron sweep also covers stragglers.
  await updateSubscriptionStatus(env.DB, obj.id, {
    status: "EXPIRED",
  });
  return { kind: "applied", summary: `sub=${obj.id} → EXPIRED (deleted)` };
}

async function onInvoiceFailed(env: Env, obj: StripeInvoiceObj): Promise<HandlerResult> {
  if (!obj.subscription) {
    return { kind: "noop", reason: "invoice.payment_failed without subscription id" };
  }
  await updateSubscriptionStatus(env.DB, obj.subscription, {
    status: "GRACE",
  });
  return { kind: "applied", summary: `sub=${obj.subscription} → GRACE` };
}

async function onInvoicePaid(env: Env, obj: StripeInvoiceObj): Promise<HandlerResult> {
  if (!obj.subscription) {
    return { kind: "noop", reason: "invoice.paid without subscription id" };
  }
  await updateSubscriptionStatus(env.DB, obj.subscription, {
    status: "ACTIVE",
  });
  return { kind: "applied", summary: `sub=${obj.subscription} → ACTIVE` };
}

async function onDisputeOpened(env: Env, obj: StripeDisputeObj): Promise<HandlerResult> {
  const subscriptionId = obj.metadata?.subscription_id;
  if (!subscriptionId) {
    // Without the subscription_id in metadata we can't safely
    // revoke. Log and noop; manual review picks up from there.
    console.warn(
      `[dispute] charge.dispute.created on charge=${obj.charge ?? "?"} without subscription_id metadata; skipping auto-revoke`,
    );
    return {
      kind: "noop",
      reason: "dispute without subscription_id metadata; manual review required",
    };
  }
  await updateSubscriptionStatus(env.DB, subscriptionId, {
    status: "REVOKED",
  });
  await revokeLicensesForSubscription(env.DB, subscriptionId, "chargeback");
  return { kind: "applied", summary: `sub=${subscriptionId} → REVOKED + license revoked` };
}
