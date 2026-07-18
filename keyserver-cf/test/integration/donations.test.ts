import { env, SELF } from "cloudflare:test";
import { describe, expect, it, vi } from "vitest";
import { handleStripeDonationSession } from "../../src/endpoints/donation-stripe.js";
import { handleStripeWebhook } from "../../src/endpoints/stripe-webhook.js";
import type { Env } from "../../src/env.js";
import { postSignedWebhook, signStripeWebhook, uniqueEventId } from "./helpers-stripe.js";

const BOT_TOKEN = "1234567890:abcdefghijklmnopqrstuvwxyzABCDE";

function requestToken(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function configuredEnv(overrides: Partial<Env> = {}): Env {
  return {
    ...env,
    STRIPE_SECRET_KEY: "rk_live_donation_test",
    DONATION_SUCCESS_URL: "https://oslprivacy.com/donate?status=thanks",
    DONATION_CANCEL_URL: "https://oslprivacy.com/donate",
    ...overrides,
  };
}

function donationObject(
  paymentIntent: string,
  amount = 2000,
  overrides: Record<string, unknown> = {},
): Record<string, unknown> {
  return {
    id: `cs_live_${crypto.randomUUID().replace(/-/g, "")}`,
    mode: "payment",
    payment_status: "paid",
    payment_intent: paymentIntent,
    amount_total: amount,
    currency: "usd",
    metadata: {
      osl_kind: "donation",
      osl_purchase: "one-time",
      osl_fulfillment: "none",
      osl_donation_amount_cents: String(amount),
    },
    ...overrides,
  };
}

async function tableCount(table: string): Promise<number> {
  const row = await env.DB.prepare(`SELECT COUNT(*) AS count FROM ${table}`)
    .first<{ count: number }>();
  return row?.count ?? 0;
}

describe("POST /v1/donations/stripe/session", () => {
  it.each([500, 2000, 5000] as const)(
    "maps %i cents to a server-owned one-time amount",
    async (amount) => {
    const token = requestToken();
    const idempotencyKeys: string[] = [];
    const fetcher = vi.fn<typeof fetch>(async (_input, init) => {
      const form = new URLSearchParams(String(init?.body));
      const headers = new Headers(init?.headers);
      expect(form.get("mode")).toBe("payment");
      expect(form.get("customer_creation")).toBe("if_required");
      expect(form.has("line_items[0][price]")).toBe(false);
      expect(form.get("line_items[0][price_data][currency]")).toBe("usd");
      expect(form.get("line_items[0][price_data][unit_amount]")).toBe(String(amount));
      expect(form.get("line_items[0][price_data][product_data][name]")).toBe(
        "Support OSL open-source privacy work",
      );
      expect(form.get("metadata[osl_kind]")).toBe("donation");
      expect(form.get("metadata[osl_donation_amount_cents]")).toBe(String(amount));
      expect(form.get("payment_intent_data[metadata][osl_kind]")).toBe("donation");
      expect(form.get("payment_intent_data[metadata][osl_fulfillment]")).toBe("none");
      expect(form.has("customer")).toBe(false);
      expect(form.has("customer_email")).toBe(false);
      expect(form.has("payment_intent_data[setup_future_usage]")).toBe(false);
      idempotencyKeys.push(headers.get("idempotency-key") ?? "");
      return Response.json({
        id: `cs_live_donation_${amount}`,
        url: `https://checkout.stripe.com/c/pay/${amount}`,
      });
    });
    const send = () => handleStripeDonationSession(new Request(
      "https://keyserver.test/v1/donations/stripe/session",
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ amount_usd_cents: amount, request_token: token }),
      },
    ), configuredEnv(), fetcher);

    expect((await send()).status).toBe(200);
    expect((await send()).status).toBe(200);
    expect(idempotencyKeys).toHaveLength(2);
    expect(idempotencyKeys[0]).toMatch(/^osl-donation-v1-/);
    expect(idempotencyKeys[1]).toBe(idempotencyKeys[0]);
    },
  );

  it("rejects arbitrary amounts, malformed tokens, and identity-bearing fields", async () => {
    const fetcher = vi.fn<typeof fetch>();
    for (const body of [
      null,
      [],
      { amount_usd_cents: 501, request_token: requestToken() },
      { amount_usd_cents: 500, request_token: "short" },
      { amount_usd_cents: 500, request_token: requestToken(), email: "do-not-store@example.test" },
      { amount_usd_cents: 500, request_token: requestToken(), message: "do not retain this" },
    ]) {
      const response = await handleStripeDonationSession(new Request(
        "https://keyserver.test/v1/donations/stripe/session",
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(body),
        },
      ), configuredEnv(), fetcher);
      expect(response.status).toBe(400);
    }
    expect(fetcher).not.toHaveBeenCalled();
  });

  it.each(["sk_test_not_live", "rk_test_not_live", "pk_live_publishable"])(
    "fails closed for non-live Stripe key %s",
    async (stripeKey) => {
      const fetcher = vi.fn<typeof fetch>();
      const response = await handleStripeDonationSession(new Request(
        "https://keyserver.test/v1/donations/stripe/session",
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ amount_usd_cents: 500, request_token: requestToken() }),
        },
      ), configuredEnv({ STRIPE_SECRET_KEY: stripeKey }), fetcher);
      expect(response.status).toBe(503);
      expect(fetcher).not.toHaveBeenCalled();
    },
  );

  it("has an explicit browser CORS preflight", async () => {
    const response = await SELF.fetch("http://test/v1/donations/stripe/session", {
      method: "OPTIONS",
      headers: { origin: "https://oslprivacy.com" },
    });
    expect(response.status).toBe(204);
    expect(response.headers.get("access-control-allow-origin")).toBe("https://oslprivacy.com");
  });
});

