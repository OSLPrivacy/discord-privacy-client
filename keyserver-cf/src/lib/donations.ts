import type { StripeEvent } from "./stripe.js";

export const DONATION_AMOUNTS_USD_CENTS = [500, 2000, 5000] as const;
export const DONATION_MIN_USD_CENTS = 100;
export const DONATION_MAX_USD_CENTS = 1_000_000;

declare const donationAmountUsdCentsBrand: unique symbol;
export type DonationAmountUsdCents = number & {
  readonly [donationAmountUsdCentsBrand]: true;
};

export interface VerifiedDonation {
  donationId: string;
  provider: "stripe";
  providerReference: string;
  amountUsdCents: DonationAmountUsdCents;
  currency: "usd";
  occurredAt: number;
}

export type StripeDonationRecordResult =
  | { kind: "not_donation" }
  | { kind: "invalid_donation" }
  | { kind: "inserted"; donation: VerifiedDonation }
  | { kind: "duplicate"; donation: VerifiedDonation };

interface StripeCheckoutObject {
  mode?: unknown;
  payment_status?: unknown;
  payment_intent?: unknown;
  amount_total?: unknown;
  currency?: unknown;
  metadata?: unknown;
}

interface DonationMetadata {
  osl_kind?: unknown;
  osl_purchase?: unknown;
  osl_fulfillment?: unknown;
  osl_donation_amount_cents?: unknown;
}

export function isDonationAmount(value: unknown): value is DonationAmountUsdCents {
  return typeof value === "number" &&
    Number.isSafeInteger(value) &&
    value >= DONATION_MIN_USD_CENTS &&
    value <= DONATION_MAX_USD_CENTS;
}

export function validDonationRequestToken(value: unknown): value is string {
  return typeof value === "string" && /^[A-Za-z0-9_-]{43}$/.test(value);
}

export function donationMetadata(amount: DonationAmountUsdCents): Record<string, string> {
  return {
    osl_kind: "donation",
    osl_purchase: "one-time",
    osl_fulfillment: "none",
    osl_donation_amount_cents: String(amount),
  };
}

export async function donationIdempotencyKey(
  requestToken: string,
  amount: DonationAmountUsdCents,
): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(`osl-donation-session-v1\0${amount}\0${requestToken}`),
  );
  let hex = "";
  for (const byte of new Uint8Array(digest)) hex += byte.toString(16).padStart(2, "0");
  return `osl-donation-v1-${amount}-${hex}`;
}

function parseVerifiedDonation(event: StripeEvent): VerifiedDonation | "invalid" | null {
  if (
    event.type !== "checkout.session.completed" &&
    event.type !== "checkout.session.async_payment_succeeded"
  ) {
    return null;
  }
  const object = event.data.object as StripeCheckoutObject;
  const metadata = object.metadata && typeof object.metadata === "object"
    ? object.metadata as DonationMetadata
    : {};
  if (metadata.osl_kind !== "donation") return null;
  if (
    metadata.osl_purchase !== "one-time" ||
    metadata.osl_fulfillment !== "none" ||
    object.mode !== "payment" ||
    object.payment_status !== "paid" ||
    object.currency !== "usd" ||
    !isDonationAmount(object.amount_total) ||
    metadata.osl_donation_amount_cents !== String(object.amount_total) ||
    typeof object.payment_intent !== "string" ||
    !/^pi_[A-Za-z0-9_]{1,252}$/.test(object.payment_intent)
  ) {
    return "invalid";
  }
  return {
    donationId: `stripe:${object.payment_intent}`,
    provider: "stripe",
    providerReference: object.payment_intent,
    amountUsdCents: object.amount_total,
    currency: "usd",
    occurredAt: event.created ?? Math.floor(Date.now() / 1000),
  };
}

export async function recordVerifiedStripeDonation(
  db: D1Database,
  event: StripeEvent,
): Promise<StripeDonationRecordResult> {
  const donation = parseVerifiedDonation(event);
  if (donation === null) return { kind: "not_donation" };
  if (donation === "invalid") {
    console.warn("[donation] ignored malformed verified Stripe donation event");
    return { kind: "invalid_donation" };
  }
  const inserted = await db.prepare(
    `INSERT OR IGNORE INTO donation_events (
       donation_id, provider, provider_reference, amount_usd_cents,
       currency, occurred_at
     ) VALUES (?, 'stripe', ?, ?, 'usd', ?)`,
  ).bind(
    donation.donationId,
    donation.providerReference,
    donation.amountUsdCents,
    donation.occurredAt,
  ).run();
  if ((inserted.meta?.changes ?? 0) === 1) return { kind: "inserted", donation };

  const existing = await db.prepare(
    `SELECT donation_id, provider_reference, amount_usd_cents, currency, occurred_at
       FROM donation_events WHERE provider = 'stripe' AND provider_reference = ?`,
  ).bind(donation.providerReference).first<{
    donation_id: string;
    provider_reference: string;
    amount_usd_cents: number;
    currency: string;
    occurred_at: number;
  }>();
  if (
    !existing ||
    existing.donation_id !== donation.donationId ||
    existing.amount_usd_cents !== donation.amountUsdCents ||
    existing.currency !== donation.currency
  ) {
    console.error("[donation] PaymentIntent conflicts with an existing donation");
    return { kind: "invalid_donation" };
  }
  return {
    kind: "duplicate",
    donation: {
      ...donation,
      occurredAt: existing.occurred_at,
    },
  };
}

export interface DonationSummary {
  verified: number;
  grossUsdCents: number;
}

export async function getDonationSummary(db: D1Database): Promise<DonationSummary> {
  const row = await db.prepare(
    `SELECT COUNT(*) AS count, COALESCE(SUM(amount_usd_cents), 0) AS cents
       FROM donation_events WHERE provider = 'stripe'`,
  ).first<{ count: number; cents: number }>();
  return {
    verified: row?.count ?? 0,
    grossUsdCents: row?.cents ?? 0,
  };
}
