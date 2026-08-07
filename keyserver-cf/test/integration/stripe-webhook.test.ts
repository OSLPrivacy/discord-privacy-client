import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  postSignedWebhook,
  signStripeWebhook,
  uniqueEventId,
  uniqueSubId,
} from "./helpers-stripe.js";
import { sha256Hex } from "../../src/lib/crypto-watcher-auth.js";
import {
  ONE_TIME_PRO_AMOUNT_CENTS,
  ONE_TIME_PRO_AMOUNT_REFUSAL,
  ONE_TIME_PRO_CURRENCY,
  ONE_TIME_PRO_CURRENCY_REFUSAL,
} from "../../src/lib/subscription-state.js";
import { repairPaidOneTimeCheckoutClaimsWithoutCodes } from "../../src/lib/stripe-checkout-claims.js";
import { generateLicenseKey } from "../../src/lib/license.js";

function browserClaimToken(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

async function insertPendingCheckoutClaim(sessionId: string, licenseHash: string): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  await env.DB.prepare(
    `INSERT INTO stripe_checkout_claims (
       session_id, claim_hash, delivery_public_key_spki,
       encrypted_license, license_hash, subscription_id, status,
       created_at, expires_at, delivered_at
     ) VALUES (?, ?, 'public-key', 'ciphertext', ?, NULL, 'pending', ?, ?, NULL)`,
  ).bind(sessionId, `claim-${sessionId}`, licenseHash, now, now + 3600).run();
async function paymentWriteCounts(): Promise<Record<string, number>> {
  const tables = [
    "stripe_event_claims",
    "stripe_events",
    "subscriptions",
    "licenses",
    "stripe_subscription_observations",
    "stripe_checkout_claims",
    "commerce_events",
    "donation_events",
    "payment_alert_outbox",
  ];
  const counts: Record<string, number> = {};
  for (const table of tables) {
    const row = await env.DB.prepare(`SELECT COUNT(*) AS count FROM ${table}`)
      .first<{ count: number }>();
    counts[table] = row?.count ?? 0;
  }
  return counts;
}

async function stripeEventMarkerCounts(eventId: string): Promise<{
  completed: number;
  claims: number;
}> {
  const row = await env.DB.prepare(
    `SELECT
       (SELECT COUNT(*) FROM stripe_events WHERE event_id = ?) AS completed,
       (SELECT COUNT(*) FROM stripe_event_claims WHERE event_id = ?) AS claims`,
  ).bind(eventId, eventId).first<{ completed: number; claims: number }>();
  return {
    completed: row?.completed ?? 0,
    claims: row?.claims ?? 0,
  };
}

describe("POST /v1/stripe/webhook signature", () => {
  it("TASK 3189 accepts only Stripe-signed payment callbacks before writes", async () => {
    const signedEventId = `evt_task3189_signed_${crypto.randomUUID().replace(/-/g, "")}a`;
    const tamperedEventId = `${signedEventId.slice(0, -1)}b`;
    const unsignedEventId = `evt_task3189_unsigned_${crypto.randomUUID().replace(/-/g, "")}`;
    const signedBody = JSON.stringify({
      id: signedEventId,
      type: "ping.unhandled",
      livemode: true,
      created: Math.floor(Date.now() / 1000),
      data: { object: {} },
    });
    const tamperedBody = signedBody.replace(signedEventId, tamperedEventId);
    expect(tamperedBody.length).toBe(signedBody.length);
    expect(
      [...signedBody].filter((char, index) => char !== tamperedBody[index]),
    ).toHaveLength(1);
    const signature = await signStripeWebhook(signedBody);

    const accepted = await SELF.fetch("http://test/v1/stripe/webhook", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "stripe-signature": signature,
      },
      body: signedBody,
    });
    const acceptedBody = (await accepted.json()) as { received: boolean; kind?: string };
    expect(accepted.status).toBe(200);
    expect(acceptedBody).toMatchObject({ received: true, kind: "noop" });
    expect(await stripeEventMarkerCounts(signedEventId)).toEqual({
      completed: 1,
      claims: 1,
    });

    const afterAccepted = await paymentWriteCounts();
    const tampered = await SELF.fetch("http://test/v1/stripe/webhook", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "stripe-signature": signature,
      },
      body: tamperedBody,
    });
    const tamperedBodyJson = (await tampered.json()) as { error?: string };
    expect(tampered.status).toBe(401);
    expect(tamperedBodyJson.error).toBe("bad-signature");
    expect(await paymentWriteCounts()).toEqual(afterAccepted);
    expect(await stripeEventMarkerCounts(tamperedEventId)).toEqual({
      completed: 0,
      claims: 0,
    });

    const unsignedBody = JSON.stringify({
      id: unsignedEventId,
      type: "ping.unhandled",
      livemode: true,
      created: Math.floor(Date.now() / 1000),
      data: { object: {} },
    });
    const unsigned = await SELF.fetch("http://test/v1/stripe/webhook", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: unsignedBody,
    });
    const unsignedBodyJson = (await unsigned.json()) as { error?: string };
    expect(unsigned.status).toBe(401);
    expect(unsignedBodyJson.error).toBe("bad-signature");
    expect(await paymentWriteCounts()).toEqual(afterAccepted);
    expect(await stripeEventMarkerCounts(unsignedEventId)).toEqual({
      completed: 0,
      claims: 0,
    });

    console.log(
      `TASK3189 signed_status=${accepted.status} signed_received=${acceptedBody.received} signed_kind=${acceptedBody.kind}`,
    );
    console.log(
      `TASK3189 signed_event_completed=1 signed_event_claims=1`,
    );
    console.log(
      `TASK3189 tampered_changed_characters=1 tampered_status=${tampered.status} tampered_error=${tamperedBodyJson.error}`,
    );
    console.log(
      `TASK3189 tampered_event_completed=0 tampered_event_claims=0`,
    );
    console.log(
      `TASK3189 unsigned_status=${unsigned.status} unsigned_error=${unsignedBodyJson.error}`,
    );
    console.log(
      `TASK3189 unsigned_event_completed=0 unsigned_event_claims=0`,
    );
  });

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
  it("503s paid fulfillment without consuming the retry or writing entitlement state", async () => {
    const eventId = uniqueEventId();
    const sessionId = `cs_live_gated_${crypto.randomUUID().replace(/-/g, "")}`;
    const paymentIntentId = `pi_gated_${crypto.randomUUID().replace(/-/g, "")}`;
    const licenseHash = `license-${sessionId}`;
    const now = Math.floor(Date.now() / 1000);
    await env.DB.prepare(
      `INSERT INTO stripe_checkout_claims (
         session_id, claim_hash, delivery_public_key_spki,
         encrypted_license, license_hash, subscription_id, status,
         created_at, expires_at, delivered_at
       ) VALUES (?, ?, 'public-key', 'ciphertext', ?, NULL, 'pending', ?, ?, NULL)`,
    ).bind(sessionId, `claim-${sessionId}`, licenseHash, now, now + 3600).run();

    const body = JSON.stringify({
      id: eventId,
      type: "checkout.session.completed",
      livemode: true,
      created: now,
      data: {
        object: {
          id: sessionId,
          mode: "payment",
          metadata: {
            osl_plan: "pro",
            osl_purchase: "one-time",
            osl_fulfillment: "instant-v1",
          },
          payment_status: "paid",
          payment_intent: paymentIntentId,
          amount_total: 500,
          currency: "usd",
        },
      },
    });
    const response = await SELF.fetch("http://test/v1/stripe/webhook", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "stripe-signature": await signStripeWebhook(body),
      },
      body,
    });

    expect(response.status).toBe(503);
    await expect(response.json()).resolves.toMatchObject({
      error: "paid checkout is unavailable until prepaid-code redemption is ready",
    });
    const state = await env.DB.prepare(
      `SELECT
        (SELECT status FROM stripe_checkout_claims WHERE session_id = ?) AS claim_status,
        (SELECT COUNT(*) FROM stripe_event_claims WHERE event_id = ?) AS event_claims,
        (SELECT COUNT(*) FROM stripe_events WHERE event_id = ?) AS completed_events,
        (SELECT COUNT(*) FROM subscriptions WHERE subscription_id = ?) AS subscriptions,
        (SELECT COUNT(*) FROM licenses WHERE license_hash = ?) AS licenses`,
    ).bind(
      sessionId,
      eventId,
      eventId,
      paymentIntentId,
      licenseHash,
    ).first<{
      claim_status: string;
      event_claims: number;
      completed_events: number;
      subscriptions: number;
      licenses: number;
    }>();
    expect(state).toEqual({
      claim_status: "pending",
      event_claims: 0,
      completed_events: 0,
      subscriptions: 0,
      licenses: 0,
    });
  });

  it("T16-T05 issues an unredeemed one-month code from a paid one-time checkout", async () => {
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

    const license = await env.DB.prepare(
      "SELECT subscription_id, redeemed_at, expires_at, grant_seconds FROM licenses WHERE license_hash = ?",
    ).bind(licenseHash).first<{
      subscription_id: string;
      redeemed_at: number | null;
      expires_at: number | null;
      grant_seconds: number | null;
    }>();
    expect(license).toEqual({
      subscription_id: expect.stringMatching(/^lic_/),
      redeemed_at: null,
      expires_at: null,
      grant_seconds: 30 * 24 * 60 * 60,
    });
    expect(license?.subscription_id).not.toBe(paymentIntentId);

    const entitlement = await env.DB.prepare(
      `SELECT customer_id, customer_email, status, current_period_end,
              cancel_at_period_end
         FROM subscriptions WHERE subscription_id = ?`,
    ).bind(license?.subscription_id).first<{
      customer_id: string;
      customer_email: string;
      status: string;
      current_period_end: number | null;
      cancel_at_period_end: number;
    }>();
    expect(entitlement).toEqual({
      customer_id: "",
      customer_email: "",
      status: "PENDING",
      current_period_end: null,
      cancel_at_period_end: 0,
    });
    const metric = await env.DB.prepare(
      `SELECT amount_cents FROM commerce_events
        WHERE event_type = 'checkout.session.completed' AND stripe_object_id = ?`,
    ).bind(sessionId).first<{ amount_cents: number }>();
    expect(metric?.amount_cents).toBe(500);
  });

  it("TASK1503 creates exactly one unused one-month Pro code only after confirmed fixture payment", async () => {
    const confirmedSessionId = `cs_live_task1503_confirmed_${crypto.randomUUID().replace(/-/g, "")}`;
    const confirmedPaymentIntentId = `pi_task1503_confirmed_${crypto.randomUUID().replace(/-/g, "")}`;
    const confirmedLicenseHash = `license-task1503-confirmed-${confirmedSessionId}`;
    const unconfirmedSessionId = `cs_live_task1503_unconfirmed_${crypto.randomUUID().replace(/-/g, "")}`;
    const unconfirmedPaymentIntentId = `pi_task1503_unconfirmed_${crypto.randomUUID().replace(/-/g, "")}`;
    const unconfirmedLicenseHash = `license-task1503-unconfirmed-${unconfirmedSessionId}`;
    const now = Math.floor(Date.now() / 1000);
    for (const [sessionId, licenseHash] of [
      [confirmedSessionId, confirmedLicenseHash],
      [unconfirmedSessionId, unconfirmedLicenseHash],
    ] as const) {
  it("TASK3198 repairs paid one-time checkout claims without codes exactly once", async () => {
    const prefix = `task3198_${crypto.randomUUID().replace(/-/g, "")}`;
    const now = Math.floor(Date.now() / 1000);
    const sessionIds: string[] = [];
    const licenseHashes: string[] = [];
    for (let i = 0; i < 3; i += 1) {
      const sessionId = `cs_live_${prefix}_${i}`;
      const licenseHash = `${prefix}_license_${i}`;
      sessionIds.push(sessionId);
      licenseHashes.push(licenseHash);
      await env.DB.prepare(
        `INSERT INTO stripe_checkout_claims (
           session_id, claim_hash, delivery_public_key_spki,
           encrypted_license, license_hash, subscription_id, status,
           created_at, expires_at, delivered_at
         ) VALUES (?, ?, 'public-key', 'ciphertext', ?, NULL, 'pending', ?, ?, NULL)`,
      ).bind(sessionId, `claim-${sessionId}`, licenseHash, now, now + 3600).run();
    }

    const confirmed = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "checkout.session.completed",
      data: {
        object: {
          id: confirmedSessionId,
          mode: "payment",
          metadata: { osl_plan: "pro", osl_purchase: "one-time", osl_fulfillment: "instant-v1" },
          payment_status: "paid",
          payment_intent: confirmedPaymentIntentId,
          amount_total: 500,
          currency: "usd",
        },
      },
    });
    expect(confirmed.status).toBe(200);
    await expect(confirmed.json()).resolves.toMatchObject({ kind: "applied" });

    const unconfirmed = await postSignedWebhook(SELF, {
      id: uniqueEventId(),
      type: "checkout.session.completed",
      data: {
        object: {
          id: unconfirmedSessionId,
          mode: "payment",
          metadata: { osl_plan: "pro", osl_purchase: "one-time", osl_fulfillment: "instant-v1" },
          payment_status: "unpaid",
          payment_intent: unconfirmedPaymentIntentId,
          amount_total: 500,
          currency: "usd",
        },
      },
    });
    expect(unconfirmed.status).toBe(200);
    await expect(unconfirmed.json()).resolves.toMatchObject({ kind: "noop" });

    const counts = await env.DB.prepare(
      `SELECT
         (SELECT COUNT(*)
            FROM licenses
           WHERE license_hash = ?
             AND grant_seconds = ?
             AND redeemed_at IS NULL
             AND expires_at IS NULL) AS confirmed_codes,
         (SELECT COUNT(*)
            FROM licenses
           WHERE license_hash = ?) AS unconfirmed_codes`,
    ).bind(
      confirmedLicenseHash,
      30 * 24 * 60 * 60,
      unconfirmedLicenseHash,
    ).first<{
      confirmed_codes: number;
      unconfirmed_codes: number;
    }>();
    expect(counts).toEqual({
      confirmed_codes: 1,
      unconfirmed_codes: 0,
    });

    const confirmedClaim = await env.DB.prepare(
      "SELECT status FROM stripe_checkout_claims WHERE session_id = ?",
    ).bind(confirmedSessionId).first<{ status: string }>();
    const unconfirmedClaim = await env.DB.prepare(
      "SELECT status FROM stripe_checkout_claims WHERE session_id = ?",
    ).bind(unconfirmedSessionId).first<{ status: string }>();
    expect(confirmedClaim?.status).toBe("delivery_ready");
    expect(unconfirmedClaim?.status).toBe("pending");
    console.info(
      `TASK1503 confirmed_fixture_payment=paid confirmed_codes=${counts?.confirmed_codes} ` +
        `confirmed_claim_status=${confirmedClaim?.status} ` +
        `unconfirmed_fixture_payment=unpaid unconfirmed_codes=${counts?.unconfirmed_codes} ` +
        `unconfirmed_claim_status=${unconfirmedClaim?.status}`,
    );
         ) VALUES (?, ?, 'public-key', 'ciphertext', ?, ?, 'delivery_ready', ?, ?, NULL)`,
      ).bind(
        sessionId,
        `claim-${prefix}-${i}`,
        licenseHash,
        `pi_${prefix}_${i}`,
        now,
        now + 3600,
      ).run();
    }

    const codeCount = async () => {
      const row = await env.DB.prepare(
        `SELECT COUNT(*) AS count
           FROM licenses
          WHERE license_hash IN (?, ?, ?)`,
      ).bind(...licenseHashes).first<{ count: number }>();
      return row?.count ?? 0;
    };
    const joinedRows = async () => await env.DB.prepare(
      `SELECT
         stripe_checkout_claims.session_id,
         stripe_checkout_claims.subscription_id AS payment_intent_id,
         licenses.license_hash,
         licenses.subscription_id AS entitlement_id
       FROM licenses
       JOIN stripe_checkout_claims
         ON stripe_checkout_claims.license_hash = licenses.license_hash
      WHERE stripe_checkout_claims.session_id IN (?, ?, ?)
      ORDER BY stripe_checkout_claims.session_id`,
    ).bind(...sessionIds).all<{
      session_id: string;
      payment_intent_id: string;
      license_hash: string;
      entitlement_id: string;
    }>();

    const before = await codeCount();
    const firstRepair = await repairPaidOneTimeCheckoutClaimsWithoutCodes(env.DB);
    const afterFirst = await codeCount();
    const secondRepair = await repairPaidOneTimeCheckoutClaimsWithoutCodes(env.DB);
    const afterSecond = await codeCount();
    const joined = (await joinedRows()).results ?? [];
    const distinctPaidRecords = new Set(joined.map((row) => row.session_id)).size;

    expect(before).toBe(0);
    expect(firstRepair).toBe(3);
    expect(afterFirst).toBe(3);
    expect(secondRepair).toBe(0);
    expect(afterSecond).toBe(3);
    expect(joined).toHaveLength(3);
    expect(distinctPaidRecords).toBe(3);
    expect(new Set(joined.map((row) => row.license_hash)).size).toBe(3);
    for (const row of joined) {
      expect(row.payment_intent_id).toMatch(/^pi_/);
      expect(row.entitlement_id).toBe(`lic_${row.license_hash}`);
      expect(row.entitlement_id).not.toBe(row.payment_intent_id);
    }

    console.log(`TASK3198 code_count_before=${before}`);
    console.log(`TASK3198 repair_first=${firstRepair}`);
    console.log(`TASK3198 code_count_after_first=${afterFirst}`);
    console.log(`TASK3198 repair_second=${secondRepair}`);
    console.log(`TASK3198 code_count_after_second=${afterSecond}`);
    console.log(`TASK3198 distinct_paid_records_joined=${distinctPaidRecords}`);
    console.log("TASK3198 every_code_joined_to_different_paid_record=true");
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

      // P-44: the subscription is keyed by the licence, not by the Stripe payment
      // identifier. `licenseSubscriptionId` (stripe-checkout-claims.ts:252) returns
      // `lic_<licenseHash>`; binding paymentIntentId here selects a row that no
      // longer exists and the assertion reads `undefined`, not "not revoked".
      const entitlement = await env.DB.prepare(
        "SELECT status FROM subscriptions WHERE subscription_id = ?",
      ).bind(`lic_${licenseHash}`).first<{ status: string }>();
      expect(entitlement?.status).toBe("REVOKED");
      const license = await env.DB.prepare(
        `SELECT revoked_at, revoked_reason FROM licenses
          WHERE license_hash = ? AND subscription_id = ?`,
      ).bind(licenseHash, `lic_${licenseHash}`).first<{
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

  it("TASK3190 refuses one-time Pro callbacks unless amount and currency match the bought price", async () => {
    const cases = [
      {
        name: "exact",
        amount: ONE_TIME_PRO_AMOUNT_CENTS,
        currency: ONE_TIME_PRO_CURRENCY,
        accepted: true,
        reason: undefined,
      },
      {
        name: "one_cent_under",
        amount: ONE_TIME_PRO_AMOUNT_CENTS - 1,
        currency: ONE_TIME_PRO_CURRENCY,
        accepted: false,
        reason: ONE_TIME_PRO_AMOUNT_REFUSAL,
      },
      {
        name: "right_number_wrong_currency",
        amount: ONE_TIME_PRO_AMOUNT_CENTS,
        currency: "eur",
        accepted: false,
        reason: ONE_TIME_PRO_CURRENCY_REFUSAL,
      },
    ] as const;

    for (const testCase of cases) {
      const sessionId = `cs_live_task3190_${testCase.name}_${crypto.randomUUID().replace(/-/g, "")}`;
      const paymentIntentId = `pi_task3190_${testCase.name}_${crypto.randomUUID().replace(/-/g, "")}`;
      const licenseHash = `license-${sessionId}`;
      await insertPendingCheckoutClaim(sessionId, licenseHash);

      const response = await postSignedWebhook(SELF, {
        id: uniqueEventId(),
        type: "checkout.session.completed",
        data: {
          object: {
            id: sessionId,
            mode: "payment",
            metadata: {
              osl_plan: "pro",
              osl_purchase: "one-time",
              osl_fulfillment: "instant-v1",
            },
            payment_status: "paid",
            payment_intent: paymentIntentId,
            amount_total: testCase.amount,
            currency: testCase.currency,
          },
        },
      });
      expect(response.status).toBe(200);
      const body = await response.json() as { kind: string; reason?: string };
      expect(body.kind).toBe(testCase.accepted ? "applied" : "noop");
      if (testCase.reason) expect(body.reason).toBe(testCase.reason);

      const state = await env.DB.prepare(
        `SELECT
           (SELECT status FROM stripe_checkout_claims WHERE session_id = ?) AS claim_status,
           (SELECT COUNT(*) FROM licenses WHERE license_hash = ?) AS license_count,
           (SELECT COUNT(*) FROM commerce_events WHERE stripe_object_id = ?) AS commerce_count,
           (SELECT amount_cents FROM commerce_events WHERE stripe_object_id = ?) AS commerce_amount,
           (SELECT currency FROM commerce_events WHERE stripe_object_id = ?) AS commerce_currency`,
      ).bind(
        sessionId,
        licenseHash,
        sessionId,
        sessionId,
        sessionId,
      ).first<{
        claim_status: string;
        license_count: number;
        commerce_count: number;
        commerce_amount: number | null;
        commerce_currency: string | null;
      }>();
      expect(state?.claim_status).toBe(testCase.accepted ? "delivery_ready" : "pending");
      expect(state?.license_count).toBe(testCase.accepted ? 1 : 0);
      expect(state?.commerce_count).toBe(testCase.accepted ? 1 : 0);
      if (testCase.accepted) {
        expect(state?.commerce_amount).toBe(ONE_TIME_PRO_AMOUNT_CENTS);
        expect(state?.commerce_currency).toBe(ONE_TIME_PRO_CURRENCY);
      }

      console.log(`TASK3190 ${testCase.name}.amount_cents=${testCase.amount}`);
      console.log(`TASK3190 ${testCase.name}.currency=${testCase.currency}`);
      console.log(`TASK3190 ${testCase.name}.accepted=${testCase.accepted}`);
      if (!testCase.accepted) {
        console.log(`TASK3190 ${testCase.name}.refused=true`);
        console.log(`TASK3190 ${testCase.name}.reason=${body.reason}`);
      }
    }
  });

  it("TASK3738 counts one activation code only for exact amount and currency at the webhook edge", async () => {
    const runId = `task3738_${crypto.randomUUID().replace(/-/g, "")}`;
    const licenseHash = (name: string): string => `license-${runId}-${name}`;
    const countCodes = async (): Promise<number> => {
      const row = await env.DB.prepare(
        `SELECT COUNT(*) AS count FROM licenses
          WHERE license_hash IN (?, ?, ?)`,
      ).bind(
        licenseHash("valid"),
        licenseHash("wrong_currency"),
        licenseHash("one_cent_under"),
      ).first<{ count: number }>();
      return row?.count ?? 0;
    };
    const sendCheckout = async (
      name: string,
      amount: number,
      currency: string,
    ): Promise<{ kind: string; reason?: string }> => {
      const sessionId = `cs_live_${runId}_${name}`;
      const paymentIntentId = `pi_${runId}_${name}`;
      await insertPendingCheckoutClaim(sessionId, licenseHash(name));

      const response = await postSignedWebhook(SELF, {
        id: uniqueEventId(),
        type: "checkout.session.completed",
        data: {
          object: {
            id: sessionId,
            mode: "payment",
            metadata: {
              osl_plan: "pro",
              osl_purchase: "one-time",
              osl_fulfillment: "instant-v1",
            },
            payment_status: "paid",
            payment_intent: paymentIntentId,
            amount_total: amount,
            currency,
          },
        },
      });
      expect(response.status).toBe(200);
      return await response.json() as { kind: string; reason?: string };
    };

    const before = await countCodes();
    expect(before).toBe(0);
    console.log(`TASK3738 before.code_count=${before}`);

    const valid = await sendCheckout(
      "valid",
      ONE_TIME_PRO_AMOUNT_CENTS,
      ONE_TIME_PRO_CURRENCY,
    );
    expect(valid).toMatchObject({ kind: "applied" });
    const afterValid = await countCodes();
    expect(afterValid).toBe(1);
    console.log(`TASK3738 valid.amount_cents=${ONE_TIME_PRO_AMOUNT_CENTS}`);
    console.log(`TASK3738 valid.currency=${ONE_TIME_PRO_CURRENCY}`);
    console.log(`TASK3738 after_valid.code_count=${afterValid}`);

    const wrongCurrency = await sendCheckout(
      "wrong_currency",
      ONE_TIME_PRO_AMOUNT_CENTS,
      "eur",
    );
    expect(wrongCurrency).toMatchObject({
      kind: "noop",
      reason: ONE_TIME_PRO_CURRENCY_REFUSAL,
    });
    const afterWrongCurrency = await countCodes();
    expect(afterWrongCurrency).toBe(1);
    console.log(`TASK3738 wrong_currency.amount_cents=${ONE_TIME_PRO_AMOUNT_CENTS}`);
    console.log("TASK3738 wrong_currency.currency=eur");
    console.log(`TASK3738 wrong_currency.refusal=${wrongCurrency.reason}`);
    console.log(`TASK3738 after_wrong_currency.code_count=${afterWrongCurrency}`);

    const underAmount = await sendCheckout(
      "one_cent_under",
      ONE_TIME_PRO_AMOUNT_CENTS - 1,
      ONE_TIME_PRO_CURRENCY,
    );
    expect(underAmount).toMatchObject({
      kind: "noop",
      reason: ONE_TIME_PRO_AMOUNT_REFUSAL,
    });
    const afterUnderAmount = await countCodes();
    expect(afterUnderAmount).toBe(1);
    console.log(`TASK3738 one_cent_under.amount_cents=${ONE_TIME_PRO_AMOUNT_CENTS - 1}`);
    console.log(`TASK3738 one_cent_under.currency=${ONE_TIME_PRO_CURRENCY}`);
    console.log(`TASK3738 one_cent_under.refusal=${underAmount.reason}`);
    console.log(`TASK3738 after_one_cent_under.code_count=${afterUnderAmount}`);
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

describe("TASK 3199 refunded prepaid code enforcement", () => {
  const licenseHmac = "osl-license-test-secret-v1";

  async function buyOneTimeCode(label: string): Promise<{
    plaintext: string;
    paymentIntentId: string;
    checkoutKind: string;
  }> {
    const { plaintext, hash } = await generateLicenseKey(licenseHmac);
    const sessionId = `cs_task3199_${label}_${crypto.randomUUID().replace(/-/g, "")}`;
    const paymentIntentId = `pi_task3199_${label}_${crypto.randomUUID().replace(/-/g, "")}`;
    const now = Math.floor(Date.now() / 1000);
    await env.DB.prepare(
      `INSERT INTO stripe_checkout_claims (
         session_id, claim_hash, delivery_public_key_spki,
         encrypted_license, license_hash, subscription_id, status,
         created_at, expires_at, delivered_at
       ) VALUES (?, ?, 'public-key', 'ciphertext', ?, NULL, 'pending', ?, ?, NULL)`,
    ).bind(sessionId, `claim-${sessionId}`, hash, now, now + 3600).run();

    const checkout = await postSignedWebhook(SELF, {
      id: uniqueEventId(`evt_task3199_checkout_${label}`),
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
    expect(checkout.status).toBe(200);
    const checkoutBody = await checkout.json() as { kind: string };
    expect(checkoutBody.kind).toBe("applied");
    return { plaintext, paymentIntentId, checkoutKind: checkoutBody.kind };
  }

  async function redeem(licenseKey: string): Promise<Record<string, unknown>> {
    const response = await SELF.fetch("http://test/v1/license/redeem", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ license_key: licenseKey }),
    });
    expect(response.status).toBe(200);
    return await response.json() as Record<string, unknown>;
  }

  it("refund message stops one redeemed code while another account's code still works", async () => {
    const first = await buyOneTimeCode("refunded");
    const second = await buyOneTimeCode("unrefunded");

    const firstBeforeRefund = await redeem(first.plaintext);
    expect(firstBeforeRefund.status).toBe("ACTIVE");

    const refund = await postSignedWebhook(SELF, {
      id: uniqueEventId("evt_task3199_refund"),
      type: "charge.refunded",
      data: {
        object: {
          id: `ch_task3199_${crypto.randomUUID().replace(/-/g, "")}`,
          payment_intent: first.paymentIntentId,
          amount: 500,
          amount_refunded: 500,
          currency: "usd",
        },
      },
    });
    expect(refund.status).toBe(200);
    const refundBody = await refund.json() as { kind: string };
    expect(refundBody.kind).toBe("applied");

    const firstAfterRefund = await redeem(first.plaintext);
    expect(firstAfterRefund).toMatchObject({
      status: "REVOKED",
      checksum_ok: true,
      error: "this code was refunded",
    });

    const secondAfterRefund = await redeem(second.plaintext);
    expect(secondAfterRefund.status).toBe("ACTIVE");

    console.log(
      "TASK3199 keyserver " +
      JSON.stringify({
        bought_codes: 2,
        first_checkout_kind: first.checkoutKind,
        second_checkout_kind: second.checkoutKind,
        first_pro_before_refund: firstBeforeRefund.status,
        refund_kind: refundBody.kind,
        first_after_refund_status: firstAfterRefund.status,
        first_after_refund_error: firstAfterRefund.error,
        second_unrefunded_status: secondAfterRefund.status,
      }),
    );
  });
});
