import { encryptLicenseForDelivery } from "./anonymous-crypto.js";
import { generateLicenseKey } from "./license.js";
import { sha256Hex } from "./crypto-watcher-auth.js";
import {
  applyLatestSubscriptionObservation,
  getLatestSubscriptionObservation,
} from "./stripe-subscription-observations.js";
import { revokeLicensesForSubscription } from "./subscriptions.js";
import type { AcquiredStripeEventClaim } from "./stripe-event-claims.js";
import type { VerifiedStripeWebhook } from "./stripe.js";

export const STRIPE_CLAIM_LIFETIME_SECONDS = 24 * 60 * 60;
export const COMPLETED_STRIPE_CLAIM_LIFETIME_SECONDS = 7 * 24 * 60 * 60;
/** A one-time card purchase is a redeemable month, not a lifetime entitlement. */
export const PREPAID_PRO_GRANT_SECONDS = 30 * 24 * 60 * 60;

export interface StripeCheckoutClaimRow {
  session_id: string;
  claim_hash: string;
  delivery_public_key_spki: string;
  encrypted_license: string;
  license_hash: string;
  subscription_id: string | null;
  status: "pending" | "delivery_ready" | "expired";
  created_at: number;
  expires_at: number;
  delivered_at: number | null;
  acknowledged_at: number | null;
}

export function validClaimToken(value: unknown): value is string {
  return typeof value === "string" && /^[A-Za-z0-9_-]{43}$/.test(value);
}

export async function prepareStripeCheckoutClaim(input: {
  claimToken: string;
  deliveryPublicKeySpki: string;
  licenseHmacSecret: string;
}): Promise<{ claimHash: string; encryptedLicense: string; licenseHash: string }> {
  const license = await generateLicenseKey(input.licenseHmacSecret);
  return {
    claimHash: await sha256Hex(input.claimToken),
    encryptedLicense: await encryptLicenseForDelivery(
      input.deliveryPublicKeySpki,
      license.plaintext,
    ),
    licenseHash: license.hash,
  };
}

export async function insertStripeCheckoutClaim(
  db: D1Database,
  row: {
    sessionId: string;
    claimHash: string;
    deliveryPublicKeySpki: string;
    encryptedLicense: string;
    licenseHash: string;
    expiresAt: number;
  },
): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  await db.prepare(
    `INSERT INTO stripe_checkout_claims (
       session_id, claim_hash, delivery_public_key_spki,
       encrypted_license, license_hash, subscription_id, status,
       created_at, expires_at, delivered_at
     ) VALUES (?, ?, ?, ?, ?, NULL, 'pending', ?, ?, NULL)`,
  ).bind(
    row.sessionId,
    row.claimHash,
    row.deliveryPublicKeySpki,
    row.encryptedLicense,
    row.licenseHash,
    now,
    row.expiresAt,
  ).run();
}

export async function getStripeCheckoutClaim(
  db: D1Database,
  sessionId: string,
): Promise<StripeCheckoutClaimRow | null> {
  return await db.prepare(
    "SELECT * FROM stripe_checkout_claims WHERE session_id = ?",
  ).bind(sessionId).first<StripeCheckoutClaimRow>();
}

export async function stripeClaimMatches(
  row: StripeCheckoutClaimRow,
  claimToken: string,
): Promise<boolean> {
  const actual = await sha256Hex(claimToken);
  const encoder = new TextEncoder();
  const [actualDigest, storedDigest] = await Promise.all([
    crypto.subtle.digest("SHA-256", encoder.encode(actual)),
    crypto.subtle.digest("SHA-256", encoder.encode(row.claim_hash)),
  ]);
  return crypto.subtle.timingSafeEqual(actualDigest, storedDigest);
}

/**
 * Activate the pre-generated license after Stripe's signed completion event.
 * Every statement uses the same pre-generated hash, making retries converge on
 * one subscription, one license, and one browser claim.
 */
