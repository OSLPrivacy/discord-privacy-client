import { env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  PREPAID_PRO_GRANT_SECONDS,
  repairPaidStripeCheckoutsMissingCodes,
} from "../../src/lib/stripe-checkout-claims.js";

async function directPaidNoCodeCount(sessionId: string): Promise<number> {
  const row = await env.DB.prepare(
    `SELECT COUNT(*) AS count
       FROM commerce_events
       JOIN stripe_checkout_claims AS claims
         ON claims.session_id = commerce_events.stripe_object_id
       LEFT JOIN licenses
         ON licenses.license_hash = claims.license_hash
      WHERE commerce_events.stripe_object_id = ?
        AND commerce_events.event_type = 'checkout.session.completed'
        AND commerce_events.amount_cents = 500
        AND commerce_events.currency = 'usd'
        AND claims.status = 'delivery_ready'
        AND claims.subscription_id IS NOT NULL
        AND licenses.license_hash IS NULL`,
  ).bind(sessionId).first<{ count: number }>();
  return row?.count ?? 0;
}

describe("TASK3192 paid-but-no-code repair", () => {
  it("finds a paid row with no code and repairs it exactly once", async () => {
    const suffix = crypto.randomUUID().replace(/-/g, "");
    const sessionId = `cs_live_task3192_${suffix}`;
    const paymentIntentId = `pi_task3192_${suffix}`;
    const licenseHash = `task3192-license-${suffix}`;
    const now = Math.floor(Date.now() / 1000);

    await env.DB.prepare(
      `INSERT INTO stripe_checkout_claims (
         session_id, claim_hash, delivery_public_key_spki,
         encrypted_license, license_hash, subscription_id, status,
         created_at, expires_at, delivered_at
       ) VALUES (?, ?, 'public-key', 'ciphertext', ?, ?, 'delivery_ready', ?, ?, NULL)`,
    ).bind(
      sessionId,
      `claim-${suffix}`,
      licenseHash,
      paymentIntentId,
      now,
      now + 3600,
    ).run();
    await env.DB.prepare(
      `INSERT INTO commerce_events (
         event_id, event_type, stripe_object_id, amount_cents,
         currency, occurred_at, livemode
       ) VALUES (?, 'checkout.session.completed', ?, 500, 'usd', ?, 1)`,
    ).bind(`evt_task3192_${suffix}`, sessionId, now).run();

    const directFound = await directPaidNoCodeCount(sessionId);
    console.log(`TASK3192 direct_paid_no_code_found=${directFound}`);
    expect(directFound).toBe(1);

    const firstRepair = await repairPaidStripeCheckoutsMissingCodes(env.DB);
    console.log(`TASK3192 repair_first_found=${firstRepair.found}`);
    console.log(`TASK3192 repair_first_codes_created=${firstRepair.codesCreated}`);
    expect(firstRepair).toEqual({ found: 1, codesCreated: 1 });

    const license = await env.DB.prepare(
      `SELECT COUNT(*) AS count,
              MIN(subscription_id) AS subscription_id,
              MIN(grant_seconds) AS grant_seconds,
              MIN(redeemed_at) AS redeemed_at,
              MIN(expires_at) AS expires_at
         FROM licenses
        WHERE license_hash = ?`,
    ).bind(licenseHash).first<{
      count: number;
      subscription_id: string;
      grant_seconds: number;
      redeemed_at: number | null;
      expires_at: number | null;
    }>();
    console.log(`TASK3192 license_rows_after_first=${license?.count ?? 0}`);
    expect(license).toEqual({
      count: 1,
      subscription_id: `lic_${licenseHash}`,
      grant_seconds: PREPAID_PRO_GRANT_SECONDS,
      redeemed_at: null,
      expires_at: null,
    });

    const secondRepair = await repairPaidStripeCheckoutsMissingCodes(env.DB);
    console.log(`TASK3192 repair_second_found=${secondRepair.found}`);
    console.log(`TASK3192 repair_second_codes_created=${secondRepair.codesCreated}`);
    expect(secondRepair).toEqual({ found: 0, codesCreated: 0 });
    expect(await directPaidNoCodeCount(sessionId)).toBe(0);
  });
});
