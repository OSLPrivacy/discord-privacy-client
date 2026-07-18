import { SELF, env } from "cloudflare:test";
import { describe, expect, it, vi } from "vitest";
import { handleCheckout } from "../../src/endpoints/checkout.js";
import type { Env as WorkerEnv } from "../../src/env.js";

function claimToken(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

async function deliveryPublicKey(): Promise<string> {
  const pair = await crypto.subtle.generateKey({
    name: "RSA-OAEP",
    modulusLength: 2048,
    publicExponent: new Uint8Array([1, 0, 1]),
    hash: "SHA-256",
  }, true, ["encrypt", "decrypt"]) as CryptoKeyPair;
  const bytes = new Uint8Array(
    await crypto.subtle.exportKey("spki", pair.publicKey) as ArrayBuffer,
  );
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function configuredEnv(stripeKey = "sk_live_restricted_test"): WorkerEnv {
  return {
    ...env,
    STRIPE_SECRET_KEY: stripeKey,
    STRIPE_PRICE_ID_PRO: "price_one_time_pro",
    CHECKOUT_SUCCESS_URL: "https://oslprivacy.com/download",
    CHECKOUT_CANCEL_URL: "https://oslprivacy.com/pricing",
  };
}

describe("POST /v1/checkout-session", () => {
  it("503s when Stripe is unconfigured", async () => {
    // Default test env has no STRIPE_SECRET_KEY (we set it
    // per-test below; default state is missing).
    const res = await SELF.fetch("http://test/v1/checkout-session", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ plan: "monthly" }),
    });
    expect(res.status).toBe(503);
  });

  it("includes Access-Control-Allow-Origin on the POST response (even on error)", async () => {
    const res = await SELF.fetch("http://test/v1/checkout-session", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ plan: "monthly" }),
    });
    expect(res.headers.get("Access-Control-Allow-Origin")).toBe(
      "https://oslprivacy.com",
    );
  });

  it("accepts plan pro and creates only a one-time payment session", async () => {
    const sessionId = `cs_live_pro_${crypto.randomUUID().replace(/-/g, "")}`;
    const fetcher = vi.fn<typeof fetch>(async (_input, init) => {
      const form = new URLSearchParams(String(init?.body));
      expect(form.get("mode")).toBe("payment");
      expect(form.get("line_items[0][price]")).toBe("price_one_time_pro");
      expect(form.has("customer_email")).toBe(false);
      return new Response(JSON.stringify({
        id: sessionId,
        url: `https://checkout.stripe.com/c/pay/${sessionId}`,
      }), { status: 200, headers: { "content-type": "application/json" } });
    });
    const response = await handleCheckout(new Request("https://test/v1/checkout-session", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        plan: "pro",
        claim_token: claimToken(),
        delivery_public_key_spki: await deliveryPublicKey(),
      }),
    }), configuredEnv(), fetcher);
    expect(response.status).toBe(200);
    await expect(response.json()).resolves.toMatchObject({ session_id: sessionId });
    expect(fetcher).toHaveBeenCalledOnce();
  });

  it("accepts a restricted live key for one-time checkout", async () => {
    const sessionId = `cs_live_restricted_${crypto.randomUUID().replace(/-/g, "")}`;
    const fetcher = vi.fn<typeof fetch>(async () => Response.json({
      id: sessionId,
      url: `https://checkout.stripe.com/c/pay/${sessionId}`,
    }));
    const response = await handleCheckout(new Request("https://test/v1/checkout-session", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        plan: "pro",
        claim_token: claimToken(),
        delivery_public_key_spki: await deliveryPublicKey(),
      }),
    }), configuredEnv("rk_live_restricted_test"), fetcher);
    expect(response.status).toBe(200);
    expect(fetcher).toHaveBeenCalledOnce();
  });

  it.each(["sk_test_not_live", "rk_test_not_live", "pk_live_publishable"])(
    "rejects non-live-secret Stripe key %s before calling Stripe",
    async (stripeKey) => {
      const fetcher = vi.fn<typeof fetch>();
      const response = await handleCheckout(new Request("https://test/v1/checkout-session", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          plan: "pro",
          claim_token: claimToken(),
          delivery_public_key_spki: await deliveryPublicKey(),
        }),
      }), configuredEnv(stripeKey), fetcher);
      expect(response.status).toBe(503);
      expect(fetcher).not.toHaveBeenCalled();
    },
  );

  it("rejects legacy recurring plan tokens", async () => {
    const fetcher = vi.fn<typeof fetch>();
    const response = await handleCheckout(new Request("https://test/v1/checkout-session", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ plan: "monthly" }),
    }), configuredEnv(), fetcher);
    expect(response.status).toBe(400);
    await expect(response.json()).resolves.toMatchObject({ error: 'plan must be "pro"' });
    expect(fetcher).not.toHaveBeenCalled();
  });
});

describe("CORS preflight", () => {
  it("OPTIONS /v1/checkout-session → 204 with full CORS headers", async () => {
    const res = await SELF.fetch("http://test/v1/checkout-session", {
      method: "OPTIONS",
    });
    expect(res.status).toBe(204);
    expect(res.headers.get("Access-Control-Allow-Origin")).toBe(
      "https://oslprivacy.com",
    );
    expect(res.headers.get("Access-Control-Allow-Methods")).toBe(
      "POST, OPTIONS",
    );
    expect(res.headers.get("Access-Control-Allow-Headers")).toBe(
      "Content-Type",
    );
    expect(res.headers.get("Access-Control-Max-Age")).toBe("86400");
  });

  it("OPTIONS /v1/billing-portal-session → 204 with CORS headers", async () => {
    const res = await SELF.fetch("http://test/v1/billing-portal-session", {
      method: "OPTIONS",
    });
    expect(res.status).toBe(204);
    expect(res.headers.get("Access-Control-Allow-Origin")).toBe(
      "https://oslprivacy.com",
    );
  });

  it("does NOT grant CORS to the Stripe webhook (browser-blocked by design)", async () => {
    const res = await SELF.fetch("http://test/v1/stripe/webhook", {
      method: "OPTIONS",
    });
    expect(res.status).toBe(405);
    expect(res.headers.get("Access-Control-Allow-Origin")).toBeNull();
  });
});
