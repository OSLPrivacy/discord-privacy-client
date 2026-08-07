import { encryptLicenseForDelivery } from "./anonymous-crypto.js";
import { generateLicenseKey } from "./license.js";
import { sha256Hex } from "./crypto-watcher-auth.js";
import {
  applyLatestSubscriptionObservation,
  getLatestSubscriptionObservation,
} from "./stripe-subscription-observations.js";
import { revokeLicensesForSubscription } from "./subscriptions.js";

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
  input: {
    sessionId: string;
    paymentIntentId: string;
  },
): Promise<"completed" | "already_completed" | "missing"> {
  const claim = await getStripeCheckoutClaim(db, input.sessionId);
  if (!claim) return "missing";
  if (
    (claim.status === "delivery_ready" || claim.status === "expired") &&
    claim.subscription_id === input.paymentIntentId
  ) {
    await reconcileTerminalOneTimeObservation(db, input.paymentIntentId);
    return "already_completed";
  }
  if (claim.status !== "pending") return "missing";

  const now = Math.floor(Date.now() / 1000);
  const priorObservation = await getLatestSubscriptionObservation(db, input.paymentIntentId);
  const priorTerminal = priorObservation?.status === "REVOKED" ||
    priorObservation?.status === "EXPIRED";
  // A paid checkout is not an entitlement.  The code is deliberately inert
  // until the holder redeems it; only then does license-redeem grant its
  // bounded period.  PENDING is the existing non-entitled subscription state.
  const initialStatus = priorTerminal ? priorObservation.status : "PENDING";
  const initialRevokedAt = priorTerminal ? now : null;
  const initialRevokedReason = priorTerminal
    ? observationRevocationReason(priorObservation.event_type)
    : null;
  const entitlementId = oneTimeEntitlementId(claim.license_hash);
  await db.batch([
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
      claim.license_hash,
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
      input.paymentIntentId,
      input.paymentIntentId,
      input.paymentIntentId,
      now,
      now + COMPLETED_STRIPE_CLAIM_LIFETIME_SECONDS,
      input.sessionId,
    ),
  ]);
  await reconcileTerminalOneTimeObservation(db, input.paymentIntentId);
  return "completed";
}

/**
 * Repair one-time payments whose checkout claim was marked paid/deliverable
 * before the prepaid license row landed. The license hash is already part of
 * the browser-bound claim, so the repair can converge without Stripe customer
 * data or plaintext license material.
 */
export async function repairPaidOneTimeCheckoutClaimsWithoutCodes(
  db: D1Database,
): Promise<number> {
  const now = Math.floor(Date.now() / 1000);
  const statements = await db.batch([
    db.prepare(
      `INSERT OR IGNORE INTO subscriptions (
         subscription_id, customer_id, customer_email, status,
         current_period_end, cancel_at_period_end, created_at, updated_at,
         is_comp
       )
       SELECT 'lic_' || license_hash, '', '', 'PENDING', NULL, 0, ?, ?, 0
         FROM stripe_checkout_claims
        WHERE status = 'delivery_ready'
          AND substr(subscription_id, 1, 3) = 'pi_'
          AND NOT EXISTS (
            SELECT 1 FROM licenses
             WHERE licenses.license_hash = stripe_checkout_claims.license_hash
          )`,
    ).bind(now, now),
    db.prepare(
      `INSERT OR IGNORE INTO licenses (
         license_hash, subscription_id, issued_at, grant_seconds,
         revoked_at, revoked_reason
       )
       SELECT license_hash, 'lic_' || license_hash, ?, ?, NULL, NULL
         FROM stripe_checkout_claims
        WHERE status = 'delivery_ready'
          AND substr(subscription_id, 1, 3) = 'pi_'
          AND NOT EXISTS (
            SELECT 1 FROM licenses
             WHERE licenses.license_hash = stripe_checkout_claims.license_hash
          )`,
    ).bind(now, PREPAID_PRO_GRANT_SECONDS),
  ]);
  return statements[1]?.meta?.changes ?? 0;
}

/**
 * Repair one authenticated browser-bound one-time checkout claim. This is the
 * buyer-facing recovery path: a paid claim can re-create its missing durable
 * license row without touching Stripe or starting another charge.
 */
export async function repairPaidOneTimeCheckoutClaimWithoutCode(
  db: D1Database,
  sessionId: string,
): Promise<"repaired" | "already_has_code" | "not_repairable"> {
  const claim = await db.prepare(
    `SELECT license_hash, subscription_id, status
       FROM stripe_checkout_claims
      WHERE session_id = ?`,
  ).bind(sessionId).first<{
    license_hash: string;
    subscription_id: string | null;
    status: string;
  }>();
  if (!claim || claim.status !== "delivery_ready" || !claim.subscription_id?.startsWith("pi_")) {
    return "not_repairable";
  }
  const existing = await db.prepare(
    "SELECT 1 AS present FROM licenses WHERE license_hash = ?",
  ).bind(claim.license_hash).first<{ present: number }>();
  if (existing) return "already_has_code";

  const now = Math.floor(Date.now() / 1000);
  const entitlementId = oneTimeEntitlementId(claim.license_hash);
  const statements = await db.batch([
    db.prepare(
      `INSERT OR IGNORE INTO subscriptions (
         subscription_id, customer_id, customer_email, status,
         current_period_end, cancel_at_period_end, created_at, updated_at,
         is_comp
       ) VALUES (?, '', '', 'PENDING', NULL, 0, ?, ?, 0)`,
    ).bind(entitlementId, now, now),
    db.prepare(
      `INSERT OR IGNORE INTO licenses (
         license_hash, subscription_id, issued_at, grant_seconds,
         revoked_at, revoked_reason
       ) VALUES (?, ?, ?, ?, NULL, NULL)`,
    ).bind(claim.license_hash, entitlementId, now, PREPAID_PRO_GRANT_SECONDS),
  ]);
  return (statements[1]?.meta?.changes ?? 0) === 1 ? "repaired" : "already_has_code";
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

async function revokeOneTimeLicensesForPayment(
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
          SET status = ?, updated_at = ?
        WHERE subscription_id IN (
          SELECT licenses.subscription_id
            FROM licenses
            JOIN stripe_checkout_claims
              ON stripe_checkout_claims.license_hash = licenses.license_hash
           WHERE stripe_checkout_claims.subscription_id = ?
        )
          AND status NOT IN ('REVOKED', 'EXPIRED')`,
    ).bind(reason === "chargeback" ? "REVOKED" : "EXPIRED", now, paymentIntentId),
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
