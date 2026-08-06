import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  postSignedWebhook,
  uniqueEventId,
} from "./helpers-stripe.js";

describe("TASK3194 callback checks on payment path", () => {
  it("lets a good signed payment make exactly one code, including after repeat delivery", async () => {
    const suffix = crypto.randomUUID().replace(/-/g, "");
    const sessionId = `cs_live_task3194_${suffix}`;
    const paymentIntentId = `pi_task3194_${suffix}`;
    const licenseHash = `task3194-license-${suffix}`;
    const eventId = uniqueEventId("evt_task3194");
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
      licenseHash,
      now,
      now + 3600,
    ).run();

    const event = {
      id: eventId,
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
    };
    const first = await postSignedWebhook(SELF, event);
    expect(first.status).toBe(200);
    await expect(first.json()).resolves.toMatchObject({ kind: "applied" });

    const afterFirst = await licenseCount(licenseHash);
    console.log(`TASK3194 good_payment_code_count=${afterFirst}`);
    expect(afterFirst).toBe(1);

    const repeat = await postSignedWebhook(SELF, event);
    expect(repeat.status).toBe(200);
    await expect(repeat.json()).resolves.toMatchObject({ deduped: true });

    const afterRepeat = await licenseCount(licenseHash);
    console.log(`TASK3194 repeat_callback_code_count=${afterRepeat}`);
    expect(afterRepeat).toBe(1);
  });
});

async function licenseCount(licenseHash: string): Promise<number> {
  const row = await env.DB.prepare(
    "SELECT COUNT(*) AS count FROM licenses WHERE license_hash = ?",
  ).bind(licenseHash).first<{ count: number }>();
  return row?.count ?? 0;
}
