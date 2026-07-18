import { env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { sweepExpired } from "../../src/lib/subscriptions.js";
import { sweepExpiredPrivacyRows } from "../../src/lib/db.js";

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

describe("sweepExpiredPrivacyRows (hourly cron)", () => {
  it("physically deletes expired wrapped keys and receipts only", async () => {
    const owner = `retention-owner-${crypto.randomUUID()}`;
    const nowMs = Date.now();
    const nowSeconds = Math.floor(nowMs / 1000);
    const expiredContent = `expired-${crypto.randomUUID()}`;
    const freshContent = `fresh-${crypto.randomUUID()}`;
    const expiredIsoWithOffset = new Date(nowMs - 60_000)
      .toISOString()
      .replace("Z", "+00:00");
    await env.DB.batch([
      env.DB
        .prepare(
          `INSERT INTO wrapped_keys
             (content_id, content_type, sender_id, recipient_id, session_version,
              share_index, wrapped_share_blob, blob_version, single_use,
              expires_at, created_at)
           VALUES (?, 'text', ?, 'recipient', 1, 0, 'blob', 1, 0, ?, ?)`,
        )
        .bind(expiredContent, owner, expiredIsoWithOffset, new Date(nowMs).toISOString()),
      env.DB
        .prepare(
          `INSERT INTO wrapped_keys
             (content_id, content_type, sender_id, recipient_id, session_version,
              share_index, wrapped_share_blob, blob_version, single_use,
              expires_at, created_at)
           VALUES (?, 'text', ?, 'recipient', 1, 0, 'blob', 1, 0, ?, ?)`,
        )
        .bind(
          freshContent,
          owner,
          new Date(nowMs + 60_000).toISOString(),
          new Date(nowMs).toISOString(),
        ),
      env.DB
        .prepare(
          `INSERT INTO consuming_get_receipts
             (requester_id, request_digest, recipient_id, target_id, expires_at)
           VALUES (?, ?, 'recipient', 'expired', ?)`,
        )
        .bind(owner, new Uint8Array(32).fill(1), nowSeconds - 1),
      env.DB
        .prepare(
          `INSERT INTO consuming_get_receipts
             (requester_id, request_digest, recipient_id, target_id, expires_at)
           VALUES (?, ?, 'recipient', 'fresh', ?)`,
        )
        .bind(owner, new Uint8Array(32).fill(2), nowSeconds + 60),
      env.DB
        .prepare(
          `INSERT INTO wrapped_key_post_receipts
             (sender_id, request_digest, content_id, expires_at)
           VALUES (?, ?, 'expired-post', ?)`,
        )
        .bind(owner, new Uint8Array(32).fill(3), nowSeconds - 1),
    ]);

    expect(await sweepExpiredPrivacyRows(env.DB, nowMs)).toEqual({
      wrappedKeys: 1,
      consumingGetReceipts: 1,
      wrappedKeyPostReceipts: 1,
      prekeyReplenishReceipts: 0,
      wrappedKeyBurnReceipts: 0,
      unregisterReceipts: 0,
    });

    const wrapped = await env.DB.prepare(
      "SELECT content_id FROM wrapped_keys WHERE sender_id = ?",
    )
      .bind(owner)
      .all<{ content_id: string }>();
    expect(wrapped.results.map((row) => row.content_id)).toEqual([freshContent]);
    const receipts = await env.DB.prepare(
      "SELECT target_id FROM consuming_get_receipts WHERE requester_id = ?",
    )
      .bind(owner)
      .all<{ target_id: string }>();
    expect(receipts.results.map((row) => row.target_id)).toEqual(["fresh"]);
  });
});
