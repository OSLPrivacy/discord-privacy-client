import { describe, expect, it, vi } from "vitest";
import { createCheckoutSession } from "../../src/lib/stripe.js";
import {
  completeOneTimeStripeCheckoutClaim,
  PREPAID_PRO_GRANT_SECONDS,
} from "../../src/lib/stripe-checkout-claims.js";

function issuanceDb(): { db: D1Database; batches: unknown[][] } {
  const batches: unknown[][] = [];
  const pendingClaim = {
    session_id: "cs_card_month",
    claim_hash: "claim",
    delivery_public_key_spki: "spki",
    encrypted_license: "ciphertext",
    license_hash: "license-card-month",
    subscription_id: null,
    status: "pending",
    created_at: 1,
    expires_at: 2,
    delivered_at: null,
    acknowledged_at: null,
  };
  const db = {
    prepare(sql: string) {
      return {
        bind(...values: unknown[]) {
          return {
            sql,
            values,
            first: async () => sql.includes("stripe_checkout_claims") ? pendingClaim : null,
            run: async () => ({ success: true }),
          };
        },
      };
    },
    async batch(statements: unknown[]) {
      batches.push(statements);
      return [];
    },
  } as unknown as D1Database;
  return { db, batches };
}

describe("privacy-minimal one-time Stripe Checkout", () => {
  it("defines card purchases as an unredeemed one-month grant", () => {
    expect(PREPAID_PRO_GRANT_SECONDS).toBe(30 * 24 * 60 * 60);
  });

  it("issues an unredeemed monthly code, never an active lifetime entitlement", async () => {
    const { db, batches } = issuanceDb();

    await expect(completeOneTimeStripeCheckoutClaim(db, {
      sessionId: "cs_card_month",
      paymentIntentId: "pi_card_month",
    })).resolves.toBe("completed");

    const licenseInsert = batches[0]?.[1] as { sql: string; values: unknown[] };
    expect(licenseInsert.sql).toContain("grant_seconds");
    // `e8519b681 B0-07a P-44: unlink redeemed licenses from payment ids` moved
    // prepaid issuance off the PaymentIntent id: licenses.subscription_id is
    // now `lic_<license_hash>`, so a license row carries no Stripe payment
    // reference.  That commit updated test/integration/stripe-webhook.test.ts
    // (which now pins `expect.stringMatching(/^lic_/)` and asserts the value is
    // NOT the PaymentIntent id) but missed this unit test, and no CI run caught
    // it (D-172).  Pinning the literal keeps the unlinking under test here.
    expect(licenseInsert.values).toEqual([
      "license-card-month",
      "lic_license-card-month",
      expect.any(Number),
      PREPAID_PRO_GRANT_SECONDS,
      null,
      null,
    ]);
    // Redemption fields are deliberately absent from issuance: redeem is the
    // only operation allowed to start the month.
    expect(licenseInsert.sql).not.toContain("redeemed_at");
    // Same two commits, same relation.  `0c232eba7 T16-B5 issue unredeemed
    // prepaid card codes` changed the issued entitlement from ACTIVE to
    // PENDING precisely so a paid checkout is not itself an entitlement — which
    // is what this test's own name asserts ("never an active lifetime
    // entitlement").  The stale "ACTIVE" here contradicted the title.
    expect(batches[0]?.[0]).toMatchObject({
      values: ["lic_license-card-month", "PENDING", expect.any(Number), expect.any(Number)],
    });
  });
  it("creates a payment session without subscription, email, or saved-payment fields", async () => {
    const fetcher = vi.fn<typeof fetch>(async (_input, init) => {
      const form = new URLSearchParams(String(init?.body));
      expect(form.get("mode")).toBe("payment");
      expect(form.get("customer_creation")).toBe("if_required");
      expect(form.get("payment_method_types[0]")).toBe("card");
      expect(form.get("line_items[0][price]")).toBe("price_one_time_pro");
      expect(form.get("line_items[0][quantity]")).toBe("1");
      expect(form.has("customer")).toBe(false);
      expect(form.has("customer_email")).toBe(false);
      expect(form.has("subscription_data[metadata][osl_plan]")).toBe(false);
      expect(form.has("payment_intent_data[setup_future_usage]")).toBe(false);
      return new Response(JSON.stringify({
        id: "cs_live_one_time",
        url: "https://checkout.stripe.com/c/pay/cs_live_one_time",
      }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    });

    await expect(createCheckoutSession("sk_live_restricted", {
      priceId: "price_one_time_pro",
      successUrl: "https://oslprivacy.com/download?session_id={CHECKOUT_SESSION_ID}",
      cancelUrl: "https://oslprivacy.com/pricing",
      metadata: { osl_plan: "pro", osl_purchase: "one-time" },
    }, fetcher)).resolves.toMatchObject({ id: "cs_live_one_time" });
    expect(fetcher).toHaveBeenCalledOnce();
  });

  it("copies non-personal donation metadata and sends a stable idempotency key", async () => {
    const fetcher = vi.fn<typeof fetch>(async (_input, init) => {
      const form = new URLSearchParams(String(init?.body));
      const headers = new Headers(init?.headers);
      expect(form.get("metadata[osl_kind]")).toBe("donation");
      expect(form.get("payment_intent_data[metadata][osl_kind]")).toBe("donation");
      expect(form.has("customer")).toBe(false);
      expect(form.has("customer_email")).toBe(false);
      expect(form.has("payment_intent_data[setup_future_usage]")).toBe(false);
      expect(headers.get("idempotency-key")).toBe("stable-donation-request");
      return Response.json({
        id: "cs_live_donation",
        url: "https://checkout.stripe.com/c/pay/cs_live_donation",
      });
    });

    await createCheckoutSession("sk_live_restricted", {
      priceId: "price_donation_2000",
      successUrl: "https://oslprivacy.com/donate?status=thanks",
      cancelUrl: "https://oslprivacy.com/donate",
      metadata: { osl_kind: "donation" },
      paymentIntentMetadata: { osl_kind: "donation" },
      idempotencyKey: "stable-donation-request",
    }, fetcher);
    expect(fetcher).toHaveBeenCalledOnce();
  });
});
