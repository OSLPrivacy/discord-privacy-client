import { SELF, env } from "cloudflare:test";
import { describe, expect, it, vi } from "vitest";
import {
  formatOperatorStats,
  getLiveStripeBalance,
  verifyTelegramWebhook,
} from "../../src/lib/telegram.js";

describe("privacy minimal commerce reporting", () => {
  it("counts a Windows download without storing request identity", async () => {
    const before = await env.DB.prepare(
      "SELECT COUNT(*) AS count FROM download_events",
    ).first<{ count: number }>();
    const response = await SELF.fetch("http://test/v1/download/windows", {
      headers: {
        "user-agent": "identity-that-must-not-be-stored",
        "x-forwarded-for": "192.0.2.200",
      },
      redirect: "manual",
    });
    expect(response.status).toBe(302);
    expect(response.headers.get("location")).toBe(
      "https://installers.oslprivacy.com/osl-privacy-0.0.1.msi",
    );
    const after = await env.DB.prepare(
      "SELECT COUNT(*) AS count FROM download_events",
    ).first<{ count: number }>();
    expect(after?.count).toBe((before?.count ?? 0) + 1);

    const columns = await env.DB.prepare(
      "PRAGMA table_info(download_events)",
    ).all<{ name: string }>();
    expect(columns.results.map((column) => column.name)).toEqual([
      "event_id",
      "artifact",
      "created_at",
    ]);
  });

  it("formats aggregate Telegram statistics without customer fields", () => {
    const report = formatOperatorStats({
      successful_payments: 7,
      gross_cents: 3500,
      refunds_and_disputes_cents: -500,
      verified_donations: 3,
      donation_gross_cents: 4500,
      active_subscriptions: 6,
      download_starts: 91,
      download_starts_24h: 4,
    }, {
      available: [{ amount: 1200, currency: "usd" }],
      pending: [{ amount: 500, currency: "usd" }],
    });
    expect(report).toContain("Payments: 7");
    expect(report).toContain("Gross verified: $35.00");
    expect(report).toContain("Donations: 3 ($45.00)");
    expect(report).toContain("Download requests: 91 (4 in 24h)");
    expect(report).toContain("Mode: LIVE");
    expect(report).not.toMatch(/email|customer|card|license|wallet/i);
  });

  it("accepts only the configured Telegram webhook secret", async () => {
    const good = new Request("https://example.test/webhook", {
      headers: { "x-telegram-bot-api-secret-token": "correct-secret" },
    });
    const bad = new Request("https://example.test/webhook", {
      headers: { "x-telegram-bot-api-secret-token": "wrong-secret" },
    });
    await expect(verifyTelegramWebhook(good, "correct-secret")).resolves.toBe(true);
    await expect(verifyTelegramWebhook(bad, "correct-secret")).resolves.toBe(false);
  });

  it.each(["sk_test_not_live", "rk_test_not_live", "pk_live_publishable"])(
    "refuses to query Stripe balance with non-live-secret key %s",
    async (stripeKey) => {
      const fetcher = vi.fn<typeof fetch>();
      await expect(getLiveStripeBalance(stripeKey, fetcher)).rejects.toThrow(
        "live Stripe key is unavailable",
      );
      expect(fetcher).not.toHaveBeenCalled();
    },
  );

  it.each(["sk_live_secret", "rk_live_restricted"])(
    "queries Stripe balance with supported live key %s",
    async (stripeKey) => {
      const fetcher = vi.fn<typeof fetch>(async () => Response.json({
        available: [{ amount: 500, currency: "usd" }],
        pending: [],
      }));
      await expect(getLiveStripeBalance(stripeKey, fetcher)).resolves.toMatchObject({
        available: [{ amount: 500, currency: "usd" }],
      });
      expect(fetcher).toHaveBeenCalledOnce();
      expect(fetcher.mock.calls[0]?.[1]?.headers).toEqual({
        authorization: `Bearer ${stripeKey}`,
      });
    },
  );

});
