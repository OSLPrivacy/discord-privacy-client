import { SELF, env } from "cloudflare:test";
import { beforeAll, describe, expect, it } from "vitest";

const ADMIN_TOKEN = "test-admin-token-do-not-ship";

// Seed today's BTC + XMR price snapshots so /v1/crypto/quote can
// price the form. (The daily cron would normally do this; tests
// can't wait 24h for it.)
async function seedTodayPrice(asset: "btc" | "xmr", price: string): Promise<void> {
  const today = new Date().toISOString().slice(0, 10);
  await env.DB.prepare(
    `INSERT INTO crypto_price_snapshots (asset, snapshot_date, price_usd, fetched_at)
     VALUES (?, ?, ?, strftime('%s','now'))
     ON CONFLICT(asset, snapshot_date) DO UPDATE SET price_usd = excluded.price_usd`,
  )
    .bind(asset, today, price)
    .run();
}

beforeAll(async () => {
  await seedTodayPrice("btc", "60000");
  await seedTodayPrice("xmr", "150");
});

describe("crypto payment manual flow", () => {
  it("returns a quote with native amount derived from the snapshot price", async () => {
    const res = await SELF.fetch("http://test/v1/crypto/quote", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        plan: "monthly",
        payment_method: "btc",
        email: "user@example.com",
      }),
    });
    expect(res.status).toBe(200);
    const j = (await res.json()) as {
      payment_id: string;
      address: string;
      amount_native: string;
      amount_usd_cents: number;
      price_locked_at: string;
      expires_at: number;
    };
    expect(j.payment_id).toMatch(/^cpay_/);
    expect(j.address).toMatch(/^bc1q/);
    expect(j.amount_usd_cents).toBe(500);
    // $5 / $60000 = 0.0000833... → "0.00008333" at 8 dp.
    expect(j.amount_native).toBe("0.00008333");
    expect(j.expires_at).toBeGreaterThan(Math.floor(Date.now() / 1000));
  });

  it("400s with malformed body", async () => {
    const res = await SELF.fetch("http://test/v1/crypto/quote", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ plan: "weekly" }),
    });
    expect(res.status).toBe(400);
  });

  it("crypto/submit 404s on unknown payment_id", async () => {
    const res = await SELF.fetch("http://test/v1/crypto/submit", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ payment_id: "cpay_does_not_exist", txid: "abc123def" }),
    });
    expect(res.status).toBe(404);
  });

  it("admin/crypto/confirm 401s without admin token", async () => {
    const res = await SELF.fetch("http://test/v1/admin/crypto/confirm", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ payment_id: "cpay_x" }),
    });
    expect(res.status).toBe(401);
  });

  it("admin/crypto/confirm 404s for unknown payment_id", async () => {
    const res = await SELF.fetch("http://test/v1/admin/crypto/confirm", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${ADMIN_TOKEN}`,
      },
      body: JSON.stringify({ payment_id: "cpay_ghost" }),
    });
    expect(res.status).toBe(404);
  });

  it("end-to-end: insert quote → submit → admin confirm → license issued", async () => {
    // Insert a 'quoted' row directly (bypasses the env-gated quote
    // endpoint; same shape).
    const paymentId = `cpay_${crypto.randomUUID().replace(/-/g, "")}`;
    await env.DB.prepare(
      `INSERT INTO crypto_pending_payments
         (payment_id, payment_method, plan, amount_usd_cents, amount_native,
          address, customer_email, status, created_at)
       VALUES (?, 'btc', 'monthly', 500, '0.00833333',
         'bc1qxxx...', 'buyer@example.com', 'quoted',
         strftime('%s','now'))`,
    )
      .bind(paymentId)
      .run();

    const submit = await SELF.fetch("http://test/v1/crypto/submit", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        payment_id: paymentId,
        txid: "deadbeef".repeat(8),
      }),
    });
    expect(submit.status).toBe(200);
    const subJson = (await submit.json()) as { status: string };
    expect(subJson.status).toBe("awaiting");

    const confirm = await SELF.fetch("http://test/v1/admin/crypto/confirm", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${ADMIN_TOKEN}`,
      },
      body: JSON.stringify({ payment_id: paymentId }),
    });
    expect(confirm.status).toBe(200);
    const confirmJson = (await confirm.json()) as {
      ok: boolean;
      license_issued: boolean;
      email_sent: boolean;
      subscription_id: string;
    };
    expect(confirmJson.license_issued).toBe(true);
    expect(confirmJson.subscription_id).toBe(`crypto_${paymentId}`);

    // Verify DB state.
    const sub = await env.DB.prepare(
      "SELECT status FROM subscriptions WHERE subscription_id = ?",
    )
      .bind(confirmJson.subscription_id)
      .first<{ status: string }>();
    expect(sub?.status).toBe("ACTIVE");
    const lic = await env.DB.prepare(
      "SELECT COUNT(*) AS c FROM licenses WHERE subscription_id = ?",
    )
      .bind(confirmJson.subscription_id)
      .first<{ c: number }>();
    expect(lic?.c).toBe(1);
  });

  it("admin/crypto/confirm 409s a re-confirmation", async () => {
    const paymentId = `cpay_${crypto.randomUUID().replace(/-/g, "")}`;
    await env.DB.prepare(
      `INSERT INTO crypto_pending_payments
         (payment_id, payment_method, plan, amount_usd_cents, amount_native,
          address, customer_email, status, created_at)
       VALUES (?, 'btc', 'monthly', 500, '0.00833333',
         'bc1qxxx', 'buyer@example.com', 'awaiting',
         strftime('%s','now'))`,
    )
      .bind(paymentId)
      .run();

    const r1 = await SELF.fetch("http://test/v1/admin/crypto/confirm", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${ADMIN_TOKEN}`,
      },
      body: JSON.stringify({ payment_id: paymentId }),
    });
    expect(r1.status).toBe(200);

    const r2 = await SELF.fetch("http://test/v1/admin/crypto/confirm", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${ADMIN_TOKEN}`,
      },
      body: JSON.stringify({ payment_id: paymentId }),
    });
    expect(r2.status).toBe(409);
  });
});
