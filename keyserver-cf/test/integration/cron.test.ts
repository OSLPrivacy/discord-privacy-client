import { env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { sweepExpired } from "../../src/lib/subscriptions.js";

describe("sweepExpired (hourly cron)", () => {
  it("promotes CANCELLED rows past current_period_end to EXPIRED", async () => {
    const subId = `sub_cancelled_${crypto.randomUUID().slice(0, 8)}`;
    const oneHourAgo = Math.floor(Date.now() / 1000) - 3600;
    await env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, 'cus_x', 'a@b.com', 'CANCELLED', ?, 1, ?, ?)`,
    )
      .bind(subId, oneHourAgo, oneHourAgo - 86400, oneHourAgo - 86400)
      .run();

    const promoted = await sweepExpired(env.DB);
    expect(promoted).toBeGreaterThanOrEqual(1);

    const row = await env.DB.prepare(
      "SELECT status FROM subscriptions WHERE subscription_id = ?",
    )
      .bind(subId)
      .first<{ status: string }>();
    expect(row?.status).toBe("EXPIRED");
  });

  it("promotes GRACE rows past current_period_end too", async () => {
    const subId = `sub_grace_${crypto.randomUUID().slice(0, 8)}`;
    const oneHourAgo = Math.floor(Date.now() / 1000) - 3600;
    await env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, 'cus_x', 'a@b.com', 'GRACE', ?, 0, ?, ?)`,
    )
      .bind(subId, oneHourAgo, oneHourAgo - 86400, oneHourAgo - 86400)
      .run();

    await sweepExpired(env.DB);
    const row = await env.DB.prepare(
      "SELECT status FROM subscriptions WHERE subscription_id = ?",
    )
      .bind(subId)
      .first<{ status: string }>();
    expect(row?.status).toBe("EXPIRED");
  });

  it("leaves ACTIVE rows untouched even past current_period_end", async () => {
    // ACTIVE rows past period_end shouldn't happen in practice
    // (the customer.subscription.updated event would transition
    // them), but the sweep should respect the filter regardless.
    const subId = `sub_active_${crypto.randomUUID().slice(0, 8)}`;
    const oneHourAgo = Math.floor(Date.now() / 1000) - 3600;
    await env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, 'cus_x', 'a@b.com', 'ACTIVE', ?, 0, ?, ?)`,
    )
      .bind(subId, oneHourAgo, oneHourAgo - 86400, oneHourAgo - 86400)
      .run();

    await sweepExpired(env.DB);
    const row = await env.DB.prepare(
      "SELECT status FROM subscriptions WHERE subscription_id = ?",
    )
      .bind(subId)
      .first<{ status: string }>();
    expect(row?.status).toBe("ACTIVE");
  });

  it("is idempotent (re-running is a no-op on already-EXPIRED rows)", async () => {
    const subId = `sub_iter_${crypto.randomUUID().slice(0, 8)}`;
    const oneHourAgo = Math.floor(Date.now() / 1000) - 3600;
    await env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, 'cus_x', 'a@b.com', 'CANCELLED', ?, 1, ?, ?)`,
    )
      .bind(subId, oneHourAgo, oneHourAgo - 86400, oneHourAgo - 86400)
      .run();
    const first = await sweepExpired(env.DB);
    const second = await sweepExpired(env.DB);
    expect(first).toBeGreaterThan(0);
    expect(second).toBe(0);
  });
});
