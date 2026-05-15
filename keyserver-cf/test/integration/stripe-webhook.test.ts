import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  postSignedWebhook,
  signStripeWebhook,
  uniqueEventId,
  uniqueSubId,
} from "./helpers-stripe.js";

describe("POST /v1/stripe/webhook signature", () => {
  it("401s without a signature header", async () => {
    const res = await SELF.fetch("http://test/v1/stripe/webhook", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ id: "evt_x", type: "noop", data: { object: {} } }),
    });
    expect(res.status).toBe(401);
  });

  it("401s when signature is for a tampered body", async () => {
    const original = JSON.stringify({
      id: "evt_t",
      type: "noop",
      data: { object: {} },
    });
    const sig = await signStripeWebhook(original);
    const tampered = original.replace("noop", "Noop"); // body byte-changed
    const res = await SELF.fetch("http://test/v1/stripe/webhook", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "stripe-signature": sig,
      },
      body: tampered,
    });
    expect(res.status).toBe(401);
  });

  it("401s when timestamp is outside tolerance window", async () => {
    const body = JSON.stringify({
      id: "evt_old",
      type: "noop",
      data: { object: {} },
    });
    // Signed with a timestamp 10 minutes in the past — Stripe
    // tolerance is 5 min by default.
    const sig = await signStripeWebhook(
      body,
      "whsec_test_secret",
      Math.floor(Date.now() / 1000) - 600,
    );
    const res = await SELF.fetch("http://test/v1/stripe/webhook", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "stripe-signature": sig,
      },
      body,
    });
    expect(res.status).toBe(401);
  });

  it("200s a valid signature on an unhandled event type (noop)", async () => {
    const res = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "ping.unhandled",
      data: { object: {} },
    });
    expect(res.status).toBe(200);
    const j = (await res.json()) as { received: boolean; kind?: string };
    expect(j.received).toBe(true);
    expect(j.kind).toBe("noop");
  });
});

describe("POST /v1/stripe/webhook idempotency", () => {
  it("dedups identical event.id within retry window", async () => {
    const eventId = uniqueEventId();
    const event = {
      id: eventId,
      type: "ping.unhandled",
      data: { object: {} },
    };
    const r1 = await postSignedWebhook(SELF, event);
    const r2 = await postSignedWebhook(SELF, event);
    expect(r1.status).toBe(200);
    expect(r2.status).toBe(200);
    const j2 = (await r2.json()) as { received: boolean; deduped?: boolean };
    expect(j2.deduped).toBe(true);
  });

  it("processes distinct event.ids with the same body separately", async () => {
    const body = { type: "ping.unhandled", data: { object: {} } };
    const r1 = await postSignedWebhook(SELF, { id: uniqueEventId(), ...body });
    const r2 = await postSignedWebhook(SELF, { id: uniqueEventId(), ...body });
    expect(r1.status).toBe(200);
    expect(r2.status).toBe(200);
    const j1 = (await r1.json()) as { received: boolean; deduped?: boolean };
    const j2 = (await r2.json()) as { received: boolean; deduped?: boolean };
    expect(j1.deduped).toBeUndefined();
    expect(j2.deduped).toBeUndefined();
  });
});

