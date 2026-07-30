import { env } from "cloudflare:test";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Env } from "../../src/env.js";
import {
  drainPaymentAlertOutbox,
  ensurePaymentAlert,
  paymentAlertId,
  sweepDeliveredPaymentAlerts,
} from "../../src/lib/payment-alert-outbox.js";

const BOT_TOKEN = "1234567890:abcdefghijklmnopqrstuvwxyzABCDE";
const OPERATOR_CHAT_ID = "1122334455";

function configuredEnv(overrides: Partial<Env> = {}): Env {
  return {
    DB: env.DB,
    TELEGRAM_BOT_TOKEN: BOT_TOKEN,
    TELEGRAM_OPERATOR_CHAT_IDS: OPERATOR_CHAT_ID,
    ...overrides,
  } as Env;
}

function telegramFetcher(statuses: number[]): typeof fetch {
  let index = 0;
  return vi.fn(async () => {
    const status = statuses[Math.min(index, statuses.length - 1)] ?? 200;
    index += 1;
    return Response.json({ ok: status >= 200 && status < 300 }, { status });
  }) as unknown as typeof fetch;
}

describe("durable crypto payment alert outbox", () => {
  beforeEach(async () => {
    await env.DB.prepare("DELETE FROM payment_alert_outbox").run();
  });

  it("stores only aggregate-safe facts behind a one-way idempotency key", async () => {
    const sourceId = `cpay_${"a".repeat(32)}`;
    const alertId = await ensurePaymentAlert(
      env.DB,
      "crypto_pro",
      sourceId,
      "btc",
      500,
      1_000,
    );
    await ensurePaymentAlert(env.DB, "crypto_pro", sourceId, "btc", 500, 2_000);

    expect(alertId).toBe(await paymentAlertId("crypto_pro", sourceId));
    expect(alertId).not.toContain(sourceId);
    const rows = await env.DB.prepare(
      "SELECT * FROM payment_alert_outbox WHERE alert_id = ?",
    ).bind(alertId).all<Record<string, unknown>>();
    expect(rows.results).toHaveLength(1);
    expect(JSON.stringify(rows.results[0])).not.toContain(sourceId);
    expect(rows.results[0]).toMatchObject({
      alert_kind: "crypto_pro",
      payment_method: "btc",
      amount_usd_cents: 500,
      status: "pending",
      attempts: 0,
    });
  });

  it("retains an alert without attempting delivery when configuration is missing", async () => {
    const alertId = await ensurePaymentAlert(
      env.DB,
      "crypto_pro",
      `cpay_${"b".repeat(32)}`,
      "xmr",
      500,
      3_000,
    );
    const fetcher = telegramFetcher([200]);

    await expect(drainPaymentAlertOutbox(
      configuredEnv({ TELEGRAM_BOT_TOKEN: undefined }),
      fetcher,
      3_000,
    )).resolves.toEqual({ configured: false, attempted: 0, delivered: 0 });
    expect(fetcher).not.toHaveBeenCalled();
    const row = await env.DB.prepare(
      "SELECT status, attempts, next_attempt_at FROM payment_alert_outbox WHERE alert_id = ?",
    ).bind(alertId).first();
    expect(row).toEqual({ status: "pending", attempts: 0, next_attempt_at: 3_000 });
  });

  it("backs off after a transient failure and delivers exactly once on a due retry", async () => {
    const alertId = await ensurePaymentAlert(
      env.DB,
      "crypto_pro",
      `cpay_${"c".repeat(32)}`,
      "btc",
      500,
      4_000,
    );
    const failing = telegramFetcher([503]);
    await expect(drainPaymentAlertOutbox(
      configuredEnv(),
      failing,
      4_000,
    )).resolves.toEqual({ configured: true, attempted: 1, delivered: 0 });

    const afterFailure = await env.DB.prepare(
      "SELECT status, attempts, next_attempt_at FROM payment_alert_outbox WHERE alert_id = ?",
    ).bind(alertId).first<{ status: string; attempts: number; next_attempt_at: number }>();
    expect(afterFailure).toEqual({ status: "pending", attempts: 1, next_attempt_at: 4_030 });

    const succeeding = telegramFetcher([200]);
    await expect(drainPaymentAlertOutbox(
      configuredEnv(),
      succeeding,
      4_029,
    )).resolves.toEqual({ configured: true, attempted: 0, delivered: 0 });
    await expect(drainPaymentAlertOutbox(
      configuredEnv(),
      succeeding,
      4_030,
    )).resolves.toEqual({ configured: true, attempted: 1, delivered: 1 });
    await expect(drainPaymentAlertOutbox(
      configuredEnv(),
      succeeding,
      4_031,
    )).resolves.toEqual({ configured: true, attempted: 0, delivered: 0 });
    expect(succeeding).toHaveBeenCalledTimes(1);

    const delivered = await env.DB.prepare(
      "SELECT status, attempts, delivered_at FROM payment_alert_outbox WHERE alert_id = ?",
    ).bind(alertId).first();
    expect(delivered).toEqual({ status: "delivered", attempts: 2, delivered_at: 4_030 });
  });

  it("delivers donation alerts and removes only delivered rows after retention", async () => {
    const deliveredId = await ensurePaymentAlert(
      env.DB,
      "crypto_donation",
      `cdon_${"d".repeat(32)}`,
      "xmr",
      2_000,
      10_000,
    );
    const pendingId = await ensurePaymentAlert(
      env.DB,
      "crypto_pro",
      `cpay_${"e".repeat(32)}`,
      "btc",
      500,
      10_001,
    );
    const fetcher = telegramFetcher([200]);
    await expect(drainPaymentAlertOutbox(
      configuredEnv(),
      fetcher,
      10_000,
    )).resolves.toEqual({ configured: true, attempted: 1, delivered: 1 });

    const retention = 7 * 24 * 60 * 60;
    expect(await sweepDeliveredPaymentAlerts(env.DB, 10_000 + retention)).toBe(0);
    expect(await sweepDeliveredPaymentAlerts(env.DB, 10_001 + retention)).toBe(1);
    expect(await env.DB.prepare(
      "SELECT COUNT(*) AS count FROM payment_alert_outbox WHERE alert_id = ?",
    ).bind(deliveredId).first()).toEqual({ count: 0 });
    expect(await env.DB.prepare(
      "SELECT status FROM payment_alert_outbox WHERE alert_id = ?",
    ).bind(pendingId).first()).toEqual({ status: "pending" });
  });

  it("fails closed if a deterministic alert id is already bound to different terms", async () => {
    const sourceId = `cpay_${"f".repeat(32)}`;
    const alertId = await paymentAlertId("crypto_pro", sourceId);
    await env.DB.prepare(
      `INSERT INTO payment_alert_outbox
        (alert_id, alert_kind, payment_method, amount_usd_cents,
         status, attempts, next_attempt_at, created_at)
       VALUES (?, 'crypto_pro', 'btc', 500, 'pending', 0, 1, 1)`,
    ).bind(alertId).run();

    await expect(ensurePaymentAlert(
      env.DB,
      "crypto_pro",
      sourceId,
      "xmr",
      500,
      2,
    )).rejects.toThrow("payment alert idempotency conflict");
  });
});