describe("verified Stripe donation ledger", () => {
  it("deduplicates by PaymentIntent and never creates an entitlement", async () => {
    const paymentIntent = `pi_donation_${crypto.randomUUID().replace(/-/g, "")}`;
    const before = {
      subscriptions: await tableCount("subscriptions"),
      licenses: await tableCount("licenses"),
      claims: await tableCount("stripe_checkout_claims"),
      commerce: await tableCount("commerce_events"),
    };
    const first = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "checkout.session.completed",
      data: { object: donationObject(paymentIntent) },
    });
    expect(first.status).toBe(200);
    await expect(first.json()).resolves.toMatchObject({
      kind: "noop",
      reason: "donation checkout has no entitlement",
    });

    const second = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "checkout.session.async_payment_succeeded",
      data: { object: donationObject(paymentIntent) },
    });
    expect(second.status).toBe(200);
    const stored = await env.DB.prepare(
      `SELECT provider, provider_reference, amount_usd_cents, currency
         FROM donation_events WHERE provider_reference = ?`,
    ).bind(paymentIntent).all<{
      provider: string;
      provider_reference: string;
      amount_usd_cents: number;
      currency: string;
    }>();
    expect(stored.results).toEqual([{
      provider: "stripe",
      provider_reference: paymentIntent,
      amount_usd_cents: 2000,
      currency: "usd",
    }]);
    expect(await tableCount("subscriptions")).toBe(before.subscriptions);
    expect(await tableCount("licenses")).toBe(before.licenses);
    expect(await tableCount("stripe_checkout_claims")).toBe(before.claims);
    expect(await tableCount("commerce_events")).toBe(before.commerce);
  });

  it("rejects mismatched or entitlement-shaped donation metadata", async () => {
    for (const object of [
      donationObject(`pi_donation_${crypto.randomUUID().replace(/-/g, "")}`, 2000, {
        metadata: {
          osl_kind: "donation",
          osl_purchase: "one-time",
          osl_fulfillment: "none",
          osl_donation_amount_cents: "500",
        },
      }),
      donationObject(`pi_donation_${crypto.randomUUID().replace(/-/g, "")}`, 500, {
        metadata: {
          osl_kind: "donation",
          osl_plan: "pro",
          osl_purchase: "one-time",
          osl_fulfillment: "instant-v1",
          osl_donation_amount_cents: "500",
        },
      }),
    ]) {
      const paymentIntent = String(object.payment_intent);
      const response = await postSignedWebhook(SELF, {
        id: uniqueEventId(),
        type: "checkout.session.completed",
        data: { object },
      });
      expect(response.status).toBe(200);
      expect(await env.DB.prepare(
        "SELECT 1 AS present FROM donation_events WHERE provider_reference = ?",
      ).bind(paymentIntent).first()).toBeNull();
      expect(await env.DB.prepare(
        "SELECT 1 AS present FROM subscriptions WHERE subscription_id = ?",
      ).bind(paymentIntent).first()).toBeNull();
    }
  });

  it("stores no donor profile columns", async () => {
    const columns = await env.DB.prepare("PRAGMA table_info(donation_events)")
      .all<{ name: string }>();
    expect(columns.results.map((column) => column.name)).toEqual([
      "donation_id",
      "provider",
      "provider_reference",
      "amount_usd_cents",
      "currency",
      "occurred_at",
    ]);
    expect(columns.results.map((column) => column.name).join(" ")).not.toMatch(
      /email|name|customer|card|ip|message|account|license/i,
    );
  });

  it("sends one aggregate Telegram alert for the first PaymentIntent event only", async () => {
    const paymentIntent = `pi_alert_${crypto.randomUUID().replace(/-/g, "")}`;
    const fetcher = vi.fn<typeof fetch>(async () => Response.json({ ok: true }));
    const waits: Promise<unknown>[] = [];
    const ctx = {
      waitUntil(promise: Promise<unknown>) {
        waits.push(promise);
      },
    } as ExecutionContext;
    const workerEnv = configuredEnv({
      TELEGRAM_BOT_TOKEN: BOT_TOKEN,
      TELEGRAM_OPERATOR_CHAT_IDS: "1122334455",
    });
    const send = async (eventType: string) => {
      const raw = JSON.stringify({
        id: uniqueEventId(),
        type: eventType,
        livemode: true,
        created: Math.floor(Date.now() / 1000),
        data: { object: donationObject(paymentIntent) },
      });
      return await handleStripeWebhook(new Request("http://test/v1/stripe/webhook", {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "stripe-signature": await signStripeWebhook(raw),
        },
        body: raw,
      }), workerEnv, fetcher, ctx);
    };

    expect((await send("checkout.session.completed")).status).toBe(200);
    await Promise.all(waits.splice(0));
    expect(fetcher).toHaveBeenCalledOnce();
    const alert = JSON.parse(String(fetcher.mock.calls[0]?.[1]?.body)) as { text: string };
    expect(alert.text).toContain("OSL donation verified");
    expect(alert.text).toContain("$20.00 via Stripe");
    expect(alert.text).toContain("Verified donations:");
    expect(alert.text).not.toContain(paymentIntent);

    fetcher.mockClear();
    expect((await send("checkout.session.async_payment_succeeded")).status).toBe(200);
    await Promise.all(waits.splice(0));
    expect(fetcher).not.toHaveBeenCalled();
  });
});
