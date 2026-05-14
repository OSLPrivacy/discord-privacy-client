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
});
