import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  postSignedWebhook,
  signStripeWebhook,
  uniqueEventId,
  uniqueSubId,
} from "./helpers-stripe.js";
import { sha256Hex } from "../../src/lib/crypto-watcher-auth.js";

function browserClaimToken(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

describe("POST /v1/stripe/webhook signature", () => {
  it("rejects a validly signed Stripe test mode event", async () => {
    const body = JSON.stringify({
      id: uniqueEventId(),
      type: "ping.unhandled",
      livemode: false,
      created: Math.floor(Date.now() / 1000),
      data: { object: {} },
    });
    const res = await SELF.fetch("http://test/v1/stripe/webhook", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "stripe-signature": await signStripeWebhook(body),
      },
      body,
    });
    expect(res.status).toBe(400);
  });

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
  it("does not let a replayed older event undo a later terminal event", async () => {
    const subId = uniqueSubId();
    await env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, 'cus_replay', 'replay@example.test', 'PENDING', NULL, 0,
         strftime('%s','now'), strftime('%s','now'))`,
    ).bind(subId).run();
    const createdEvent = {
      id: uniqueEventId(),
      type: "customer.subscription.created",
      data: {
        object: {
          id: subId,
          customer: "cus_replay",
          status: "active",
          current_period_end: Math.floor(Date.now() / 1000) + 3600,
        },
      },
    };
    expect((await postSignedWebhook(SELF, createdEvent)).status).toBe(200);
    expect((await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "customer.subscription.deleted",
      data: { object: { id: subId, customer: "cus_replay", status: "canceled" } },
    })).status).toBe(200);
    const replay = await postSignedWebhook(SELF, createdEvent);
    expect(replay.status).toBe(200);
    await expect(replay.json()).resolves.toMatchObject({ deduped: true });
    const row = await env.DB.prepare(
      "SELECT status FROM subscriptions WHERE subscription_id = ?",
    ).bind(subId).first<{ status: string }>();
    expect(row?.status).toBe("EXPIRED");
  });

  it("keeps only the latest cumulative refund amount for each charge", async () => {
    const chargeId = `ch_refund_${crypto.randomUUID().replace(/-/g, "")}`;
    expect((await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "charge.refunded",
      data: { object: { id: chargeId, amount_refunded: 200, currency: "usd" } },
    })).status).toBe(200);
    expect((await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "charge.refunded",
      data: { object: { id: chargeId, amount_refunded: 500, currency: "usd" } },
    })).status).toBe(200);
    const metric = await env.DB.prepare(
      `SELECT COUNT(*) AS count, SUM(amount_cents) AS cents
         FROM commerce_events
        WHERE event_type = 'charge.refunded' AND stripe_object_id = ?`,
    ).bind(chargeId).first<{ count: number; cents: number }>();
    expect(metric).toEqual({ count: 1, cents: -500 });
  });

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
  it("activates lifetime Pro from a paid one-time checkout without customer data", async () => {
    const sessionId = `cs_live_one_time_${crypto.randomUUID().replace(/-/g, "")}`;
    const paymentIntentId = `pi_${crypto.randomUUID().replace(/-/g, "")}`;
    const licenseHash = `license-${sessionId}`;
    const now = Math.floor(Date.now() / 1000);
    await env.DB.prepare(
      `INSERT INTO stripe_checkout_claims (
         session_id, claim_hash, delivery_public_key_spki,
         encrypted_license, license_hash, subscription_id, status,
         created_at, expires_at, delivered_at
       ) VALUES (?, ?, 'public-key', 'ciphertext', ?, NULL, 'pending', ?, ?, NULL)`,
    ).bind(sessionId, `claim-${sessionId}`, licenseHash, now, now + 3600).run();

    const response = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "checkout.session.completed",
      data: {
        object: {
          id: sessionId,
          mode: "payment",
          metadata: { osl_plan: "pro", osl_purchase: "one-time", osl_fulfillment: "instant-v1" },
          payment_status: "paid",
          payment_intent: paymentIntentId,
          amount_total: 500,
          currency: "usd",
        },
      },
    });
    expect(response.status).toBe(200);
    await expect(response.json()).resolves.toMatchObject({ kind: "applied" });

    const entitlement = await env.DB.prepare(
      `SELECT customer_id, customer_email, status, current_period_end,
              cancel_at_period_end
         FROM subscriptions WHERE subscription_id = ?`,
    ).bind(paymentIntentId).first<{
      customer_id: string;
      customer_email: string;
      status: string;
      current_period_end: number | null;
      cancel_at_period_end: number;
    }>();
    expect(entitlement).toEqual({
      customer_id: "",
      customer_email: "",
      status: "ACTIVE",
      current_period_end: null,
      cancel_at_period_end: 0,
    });
    const license = await env.DB.prepare(
      "SELECT subscription_id FROM licenses WHERE license_hash = ?",
    ).bind(licenseHash).first<{ subscription_id: string }>();
    expect(license?.subscription_id).toBe(paymentIntentId);
    const metric = await env.DB.prepare(
      `SELECT amount_cents FROM commerce_events
        WHERE event_type = 'checkout.session.completed' AND stripe_object_id = ?`,
    ).bind(sessionId).first<{ amount_cents: number }>();
    expect(metric?.amount_cents).toBe(500);
  });

  it.each([
    ["refund", "charge.refunded", "manual"],
    ["dispute", "charge.dispute.created", "chargeback"],
  ] as const)(
    "does not let a %s observed before delayed completion restore Pro",
    async (_kind, terminalType, revokedReason) => {
      const sessionId = `cs_live_terminal_first_${crypto.randomUUID().replace(/-/g, "")}`;
      const paymentIntentId = `pi_terminal_first_${crypto.randomUUID().replace(/-/g, "")}`;
      const licenseHash = `license-${sessionId}`;
      const claimToken = browserClaimToken();
      const now = Math.floor(Date.now() / 1000);
      await env.DB.prepare(
        `INSERT INTO stripe_checkout_claims (
           session_id, claim_hash, delivery_public_key_spki,
           encrypted_license, license_hash, subscription_id, status,
           created_at, expires_at, delivered_at
         ) VALUES (?, ?, 'public-key', 'ciphertext', ?, NULL, 'pending', ?, ?, NULL)`,
      ).bind(
        sessionId,
        await sha256Hex(claimToken),
        licenseHash,
        now,
        now + 3600,
      ).run();

      const terminalObject = terminalType === "charge.refunded"
        ? {
          id: `ch_${crypto.randomUUID().replace(/-/g, "")}`,
          payment_intent: paymentIntentId,
          amount: 500,
          amount_refunded: 500,
          currency: "usd",
        }
        : {
          id: `dp_${crypto.randomUUID().replace(/-/g, "")}`,
          payment_intent: paymentIntentId,
          charge: `ch_${crypto.randomUUID().replace(/-/g, "")}`,
          amount: 500,
          currency: "usd",
        };
      const terminal = await postSignedWebhook(SELF, {
        id: uniqueEventId(),
        type: terminalType,
        created: now,
        data: { object: terminalObject },
      });
      expect(terminal.status).toBe(200);

      const completion = await postSignedWebhook(SELF, {
        id: uniqueEventId(),
        type: "checkout.session.completed",
        created: now + 1,
        data: {
          object: {
            id: sessionId,
            mode: "payment",
            metadata: { osl_plan: "pro", osl_purchase: "one-time", osl_fulfillment: "instant-v1" },
            payment_status: "paid",
            payment_intent: paymentIntentId,
            amount_total: 500,
            currency: "usd",
          },
        },
      });
      expect(completion.status).toBe(200);

      const entitlement = await env.DB.prepare(
        "SELECT status FROM subscriptions WHERE subscription_id = ?",
      ).bind(paymentIntentId).first<{ status: string }>();
      expect(entitlement?.status).toBe("REVOKED");
      const license = await env.DB.prepare(
        `SELECT revoked_at, revoked_reason FROM licenses
          WHERE license_hash = ? AND subscription_id = ?`,
      ).bind(licenseHash, paymentIntentId).first<{
        revoked_at: number | null;
        revoked_reason: string | null;
      }>();
      expect(license?.revoked_at).not.toBeNull();
      expect(license?.revoked_reason).toBe(revokedReason);

      const claim = await SELF.fetch("http://test/v1/checkout/claim", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ session_id: sessionId, claim_token: claimToken }),
      });
      expect(claim.status).toBe(410);
      await expect(claim.json()).resolves.toMatchObject({
        error: "checkout claim expired",
      });
      const storedClaim = await env.DB.prepare(
        "SELECT status FROM stripe_checkout_claims WHERE session_id = ?",
      ).bind(sessionId).first<{ status: string }>();
      expect(storedClaim?.status).toBe("expired");
    },
  );

  it("keeps an unpaid one-time checkout pending and records no payment", async () => {
    const sessionId = `cs_live_unpaid_${crypto.randomUUID().replace(/-/g, "")}`;
    const paymentIntentId = `pi_${crypto.randomUUID().replace(/-/g, "")}`;
    const now = Math.floor(Date.now() / 1000);
    await env.DB.prepare(
      `INSERT INTO stripe_checkout_claims (
         session_id, claim_hash, delivery_public_key_spki,
         encrypted_license, license_hash, subscription_id, status,
         created_at, expires_at, delivered_at
       ) VALUES (?, ?, 'public-key', 'ciphertext', ?, NULL, 'pending', ?, ?, NULL)`,
    ).bind(
      sessionId,
      `claim-${sessionId}`,
      `license-${sessionId}`,
      now,
      now + 3600,
    ).run();

    const response = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "checkout.session.completed",
      data: {
        object: {
          id: sessionId,
          mode: "payment",
          metadata: { osl_plan: "pro", osl_purchase: "one-time", osl_fulfillment: "instant-v1" },
          payment_status: "unpaid",
          payment_intent: paymentIntentId,
          amount_total: 500,
          currency: "usd",
        },
      },
    });
    expect(response.status).toBe(200);
    await expect(response.json()).resolves.toMatchObject({ kind: "noop" });
    const claim = await env.DB.prepare(
      "SELECT status FROM stripe_checkout_claims WHERE session_id = ?",
    ).bind(sessionId).first<{ status: string }>();
    expect(claim?.status).toBe("pending");
    expect(await env.DB.prepare(
      "SELECT 1 AS present FROM subscriptions WHERE subscription_id = ?",
    ).bind(paymentIntentId).first()).toBeNull();
    expect(await env.DB.prepare(
      "SELECT 1 AS present FROM commerce_events WHERE stripe_object_id = ?",
    ).bind(sessionId).first()).toBeNull();
  });

  it("does not activate a paid checkout for the wrong amount", async () => {
    const sessionId = `cs_live_wrong_amount_${crypto.randomUUID().replace(/-/g, "")}`;
    const paymentIntentId = `pi_${crypto.randomUUID().replace(/-/g, "")}`;
    const now = Math.floor(Date.now() / 1000);
    await env.DB.prepare(
      `INSERT INTO stripe_checkout_claims (
         session_id, claim_hash, delivery_public_key_spki,
         encrypted_license, license_hash, subscription_id, status,
         created_at, expires_at, delivered_at
       ) VALUES (?, ?, 'public-key', 'ciphertext', ?, NULL, 'pending', ?, ?, NULL)`,
    ).bind(
      sessionId,
      `claim-${sessionId}`,
      `license-${sessionId}`,
      now,
      now + 3600,
    ).run();

    const response = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "checkout.session.completed",
      data: {
        object: {
          id: sessionId,
          mode: "payment",
          metadata: { osl_plan: "pro", osl_purchase: "one-time", osl_fulfillment: "instant-v1" },
          payment_status: "paid",
          payment_intent: paymentIntentId,
          amount_total: 600,
          currency: "usd",
        },
      },
    });
    expect(response.status).toBe(200);
    await expect(response.json()).resolves.toMatchObject({ kind: "noop" });
    const claim = await env.DB.prepare(
      "SELECT status FROM stripe_checkout_claims WHERE session_id = ?",
    ).bind(sessionId).first<{ status: string }>();
    expect(claim?.status).toBe("pending");
    expect(await env.DB.prepare(
      "SELECT 1 AS present FROM subscriptions WHERE subscription_id = ?",
    ).bind(paymentIntentId).first()).toBeNull();
  });

  it("applies an invoice paid observation that arrived before checkout completion", async () => {
    const subId = uniqueSubId();
    const sessionId = `cs_live_order_${crypto.randomUUID().replace(/-/g, "")}`;
    const now = Math.floor(Date.now() / 1000);
    await env.DB.prepare(
      `INSERT INTO stripe_checkout_claims (
         session_id, claim_hash, delivery_public_key_spki,
         encrypted_license, license_hash, subscription_id, status,
         created_at, expires_at, delivered_at
       ) VALUES (?, ?, 'public-key', 'ciphertext', ?, NULL, 'pending', ?, ?, NULL)`,
    ).bind(
      sessionId,
      `claim-${sessionId}`,
      `license-${sessionId}`,
      now,
      now + 3600,
    ).run();
    const periodEnd = now + 30 * 86400;
    expect((await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "invoice.paid",
      created: now,
      data: {
        object: {
          id: `in_${crypto.randomUUID().replace(/-/g, "")}`,
          customer: "cus_out_of_order",
          subscription: subId,
          amount_paid: 500,
          currency: "usd",
          lines: { data: [{ period: { end: periodEnd } }] },
        },
      },
    })).status).toBe(200);
    expect(await env.DB.prepare(
      "SELECT status FROM subscriptions WHERE subscription_id = ?",
    ).bind(subId).first()).toBeNull();

    expect((await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "checkout.session.completed",
      created: now + 1,
      data: {
        object: {
          id: sessionId,
          customer: "cus_out_of_order",
          customer_details: { email: "order@example.test" },
          subscription: subId,
          mode: "subscription",
        },
      },
    })).status).toBe(200);
    const row = await env.DB.prepare(
      "SELECT status, current_period_end FROM subscriptions WHERE subscription_id = ?",
    ).bind(subId).first<{ status: string; current_period_end: number }>();
    expect(row).toEqual({ status: "ACTIVE", current_period_end: periodEnd });
  });

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

  it("revokes a one-time entitlement by PaymentIntent when disputed", async () => {
    const paymentIntentId = `pi_dispute_${crypto.randomUUID().replace(/-/g, "")}`;
    await env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, '', '', 'ACTIVE', NULL, 0,
         strftime('%s','now'), strftime('%s','now'))`,
    ).bind(paymentIntentId).run();
    await env.DB.prepare(
      `INSERT INTO licenses (license_hash, subscription_id, issued_at)
       VALUES ('hash-' || ?, ?, strftime('%s','now'))`,
    ).bind(paymentIntentId, paymentIntentId).run();

    const response = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "charge.dispute.created",
      data: {
        object: {
          id: `dp_${crypto.randomUUID().replace(/-/g, "")}`,
          payment_intent: paymentIntentId,
          charge: `ch_${crypto.randomUUID().replace(/-/g, "")}`,
          amount: 500,
          currency: "usd",
        },
      },
    });
    expect(response.status).toBe(200);
    const entitlement = await env.DB.prepare(
      "SELECT status FROM subscriptions WHERE subscription_id = ?",
    ).bind(paymentIntentId).first<{ status: string }>();
    expect(entitlement?.status).toBe("REVOKED");
    const license = await env.DB.prepare(
      "SELECT revoked_reason FROM licenses WHERE subscription_id = ?",
    ).bind(paymentIntentId).first<{ revoked_reason: string }>();
    expect(license?.revoked_reason).toBe("chargeback");
  });

  it.each([
    ["full", 500],
    ["partial", 100],
  ])("revokes a one-time entitlement after a %s refund", async (_kind, refunded) => {
    const paymentIntentId = `pi_refund_${crypto.randomUUID().replace(/-/g, "")}`;
    const sessionId = `cs_live_refund_${crypto.randomUUID().replace(/-/g, "")}`;
    const claimToken = browserClaimToken();
    const now = Math.floor(Date.now() / 1000);
    await env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, '', '', 'ACTIVE', NULL, 0,
         strftime('%s','now'), strftime('%s','now'))`,
    ).bind(paymentIntentId).run();
    await env.DB.prepare(
      `INSERT INTO licenses (license_hash, subscription_id, issued_at)
       VALUES ('hash-' || ?, ?, strftime('%s','now'))`,
    ).bind(paymentIntentId, paymentIntentId).run();
    await env.DB.prepare(
      `INSERT INTO stripe_checkout_claims (
         session_id, claim_hash, delivery_public_key_spki,
         encrypted_license, license_hash, subscription_id, status,
         created_at, expires_at, delivered_at
       ) VALUES (?, ?, 'public-key', 'ciphertext', ?, ?, 'delivery_ready', ?, ?, ?)`,
    ).bind(
      sessionId,
      await sha256Hex(claimToken),
      `hash-${paymentIntentId}`,
      paymentIntentId,
      now,
      now + 3600,
      now,
    ).run();

    const response = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "charge.refunded",
      data: {
        object: {
          id: `ch_${crypto.randomUUID().replace(/-/g, "")}`,
          payment_intent: paymentIntentId,
          amount: 500,
          amount_refunded: refunded,
          currency: "usd",
        },
      },
    });
    expect(response.status).toBe(200);
    await expect(response.json()).resolves.toMatchObject({ kind: "applied" });
    const entitlement = await env.DB.prepare(
      "SELECT status FROM subscriptions WHERE subscription_id = ?",
    ).bind(paymentIntentId).first<{ status: string }>();
    expect(entitlement?.status).toBe("REVOKED");
    const license = await env.DB.prepare(
      "SELECT revoked_reason FROM licenses WHERE subscription_id = ?",
    ).bind(paymentIntentId).first<{ revoked_reason: string }>();
    expect(license?.revoked_reason).toBe("manual");
    const deliveredClaim = await SELF.fetch("http://test/v1/checkout/claim", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ session_id: sessionId, claim_token: claimToken }),
    });
    expect(deliveredClaim.status).toBe(200);
    await expect(deliveredClaim.json()).resolves.toMatchObject({
      status: "delivery_ready",
      encrypted_license: "ciphertext",
    });
  });
});