export async function completeStripeCheckoutClaim(
  db: D1Database,
  input: {
    sessionId: string;
    subscriptionId: string;
    customerId: string;
    customerEmail: string;
  },
): Promise<"completed" | "already_completed" | "missing"> {
  const claim = await getStripeCheckoutClaim(db, input.sessionId);
  if (!claim) return "missing";
  if (claim.status === "delivery_ready" && claim.subscription_id === input.subscriptionId) {
    return "already_completed";
  }
  if (claim.status !== "pending") return "missing";

  const now = Math.floor(Date.now() / 1000);
  await db.batch([
    db.prepare(
      `INSERT INTO subscriptions (
         subscription_id, customer_id, customer_email, status,
         current_period_end, cancel_at_period_end, created_at, updated_at,
         is_comp
       ) VALUES (?, ?, ?, 'PENDING', NULL, 0, ?, ?, 0)
       ON CONFLICT(subscription_id) DO UPDATE SET
         customer_id = excluded.customer_id,
         customer_email = excluded.customer_email,
         updated_at = excluded.updated_at`,
    ).bind(
      input.subscriptionId,
      input.customerId,
      // Checkout verification may include an email, but entitlement delivery
      // does not require OSL to retain another copy of it.
      "",
      now,
      now,
    ),
    db.prepare(
      `INSERT OR IGNORE INTO licenses (
         license_hash, subscription_id, issued_at, revoked_at, revoked_reason
       ) VALUES (?, ?, ?, NULL, NULL)`,
    ).bind(claim.license_hash, input.subscriptionId, now),
    db.prepare(
      `UPDATE stripe_checkout_claims
          SET subscription_id = ?, status = 'delivery_ready', expires_at = ?
        WHERE session_id = ? AND status = 'pending'`,
    ).bind(
      input.subscriptionId,
      now + COMPLETED_STRIPE_CLAIM_LIFETIME_SECONDS,
      input.sessionId,
    ),
  ]);
  return "completed";
}

const oneTimePaidCodeCallbackChecksBrand: unique symbol = Symbol("oneTimePaidCodeCallbackChecks");

interface TerminalPaymentObservation {
  status: "REVOKED" | "EXPIRED";
  eventType: string;
}

export interface OneTimePaidCodeCallbackChecks {
  readonly [oneTimePaidCodeCallbackChecksBrand]: true;
  readonly sessionId: string;
  readonly paymentIntentId: string;
  readonly signatureCheck: "verified_stripe_webhook" | "verified_stripe_commerce_record";
  readonly amountCheck: "paid_500_usd";
  readonly repeatCheck: "stripe_event_claim_acquired" | "repair_license_missing";
  readonly refundCheck: "latest_terminal_observation_checked";
  readonly terminalObservation: TerminalPaymentObservation | null;
}

export type OneTimePaidCodeCallbackCheckResult =
  | { ok: true; checks: OneTimePaidCodeCallbackChecks }
  | { ok: false; reason: string };

export async function verifyOneTimePaidCodeCallbackChecks(
  db: D1Database,
  input: {
    webhook: VerifiedStripeWebhook;
    eventClaim: AcquiredStripeEventClaim;
    sessionId: unknown;
    paymentIntentId: unknown;
    paymentStatus: unknown;
    amountTotal: unknown;
    currency: unknown;
  },
): Promise<OneTimePaidCodeCallbackCheckResult> {
  void input.webhook;
  void input.eventClaim;
  if (input.paymentStatus !== "paid") {
    return { ok: false, reason: "one-time checkout is not paid" };
  }
  if (typeof input.sessionId !== "string" || input.sessionId.length === 0) {
    return { ok: false, reason: "paid checkout without session id" };
  }
  if (typeof input.paymentIntentId !== "string" || input.paymentIntentId.length === 0) {
    return { ok: false, reason: "paid checkout without payment intent" };
  }
  if (input.amountTotal !== 500 || input.currency !== "usd") {
    return { ok: false, reason: "one-time checkout amount does not match $5 USD" };
  }
  return {
    ok: true,
    checks: oneTimePaidCodeCallbackChecks({
      amountCheck: "paid_500_usd",
      paymentIntentId: input.paymentIntentId,
      refundCheck: "latest_terminal_observation_checked",
      repeatCheck: "stripe_event_claim_acquired",
      sessionId: input.sessionId,
      signatureCheck: "verified_stripe_webhook",
      terminalObservation: await terminalPaymentObservation(db, input.paymentIntentId),
    }),
  };
}

