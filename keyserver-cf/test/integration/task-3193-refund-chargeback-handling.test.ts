import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { generateLicenseKey } from "../../src/lib/license.js";
import {
  postSignedWebhook,
  uniqueEventId,
} from "./helpers-stripe.js";

const HMAC = "osl-license-test-secret-v1";
const REFUNDED = "this code was refunded";

async function redeem(licenseKey: string): Promise<{
  status: string;
  checksum_ok: boolean;
  message?: string;
}> {
  const response = await SELF.fetch("http://test/v1/license/redeem", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ license_key: licenseKey }),
  });
  expect(response.status).toBe(200);
  return await response.json() as {
    status: string;
    checksum_ok: boolean;
    message?: string;
  };
}

async function validate(licenseKey: string): Promise<{
  status: string;
  checksum_ok: boolean;
  message?: string;
}> {
  const response = await SELF.fetch("http://test/v1/license/validate", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ license_key: licenseKey }),
  });
  expect(response.status).toBe(200);
  return await response.json() as {
    status: string;
    checksum_ok: boolean;
    message?: string;
  };
}

function proState(status: string | undefined): "Pro" | "Free" {
  return status === "ACTIVE" || status === "CANCELLED" || status === "GRACE"
    ? "Pro"
    : "Free";
}

describe("TASK3193 refund and chargeback handling", () => {
  it("turns a refunded code off and reports the plain refund reason", async () => {
    const suffix = crypto.randomUUID().replace(/-/g, "");
    const sessionId = `cs_live_task3193_${suffix}`;
    const paymentIntentId = `pi_task3193_${suffix}`;
    const { plaintext, hash } = await generateLicenseKey(HMAC);
    const now = Math.floor(Date.now() / 1000);

    await env.DB.prepare(
      `INSERT INTO stripe_checkout_claims (
         session_id, claim_hash, delivery_public_key_spki,
         encrypted_license, license_hash, subscription_id, status,
         created_at, expires_at, delivered_at
       ) VALUES (?, ?, 'public-key', 'ciphertext', ?, NULL, 'pending', ?, ?, NULL)`,
    ).bind(
      sessionId,
      `claim-${suffix}`,
      hash,
      now,
      now + 3600,
    ).run();

    const completion = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "checkout.session.completed",
      created: now,
      data: {
        object: {
          id: sessionId,
          mode: "payment",
          metadata: {
            osl_plan: "pro",
            osl_purchase: "one-time",
            osl_fulfillment: "instant-v1",
          },
          payment_status: "paid",
          payment_intent: paymentIntentId,
          amount_total: 500,
          currency: "usd",
        },
      },
    });
    expect(completion.status).toBe(200);

    const before = await redeem(plaintext);
    expect(before).toMatchObject({ status: "ACTIVE", checksum_ok: true });
    const beforeEntitlement = await env.DB.prepare(
      "SELECT status FROM subscriptions WHERE subscription_id = ?",
    ).bind(`lic_${hash}`).first<{ status: string }>();
    expect(proState(beforeEntitlement?.status)).toBe("Pro");

    const refund = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "charge.refunded",
      created: now + 1,
      data: {
        object: {
          id: `ch_task3193_${suffix}`,
          payment_intent: paymentIntentId,
          amount: 500,
          amount_refunded: 500,
          currency: "usd",
        },
      },
    });
    expect(refund.status).toBe(200);

    const after = await redeem(plaintext);
    expect(after).toMatchObject({
      status: "REVOKED",
      checksum_ok: true,
      message: REFUNDED,
    });
    const validation = await validate(plaintext);
    expect(validation).toMatchObject({
      status: "REVOKED",
      checksum_ok: true,
      message: REFUNDED,
    });
    const afterEntitlement = await env.DB.prepare(
      "SELECT status FROM subscriptions WHERE subscription_id = ?",
    ).bind(`lic_${hash}`).first<{ status: string }>();
    expect(proState(afterEntitlement?.status)).toBe("Free");

    const rows = await env.DB.prepare(
      `SELECT licenses.revoked_reason,
              claims.status AS claim_status
         FROM licenses
         JOIN stripe_checkout_claims AS claims
           ON claims.license_hash = licenses.license_hash
        WHERE licenses.license_hash = ?`,
    ).bind(hash).first<{ revoked_reason: string; claim_status: string }>();
    expect(rows).toEqual({ revoked_reason: "manual", claim_status: "expired" });

    console.log(`TASK3193 code_status_before_refund=${before.status}`);
    console.log(`TASK3193 pro_state_before_refund=${proState(beforeEntitlement?.status)}`);
    console.log(`TASK3193 refund_event_status=${refund.status}`);
    console.log(`TASK3193 code_status_after_refund=${after.status}`);
    console.log(`TASK3193 refusal_message=${after.message}`);
    console.log(`TASK3193 validate_refusal_message=${validation.message}`);
    console.log(`TASK3193 pro_state_after_refund=${proState(afterEntitlement?.status)}`);
    console.log(`TASK3193 checkout_claim_after_refund=${rows?.claim_status}`);
  });
});
