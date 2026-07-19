import type { StripeEvent } from "./stripe.js";

interface MetricObject {
  id?: unknown;
  mode?: unknown;
  payment_status?: unknown;
  amount_total?: unknown;
  amount_paid?: unknown;
  amount_refunded?: unknown;
  amount?: unknown;
  currency?: unknown;
}

export async function recordVerifiedStripeMetric(
  db: D1Database,
  event: StripeEvent,
): Promise<void> {
  const supported = new Set([
    "checkout.session.completed",
    "checkout.session.async_payment_succeeded",
    "invoice.paid",
    "charge.refunded",
    "charge.dispute.created",
  ]);
  if (!supported.has(event.type)) return;
  const object = event.data.object as MetricObject;
  if (typeof object.id !== "string") return;

  let amount = 0;
  if (
    (event.type === "checkout.session.completed" ||
      event.type === "checkout.session.async_payment_succeeded") &&
    object.mode === "payment" &&
    object.payment_status === "paid" &&
    Number.isSafeInteger(object.amount_total)
  ) {
    amount = object.amount_total as number;
  } else if (event.type === "invoice.paid" && Number.isSafeInteger(object.amount_paid)) {
    amount = object.amount_paid as number;
  } else if (event.type === "charge.refunded" && Number.isSafeInteger(object.amount_refunded)) {
    amount = -(object.amount_refunded as number);
  } else if (event.type === "charge.dispute.created" && Number.isSafeInteger(object.amount)) {
    amount = -(object.amount as number);
  }
  const currency = typeof object.currency === "string" && /^[a-z]{3}$/.test(object.currency)
    ? object.currency
    : "usd";
  if (
    (event.type === "checkout.session.completed" ||
      event.type === "checkout.session.async_payment_succeeded") &&
    (object.mode !== "payment" || object.payment_status !== "paid")
  ) {
    return;
  }
  await db.prepare(
    `INSERT INTO commerce_events (
       event_id, event_type, stripe_object_id, amount_cents, currency,
       occurred_at, livemode
     ) VALUES (?, ?, ?, ?, ?, ?, 1)
     ON CONFLICT(event_type, stripe_object_id) DO UPDATE SET
       event_id = excluded.event_id,
       amount_cents = excluded.amount_cents,
       currency = excluded.currency,
       occurred_at = excluded.occurred_at`,
  ).bind(
    event.id,
    event.type,
    object.id,
    amount,
    currency,
    event.created ?? Math.floor(Date.now() / 1000),
  ).run();
}

export interface CommerceSummary {
  successful_payments: number;
  gross_cents: number;
  refunds_and_disputes_cents: number;
  verified_donations: number;
  donation_gross_cents: number;
  active_subscriptions: number;
  download_starts: number;
  download_starts_24h: number;
}

export async function getCommerceSummary(db: D1Database): Promise<CommerceSummary> {
  const now = Math.floor(Date.now() / 1000);
  const [payments, cryptoPayments, losses, donations, subscriptions, downloads] = await Promise.all([
    db.prepare(
      `SELECT COUNT(*) AS count, COALESCE(SUM(amount_cents), 0) AS cents
         FROM commerce_events
        WHERE event_type IN (
          'invoice.paid',
          'checkout.session.completed',
          'checkout.session.async_payment_succeeded'
        )`,
    ).first<{ count: number; cents: number }>(),
    db.prepare(
      `SELECT COUNT(*) AS count, COALESCE(SUM(amount_usd_cents), 0) AS cents
         FROM crypto_commerce_events`,
    ).first<{ count: number; cents: number }>(),
    db.prepare(
      `SELECT COALESCE(SUM(amount_cents), 0) AS cents
         FROM commerce_events
        WHERE event_type IN ('charge.refunded', 'charge.dispute.created')`,
    ).first<{ cents: number }>(),
    db.prepare(
      `SELECT COUNT(*) AS count, COALESCE(SUM(amount_usd_cents), 0) AS cents
         FROM donation_events WHERE provider = 'stripe'`,
    ).first<{ count: number; cents: number }>(),
    db.prepare(
      `SELECT COUNT(*) AS count FROM subscriptions WHERE status = 'ACTIVE'`,
    ).first<{ count: number }>(),
    db.prepare(
      `SELECT COUNT(*) AS count,
              COALESCE(SUM(CASE WHEN created_at >= ? THEN 1 ELSE 0 END), 0) AS recent
         FROM download_events`,
    ).bind(now - 24 * 60 * 60).first<{ count: number; recent: number }>(),
  ]);
  return {
    successful_payments: (payments?.count ?? 0) + (cryptoPayments?.count ?? 0),
    gross_cents: (payments?.cents ?? 0) + (cryptoPayments?.cents ?? 0),
    refunds_and_disputes_cents: losses?.cents ?? 0,
    verified_donations: donations?.count ?? 0,
    donation_gross_cents: donations?.cents ?? 0,
    active_subscriptions: subscriptions?.count ?? 0,
    download_starts: downloads?.count ?? 0,
    download_starts_24h: downloads?.recent ?? 0,
  };
}