describe("POST /v1/stripe/webhook state machine", () => {
  // F2.0 regression: prior to the readCurrentPeriodEnd fix, a
  // Stripe 2025-03-31+ payload (which puts current_period_end
  // under items.data[0]) left the D1 row with
  // current_period_end=null. License validate then returned
  // current_period_end:null even though the subscription was
  // ACTIVE. This test pins the new wire shape.
  it("customer.subscription.created with items.data[0].current_period_end (2025-03-31 shape) → period stored", async () => {
    const subId = uniqueSubId();
    await env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, 'cus_test', 'a@b.com', 'PENDING', NULL, 0,
         strftime('%s','now'), strftime('%s','now'))`,
    )
      .bind(subId)
      .run();

    const expectedPeriod = Math.floor(Date.now() / 1000) + 30 * 86400;
    const res = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "customer.subscription.created",
      data: {
        object: {
          id: subId,
          customer: "cus_test",
          status: "active",
          cancel_at_period_end: false,
          // NOTE: no top-level current_period_end — this matches
          // the Stripe 2025-03-31 API wire shape exactly.
          items: {
            data: [
              { current_period_end: expectedPeriod, current_period_start: expectedPeriod - 30 * 86400 },
            ],
          },
        },
      },
    });
    expect(res.status).toBe(200);

    const row = await env.DB.prepare(
      "SELECT status, current_period_end FROM subscriptions WHERE subscription_id = ?",
    )
      .bind(subId)
      .first<{ status: string; current_period_end: number | null }>();
    expect(row?.status).toBe("ACTIVE");
    expect(row?.current_period_end).toBe(expectedPeriod);
  });

  it("invoice.paid stamps current_period_end from lines.data[0].period.end (defence-in-depth)", async () => {
    // Simulates the worst-case ordering: customer.subscription.created
    // never landed (or was lost), but the first invoice.paid does. The
    // worker should stamp current_period_end from the invoice line
    // rather than leaving it null forever.
    const subId = uniqueSubId();
    await env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, 'cus_test', 'a@b.com', 'PENDING', NULL, 0,
         strftime('%s','now'), strftime('%s','now'))`,
    )
      .bind(subId)
      .run();

    const expectedPeriod = Math.floor(Date.now() / 1000) + 30 * 86400;
    const res = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "invoice.paid",
      data: {
        object: {
          id: "in_test",
          subscription: subId,
          lines: {
            data: [
              { period: { end: expectedPeriod, start: expectedPeriod - 30 * 86400 } },
            ],
          },
        },
      },
    });
    expect(res.status).toBe(200);

    const row = await env.DB.prepare(
      "SELECT status, current_period_end FROM subscriptions WHERE subscription_id = ?",
    )
      .bind(subId)
      .first<{ status: string; current_period_end: number | null }>();
    expect(row?.status).toBe("ACTIVE");
    expect(row?.current_period_end).toBe(expectedPeriod);
  });

  it("invoice.paid without a line period.end leaves the existing current_period_end intact", async () => {
    // Inverse of the previous test: don't clobber a good
    // current_period_end (stamped by an earlier subscription event)
    // when invoice.paid happens to lack the period field.
    const subId = uniqueSubId();
    const seeded = Math.floor(Date.now() / 1000) + 14 * 86400;
    await env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, 'cus_test', 'a@b.com', 'GRACE', ?, 0,
         strftime('%s','now'), strftime('%s','now'))`,
    )
      .bind(subId, seeded)
      .run();

    const res = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "invoice.paid",
      data: {
        object: {
          id: "in_test_no_lines",
          subscription: subId,
          // no `lines` field — older Stripe shape or test event
        },
      },
    });
    expect(res.status).toBe(200);

    const row = await env.DB.prepare(
      "SELECT status, current_period_end FROM subscriptions WHERE subscription_id = ?",
    )
      .bind(subId)
      .first<{ status: string; current_period_end: number | null }>();
    expect(row?.status).toBe("ACTIVE");
    expect(row?.current_period_end).toBe(seeded);
  });

  it("customer.subscription.created → ACTIVE in DB", async () => {
    const subId = uniqueSubId();
    // Bootstrap: create a PENDING row via a synthetic INSERT (the
    // checkout.completed path needs Resend wired up which we
    // exercise separately).
    await env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, 'cus_test', 'a@b.com', 'PENDING', NULL, 0,
         strftime('%s','now'), strftime('%s','now'))`,
    )
      .bind(subId)
      .run();

    const res = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "customer.subscription.created",
      data: {
        object: {
          id: subId,
          customer: "cus_test",
          status: "active",
          current_period_end: Math.floor(Date.now() / 1000) + 30 * 86400,
          cancel_at_period_end: false,
        },
      },
    });
    expect(res.status).toBe(200);

    const row = await env.DB.prepare(
      "SELECT status, current_period_end FROM subscriptions WHERE subscription_id = ?",
    )
      .bind(subId)
      .first<{ status: string; current_period_end: number }>();
    expect(row?.status).toBe("ACTIVE");
    expect(row?.current_period_end).toBeGreaterThan(Math.floor(Date.now() / 1000));
  });

  it("customer.subscription.updated with cancel_at_period_end → CANCELLED", async () => {
    const subId = uniqueSubId();
    await env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, 'cus_test', 'a@b.com', 'ACTIVE',
         strftime('%s','now') + 86400, 0,
         strftime('%s','now'), strftime('%s','now'))`,
    )
      .bind(subId)
      .run();

    const res = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "customer.subscription.updated",
      data: {
        object: {
          id: subId,
          customer: "cus_test",
          status: "active",
          cancel_at_period_end: true,
          current_period_end: Math.floor(Date.now() / 1000) + 86400,
        },
      },
    });
    expect(res.status).toBe(200);

    const row = await env.DB.prepare(
      "SELECT status FROM subscriptions WHERE subscription_id = ?",
    )
      .bind(subId)
      .first<{ status: string }>();
    expect(row?.status).toBe("CANCELLED");
  });

  it("invoice.payment_failed → GRACE; invoice.paid → ACTIVE", async () => {
    const subId = uniqueSubId();
    await env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, 'cus_test', 'a@b.com', 'ACTIVE',
         strftime('%s','now') + 86400, 0,
         strftime('%s','now'), strftime('%s','now'))`,
    )
      .bind(subId)
      .run();

    await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "invoice.payment_failed",
      data: { object: { id: "in_x", subscription: subId } },
    });
    let row = await env.DB.prepare(
      "SELECT status FROM subscriptions WHERE subscription_id = ?",
    )
      .bind(subId)
      .first<{ status: string }>();
    expect(row?.status).toBe("GRACE");

    await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "invoice.paid",
      data: { object: { id: "in_y", subscription: subId } },
    });
    row = await env.DB.prepare(
      "SELECT status FROM subscriptions WHERE subscription_id = ?",
    )
      .bind(subId)
      .first<{ status: string }>();
    expect(row?.status).toBe("ACTIVE");
  });

  it("charge.dispute.created with metadata.subscription_id → REVOKED + license revoked", async () => {
    const subId = uniqueSubId();
    await env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, 'cus_x', 'a@b.com', 'ACTIVE',
         strftime('%s','now') + 86400, 0,
         strftime('%s','now'), strftime('%s','now'))`,
    )
      .bind(subId)
      .run();
    await env.DB.prepare(
      `INSERT INTO licenses (license_hash, subscription_id, issued_at)
       VALUES ('hash-' || ?, ?, strftime('%s','now'))`,
    )
      .bind(subId, subId)
      .run();

    const res = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "charge.dispute.created",
      data: {
        object: {
          id: "dp_test",
          charge: "ch_test",
          metadata: { subscription_id: subId },
        },
      },
    });
    expect(res.status).toBe(200);

    const sub = await env.DB.prepare(
      "SELECT status FROM subscriptions WHERE subscription_id = ?",
    )
      .bind(subId)
      .first<{ status: string }>();
    expect(sub?.status).toBe("REVOKED");
    const lic = await env.DB.prepare(
      "SELECT revoked_reason FROM licenses WHERE subscription_id = ?",
    )
      .bind(subId)
      .first<{ revoked_reason: string }>();
    expect(lic?.revoked_reason).toBe("chargeback");
  });
});
