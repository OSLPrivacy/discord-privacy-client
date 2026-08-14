import { env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { handleCheckoutClaim } from "../../src/endpoints/checkout-claim.js";
import { handleLicenseRedeem } from "../../src/endpoints/license-redeem.js";
import {
  insertStripeCheckoutClaim,
  prepareStripeCheckoutClaim,
} from "../../src/lib/stripe-checkout-claims.js";
import { postSignedWebhook, uniqueEventId } from "./helpers-stripe.js";

function base64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function token(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return base64(bytes).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

describe("TASK3725 recover a code after delivery fails", () => {
  it("finds the code once after a delivery failure and redeems it without a second payment", async () => {
    const pair = await crypto.subtle.generateKey(
      {
        name: "RSA-OAEP",
        modulusLength: 2048,
        publicExponent: new Uint8Array([1, 0, 1]),
        hash: "SHA-256",
      },
      true,
      ["encrypt", "decrypt"],
    ) as CryptoKeyPair;
    const publicSpki = base64(new Uint8Array(
      await crypto.subtle.exportKey("spki", pair.publicKey) as ArrayBuffer,
    ));
    const claimToken = token();
    const sessionId = `cs_live_task3725_${crypto.randomUUID().replace(/-/g, "")}`;
    const paymentIntentId = `pi_task3725_${crypto.randomUUID().replace(/-/g, "")}`;
    const prepared = await prepareStripeCheckoutClaim({
      claimToken,
      deliveryPublicKeySpki: publicSpki,
      licenseHmacSecret: "osl-license-test-secret-v1",
    });
    await insertStripeCheckoutClaim(env.DB, {
      sessionId,
      ...prepared,
      deliveryPublicKeySpki: publicSpki,
      expiresAt: Math.floor(Date.now() / 1000) + 3600,
    });

    const counts = async () => {
      const row = await env.DB.prepare(
        `SELECT
           (SELECT COUNT(*) FROM licenses WHERE license_hash = ?) AS code_count,
           (SELECT COUNT(*) FROM commerce_events
             WHERE event_type = 'checkout.session.completed'
               AND stripe_object_id = ?) AS payment_count`,
      ).bind(prepared.licenseHash, sessionId).first<{
        code_count: number;
        payment_count: number;
      }>();
      return row ?? { code_count: -1, payment_count: -1 };
    };
    const decryptCode = async (encryptedLicense: string): Promise<string> => {
      const ciphertext = Uint8Array.from(
        atob(encryptedLicense),
        (character) => character.charCodeAt(0),
      );
      const plaintext = await crypto.subtle.decrypt(
        { name: "RSA-OAEP" },
        pair.privateKey,
        ciphertext,
      );
      return new TextDecoder().decode(plaintext);
    };
    const findMyCode = async () => {
      const response = await handleCheckoutClaim(
        new Request("http://test/v1/checkout/claim", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ session_id: sessionId, claim_token: claimToken }),
        }),
        env,
      );
      expect(response.status).toBe(200);
      const body = await response.json() as {
        status: string;
        encrypted_license: string;
        delivery: string;
      };
      expect(body.status).toBe("delivery_ready");
      return await decryptCode(body.encrypted_license);
    };
    const redeem = async (licenseKey: string) => {
      const response = await handleLicenseRedeem(
        new Request("http://test/v1/license/redeem", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ license_key: licenseKey }),
        }),
        env,
      );
      return { status: response.status, body: await response.json() as Record<string, unknown> };
    };

    // postSignedWebhook calls the webhook handler directly and ignores this
    // argument -- it is not routed through the full worker/index.ts.
    const unusedSelf = { fetch: async () => new Response(null) };

    // Confirm one payment.
    const beforePayment = await counts();
    const payment = await postSignedWebhook(unusedSelf, {
      id: uniqueEventId(),
      type: "checkout.session.completed",
      data: {
        object: {
          id: sessionId,
          mode: "payment",
          metadata: { osl_plan: "pro", osl_purchase: "one-time", osl_fulfillment: "instant-v1" },
          payment_status: "paid",
          payment_intent: paymentIntentId,
          amount_total: 500,
          currency: "usd",
        },
      },
    });
    expect(payment.status).toBe(200);
    const afterPayment = await counts();

    // Force delivery to fail: the payment landed, but the code row that
    // should have been written alongside it never made it to the table --
    // the same D1-batch-interruption gap TASK3723's repair path exists for.
    await env.DB.prepare(
      "DELETE FROM licenses WHERE license_hash = ?",
    ).bind(prepared.licenseHash).run();
    const beforeRepair = await counts();

    // Use Find my code -- no new payment is made anywhere in this flow.
    const firstCode = await findMyCode();
    const afterFirstRecovery = await counts();

    // Redeem the recovered code exactly once.
    const firstRedeem = await redeem(firstCode);
    const secondRedeem = await redeem(firstCode);
    const afterRedeem = await counts();

    // A second recovery (e.g. the buyer clicks "Find my code" again) returns
    // the same code and mints no second payment or second code row.
    const secondCode = await findMyCode();
    const afterSecondRecovery = await counts();

    expect(beforePayment).toEqual({ code_count: 0, payment_count: 0 });
    expect(afterPayment.payment_count).toBe(1);
    expect(beforeRepair).toEqual({ code_count: 0, payment_count: 1 });

    expect(firstCode).toMatch(/^OSL-[0-9A-HJKMNP-TV-Z]{4}(?:-[0-9A-HJKMNP-TV-Z]{4}){3}$/);
    expect(afterFirstRecovery).toEqual({ code_count: 1, payment_count: 1 });

    expect(firstRedeem.status).toBe(200);
    expect(firstRedeem.body).toMatchObject({ status: "ACTIVE", checksum_ok: true });
    expect(secondRedeem.status).toBe(409);
    expect(secondRedeem.body).toEqual({ error: "license code already redeemed" });
    expect(afterRedeem).toEqual({ code_count: 1, payment_count: 1 });

    expect(secondCode).toBe(firstCode);
    expect(afterSecondRecovery).toEqual({ code_count: 1, payment_count: 1 });

    console.log(`TASK3725 payment_count_before=${beforePayment.payment_count}`);
    console.log(`TASK3725 payment_count_after_confirm=${afterPayment.payment_count}`);
    console.log(`TASK3725 code_count_before_repair=${beforeRepair.code_count}`);
    console.log(`TASK3725 payment_count_before_repair=${beforeRepair.payment_count}`);
    console.log(`TASK3725 recovered_code=${firstCode}`);
    console.log(`TASK3725 code_count_after_recovery=${afterFirstRecovery.code_count}`);
    console.log(`TASK3725 first_redeem_status=${firstRedeem.status}`);
    console.log(`TASK3725 second_redeem_status=${secondRedeem.status}`);
    console.log(`TASK3725 second_recovery_code=${secondCode}`);
    console.log(`TASK3725 second_recovery_same_code=${secondCode === firstCode}`);
    console.log(`TASK3725 payment_count_after_second_recovery=${afterSecondRecovery.payment_count}`);
    console.log(`TASK3725 code_count_after_second_recovery=${afterSecondRecovery.code_count}`);
  });
});
