import { describe, expect, it, vi } from "vitest";
import { createCheckoutSession } from "../../src/lib/stripe.js";

describe("privacy-minimal one-time Stripe Checkout", () => {
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
});
