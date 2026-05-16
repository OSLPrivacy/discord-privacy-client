import { SELF, fetchMock } from "cloudflare:test";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";

// Need to enable + reset the mock pool each test file.
beforeAll(() => {
  fetchMock.activate();
  fetchMock.disableNetConnect();
});
afterEach(() => fetchMock.assertNoPendingInterceptors());
beforeEach(() => {
  // No-op — fetchMock state is per-test-file via the pool.
});

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