async function verifyStoredPaidCheckoutRepairChecks(
  db: D1Database,
  row: PaidCheckoutMissingCodeRow,
): Promise<OneTimePaidCodeCallbackChecks> {
  return oneTimePaidCodeCallbackChecks({
    amountCheck: "paid_500_usd",
    paymentIntentId: row.payment_intent_id,
    refundCheck: "latest_terminal_observation_checked",
    repeatCheck: "repair_license_missing",
    sessionId: row.session_id,
    signatureCheck: "verified_stripe_commerce_record",
    terminalObservation: await terminalPaymentObservation(db, row.payment_intent_id),
  });
}

function oneTimePaidCodeCallbackChecks(
  checks: Omit<OneTimePaidCodeCallbackChecks, typeof oneTimePaidCodeCallbackChecksBrand>,
): OneTimePaidCodeCallbackChecks {
  return {
    [oneTimePaidCodeCallbackChecksBrand]: true,
    ...checks,
  } as OneTimePaidCodeCallbackChecks;
}

function assertOneTimePaidCodeCallbackChecks(
  checks: OneTimePaidCodeCallbackChecks,
): OneTimePaidCodeCallbackChecks {
  if (checks[oneTimePaidCodeCallbackChecksBrand] !== true) {
    throw new Error("one-time paid code callback checks missing");
  }
  return checks;
}

async function terminalPaymentObservation(
  db: D1Database,
  paymentIntentId: string,
): Promise<TerminalPaymentObservation | null> {
  const observation = await getLatestSubscriptionObservation(db, paymentIntentId);
  if (observation?.status !== "REVOKED" && observation?.status !== "EXPIRED") {
    return null;
  }
  return {
    eventType: observation.event_type,
    status: observation.status,
  };
}

/**
 * Issue a redeemable one-month code after a verified one-time payment.
 *
 * The legacy schema names the entitlement relation `subscriptions`, but this
 * path stores no customer id, email, billing profile, or expiry. The only
 * Stripe reference retained is the PaymentIntent id needed to make webhook
 * retries and later disputes converge on the same entitlement.
 */
export async function completeOneTimeStripeCheckoutClaim(
  db: D1Database,
  checks: OneTimePaidCodeCallbackChecks,
): Promise<"completed" | "already_completed" | "missing"> {
  const verified = assertOneTimePaidCodeCallbackChecks(checks);
  const claim = await getStripeCheckoutClaim(db, verified.sessionId);
  if (!claim) return "missing";
  if (
    (claim.status === "delivery_ready" || claim.status === "expired") &&
    claim.subscription_id === verified.paymentIntentId
  ) {
    await reconcileTerminalOneTimeObservation(db, verified.paymentIntentId);
    return "already_completed";
  }
  if (claim.status !== "pending") return "missing";

  const inserted = await makeOneTimePaidCodeAfterCallbackChecks(db, {
    license_hash: claim.license_hash,
    payment_intent_id: verified.paymentIntentId,
    session_id: verified.sessionId,
  }, verified);
  if (inserted !== 1) {
    await reconcileTerminalOneTimeObservation(db, verified.paymentIntentId);
    return "already_completed";
  }
  await reconcileTerminalOneTimeObservation(db, verified.paymentIntentId);
  return "completed";
}

async function makeOneTimePaidCodeAfterCallbackChecks(
  db: D1Database,
  row: PaidCheckoutMissingCodeRow,
  checks: OneTimePaidCodeCallbackChecks,
): Promise<number> {
  const verified = assertOneTimePaidCodeCallbackChecks(checks);
  const now = Math.floor(Date.now() / 1000);
  const priorTerminal = verified.terminalObservation !== null;
  // A paid checkout is not an entitlement.  The code is deliberately inert
  // until the holder redeems it; only then does license-redeem grant its
  // bounded period.  PENDING is the existing non-entitled subscription state.
  const initialStatus = priorTerminal ? verified.terminalObservation!.status : "PENDING";
  const initialRevokedAt = priorTerminal ? now : null;
  const initialRevokedReason = priorTerminal
    ? observationRevocationReason(verified.terminalObservation!.eventType)
    : null;
  const entitlementId = oneTimeEntitlementId(row.license_hash);
  const results = await db.batch([
    db.prepare(
      `INSERT INTO subscriptions (
         subscription_id, customer_id, customer_email, status,
         current_period_end, cancel_at_period_end, created_at, updated_at,
         is_comp
       ) VALUES (?, '', '', ?, NULL, 0, ?, ?, 0)
       ON CONFLICT(subscription_id) DO UPDATE SET
         status = CASE
           WHEN subscriptions.status IN ('REVOKED', 'EXPIRED') THEN subscriptions.status
           ELSE excluded.status
         END,
         current_period_end = NULL,
         cancel_at_period_end = 0, updated_at = excluded.updated_at`,
    ).bind(entitlementId, initialStatus, now, now),
    db.prepare(
      `INSERT OR IGNORE INTO licenses (
         license_hash, subscription_id, issued_at, grant_seconds, revoked_at, revoked_reason
       ) VALUES (?, ?, ?, ?, ?, ?)`,
    ).bind(
      row.license_hash,
      entitlementId,
      now,
      PREPAID_PRO_GRANT_SECONDS,
      initialRevokedAt,
      initialRevokedReason,
    ),
    db.prepare(
      `UPDATE stripe_checkout_claims
          SET subscription_id = ?,
              status = CASE WHEN EXISTS (
                SELECT 1 FROM stripe_subscription_observations
                 WHERE subscription_id = ? AND status IN ('REVOKED', 'EXPIRED')
              ) THEN 'expired' ELSE 'delivery_ready' END,
              expires_at = CASE WHEN EXISTS (
                SELECT 1 FROM stripe_subscription_observations
                 WHERE subscription_id = ? AND status IN ('REVOKED', 'EXPIRED')
              ) THEN ? ELSE ? END
        WHERE session_id = ? AND status = 'pending'`,
    ).bind(
      row.payment_intent_id,
      row.payment_intent_id,
      row.payment_intent_id,
      now,
      now + COMPLETED_STRIPE_CLAIM_LIFETIME_SECONDS,
      row.session_id,
    ),
  ]);
  return results[1]?.meta?.changes === 1 ? 1 : 0;
}

export interface PaidCheckoutMissingCodeRepairResult {
  found: number;
  codesCreated: number;
}

interface PaidCheckoutMissingCodeRow {
  session_id: string;
  payment_intent_id: string;
  license_hash: string;
}

/**
 * Repair paid one-time checkout rows that reached browser delivery state before
 * the matching prepaid license row was written. The encrypted plaintext already
 * exists in `stripe_checkout_claims`; this only restores the server-side code
 * record required for redemption/validation. Re-running converges on zero work.
 */
export async function repairPaidStripeCheckoutsMissingCodes(
  db: D1Database,
): Promise<PaidCheckoutMissingCodeRepairResult> {
  const rows = await db.prepare(
    `SELECT claims.session_id,
            claims.subscription_id AS payment_intent_id,
            claims.license_hash
       FROM stripe_checkout_claims AS claims
       JOIN commerce_events
             ON commerce_events.stripe_object_id = claims.session_id
       LEFT JOIN licenses
              ON licenses.license_hash = claims.license_hash
      WHERE claims.status = 'delivery_ready'
        AND claims.subscription_id IS NOT NULL
        AND commerce_events.event_type IN (
          'checkout.session.completed',
          'checkout.session.async_payment_succeeded'
        )
        AND commerce_events.amount_cents = 500
        AND commerce_events.currency = 'usd'
        AND licenses.license_hash IS NULL
      ORDER BY claims.created_at ASC, claims.session_id ASC`,
  ).all<PaidCheckoutMissingCodeRow>();
  const missing = rows.results ?? [];
  let codesCreated = 0;

  for (const row of missing) {
    const checks = await verifyStoredPaidCheckoutRepairChecks(db, row);
    const inserted = await makeOneTimePaidCodeAfterCallbackChecks(db, row, checks);
    codesCreated += inserted;
    if (inserted === 1) {
      await reconcileTerminalOneTimeObservation(db, row.payment_intent_id);
    }
  }

  return { found: missing.length, codesCreated };
}
function oneTimeEntitlementId(licenseHash: string): string {
  return `lic_${licenseHash}`;
}

function observationRevocationReason(eventType: string): "chargeback" | "manual" {
  return eventType === "charge.dispute.created" ? "chargeback" : "manual";
}

/**
 * Stripe does not guarantee webhook order. A refund or dispute can be observed
 * before a delayed checkout completion creates the entitlement row. Reapply
 * that terminal observation after issuance and revoke the bearer credential so
 * a late completion can never restore usable Pro access. This also runs on
 * completion retries, closing the gap after a Worker interruption.
 */
async function reconcileTerminalOneTimeObservation(
  db: D1Database,
  paymentIntentId: string,
): Promise<void> {
  const observed = await applyLatestSubscriptionObservation(db, paymentIntentId);
  if (observed?.status !== "REVOKED" && observed?.status !== "EXPIRED") return;
  await revokeOneTimeLicensesForPayment(
    db,
    paymentIntentId,
    observationRevocationReason(observed.event_type),
  );
}

export async function revokeOneTimeLicensesForPayment(
  db: D1Database,
  paymentIntentId: string,
  reason: "chargeback" | "manual",
): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  await db.batch([
    db.prepare(
      `UPDATE licenses
          SET revoked_at = COALESCE(revoked_at, ?),
              revoked_reason = COALESCE(revoked_reason, ?)
        WHERE license_hash IN (
          SELECT license_hash FROM stripe_checkout_claims
           WHERE subscription_id = ?
        )
          AND revoked_at IS NULL`,
    ).bind(now, reason, paymentIntentId),
    db.prepare(
      `UPDATE subscriptions
          SET status = 'REVOKED', updated_at = ?
        WHERE subscription_id IN (
          SELECT licenses.subscription_id
            FROM licenses
            JOIN stripe_checkout_claims
              ON stripe_checkout_claims.license_hash = licenses.license_hash
          WHERE stripe_checkout_claims.subscription_id = ?
        )
          AND status NOT IN ('REVOKED', 'EXPIRED')`,
    ).bind(now, paymentIntentId),
    db.prepare(
      `UPDATE stripe_checkout_claims
          SET status = 'expired',
              expires_at = CASE WHEN expires_at > ? THEN ? ELSE expires_at END
        WHERE subscription_id = ?
          AND status != 'expired'`,
    ).bind(now, now, paymentIntentId),
  ]);

  // Keep old rows revocable during migration; new prepaid rows never put a
  // PaymentIntent id in licenses.subscription_id.
  await revokeLicensesForSubscription(
    db,
    paymentIntentId,
    reason,
  );
}

/** Record first ciphertext delivery without making network loss destructive. */
export async function markStripeClaimFetched(
  db: D1Database,
  sessionId: string,
): Promise<void> {
  await db.prepare(
    `UPDATE stripe_checkout_claims
        SET delivered_at = COALESCE(delivered_at, ?)
      WHERE session_id = ? AND status = 'delivery_ready' AND acknowledged_at IS NULL`,
  ).bind(Math.floor(Date.now() / 1000), sessionId).run();
}

/**
 * Tombstone the ciphertext only after the browser says local RSA decryption
 * and local persistence succeeded. Repeating the ACK is intentionally safe.
 */
export async function acknowledgeStripeClaimDelivery(
  db: D1Database,
  sessionId: string,
): Promise<"acknowledged" | "already_acknowledged" | "not_ready"> {
  const now = Math.floor(Date.now() / 1000);
  const changed = await db.prepare(
    `UPDATE stripe_checkout_claims
        SET acknowledged_at = ?, encrypted_license = ''
      WHERE session_id = ? AND status = 'delivery_ready' AND acknowledged_at IS NULL`,
  ).bind(now, sessionId).run();
  if ((changed.meta?.changes ?? 0) === 1) return "acknowledged";
  const row = await db.prepare(
    "SELECT status, acknowledged_at FROM stripe_checkout_claims WHERE session_id = ?",
  ).bind(sessionId).first<{ status: string; acknowledged_at: number | null }>();
  if (row?.status === "delivery_ready" && row.acknowledged_at !== null) {
    return "already_acknowledged";
  }
  return "not_ready";
}

export async function sweepStripeCheckoutClaims(db: D1Database): Promise<number> {
  const now = Math.floor(Date.now() / 1000);
  const completed = await db.prepare(
    `DELETE FROM stripe_checkout_claims
      WHERE status = 'delivery_ready' AND expires_at < ?`,
  ).bind(now).run();
  const expired = await db.prepare(
    `UPDATE stripe_checkout_claims
        SET status = 'expired'
      WHERE status = 'pending' AND expires_at < ?`,
  ).bind(now).run();
  await db.prepare(
    `DELETE FROM stripe_checkout_claims
      WHERE status = 'expired' AND expires_at < ?`,
  ).bind(now - 7 * 24 * 60 * 60).run();
  return (completed.meta?.changes ?? 0) + (expired.meta?.changes ?? 0);
}
