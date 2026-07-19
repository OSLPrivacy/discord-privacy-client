import { SELF, env } from "cloudflare:test";
import { beforeAll, describe, expect, it } from "vitest";
import { handleCryptoQuote } from "../../src/endpoints/crypto-checkout.js";
import { handleCryptoSettlement } from "../../src/endpoints/crypto-settlement.js";
import {
  settlementCanonical,
  sha256Hex,
  type WatcherSettlementEvidence,
} from "../../src/lib/crypto-watcher-auth.js";
import { getCommerceSummary } from "../../src/lib/commerce-metrics.js";
import type { Env } from "../../src/env.js";

const TEST_ED25519_SEED = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";

function bytesFromHex(value: string): Uint8Array {
  return Uint8Array.from(value.match(/../g) ?? [], (pair) => Number.parseInt(pair, 16));
}

function base64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

async function settlementHeaders(
  evidence: WatcherSettlementEvidence,
  timestamp = Math.floor(Date.now() / 1000),
): Promise<Record<string, string>> {
  const pkcs8Prefix = bytesFromHex("302e020100300506032b657004220420");
  const seed = bytesFromHex(TEST_ED25519_SEED);
  const pkcs8 = new Uint8Array(pkcs8Prefix.length + seed.length);
  pkcs8.set(pkcs8Prefix);
  pkcs8.set(seed, pkcs8Prefix.length);
  const privateKey = await crypto.subtle.importKey(
    "pkcs8",
    pkcs8,
    { name: "Ed25519" },
    false,
    ["sign"],
  );
  const signature = await crypto.subtle.sign(
    { name: "Ed25519" },
    privateKey,
    new TextEncoder().encode(settlementCanonical(
      "POST",
      "/v1/internal/crypto/settle",
      String(timestamp),
      evidence,
    )),
  );
  return {
    "content-type": "application/json",
    "x-osl-timestamp": String(timestamp),
    "x-osl-settlement-signature": base64(new Uint8Array(signature)),
  };
}

async function settlementEvidence(
  invoice: { invoice_id: string; amount_atomic: string; expires_at: number },
  paymentMethod: "btc" | "xmr",
  confirmations: number,
  paymentReferenceCommitment?: string,
): Promise<WatcherSettlementEvidence> {
  const referenceCommitment = paymentReferenceCommitment ?? await sha256Hex(
    `${paymentMethod}:${invoice.invoice_id}:test-payment-reference`,
  );
  return {
    event_id: `evt_${await sha256Hex(
      `${invoice.invoice_id}:${paymentMethod}:${referenceCommitment}`,
    )}`,
    invoice_id: invoice.invoice_id,
    payment_method: paymentMethod,
    amount_atomic: invoice.amount_atomic,
    confirmations,
    observed_at: Math.min(invoice.expires_at - 1, Math.floor(Date.now() / 1000)),
    payment_reference_commitment: referenceCommitment,
  };
}

async function seedTodayPrice(asset: "btc" | "xmr", price: string): Promise<void> {
  const today = new Date().toISOString().slice(0, 10);
  await env.DB.prepare(
    `INSERT INTO crypto_price_snapshots (asset, snapshot_date, price_usd, fetched_at)
     VALUES (?, ?, ?, strftime('%s','now'))
     ON CONFLICT(asset, snapshot_date) DO UPDATE SET
       price_usd = excluded.price_usd, fetched_at = excluded.fetched_at`,
  ).bind(asset, today, price).run();
}

async function deliveryKeys(): Promise<{ publicKey: string; privateKey: CryptoKey }> {
  const pair = await crypto.subtle.generateKey(
    { name: "RSA-OAEP", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" },
    true,
    ["encrypt", "decrypt"],
  ) as CryptoKeyPair;
  return {
    publicKey: base64(new Uint8Array(
      await crypto.subtle.exportKey("spki", pair.publicKey) as ArrayBuffer,
    )),
    privateKey: pair.privateKey,
  };
}

function checkoutEnv(overrides: Partial<Env> = {}): Env {
  const result = Object.create(env) as Env;
  const values: Partial<Env> = {
    CRYPTO_BTC_ENABLED: "true",
    CRYPTO_XMR_ENABLED: "true",
    ...overrides,
  };
  for (const [key, value] of Object.entries(values)) {
    Object.defineProperty(result, key, {
      configurable: true,
      enumerable: true,
      writable: true,
      value,
    });
  }
  return result;
}

async function quote(
  asset: "btc" | "xmr",
  publicKey: string,
): Promise<{ invoice_id: string; claim_token: string; amount_atomic: string; expires_at: number }> {
  const response = await handleCryptoQuote(new Request("http://test/v1/crypto/quote", {
    method: "POST",
    headers: { "content-type": "application/json", "x-forwarded-for": `192.0.2.${asset === "btc" ? 90 : 91}` },
    body: JSON.stringify({
      plan: "pro",
      payment_method: asset,
      delivery_public_key_spki: publicKey,
    }),
  }), checkoutEnv(), async (_input, init) => {
    const watcherInvoice = JSON.parse(String(init?.body)) as { invoice_id: string };
    return Response.json({
      invoice_id: watcherInvoice.invoice_id,
      address: asset === "btc"
        ? `bc1q${"q".repeat(38)}`
        : `8${"1".repeat(94)}`,
    });
  });
  expect(response.status).toBe(200);
  return await response.json() as {
    invoice_id: string; claim_token: string; amount_atomic: string; expires_at: number;
  };
}

beforeAll(async () => {
  await seedTodayPrice("btc", "60000");
  await seedTodayPrice("xmr", "150");
});

describe("anonymous node-verified lifetime Pro flow", () => {
  it("matches the watcher Ed25519 settlement canonical test vector", async () => {
    const evidence: WatcherSettlementEvidence = {
      event_id: `evt_${"a".repeat(64)}`,
      invoice_id: `cpay_${"b".repeat(32)}`,
      payment_method: "btc",
      amount_atomic: "8333",
      confirmations: 2,
      observed_at: 1_750_000_000,
      payment_reference_commitment: "c".repeat(64),
    };
    const headers = await settlementHeaders(evidence, 1_750_000_100);
    expect(headers["x-osl-settlement-signature"]).toBe(
      "Z/Xa1d1xDhadUdNpiQC6Um29kaUEgzeziin/qbqx0iQ8m8ZUmcHLcQ31b6simvSYbQ81J8wlGca1Ua8ino9cBw==",
    );
  });

  it("accepts only server-priced one-time Pro and rejects recurring or browser pricing", async () => {
    const keys = await deliveryKeys();
    for (const body of [
      { plan: "monthly", payment_method: "btc", delivery_public_key_spki: keys.publicKey },
      { plan: "yearly", payment_method: "btc", delivery_public_key_spki: keys.publicKey },
      { plan: "pro", payment_method: "btc", delivery_public_key_spki: keys.publicKey, amount_usd_cents: 1 },
      { plan: "pro", payment_method: "btc", delivery_public_key_spki: keys.publicKey, price: "0.01" },
    ]) {
      const response = await handleCryptoQuote(new Request("http://test/v1/crypto/quote", {
        method: "POST",
        headers: { "content-type": "application/json", "x-forwarded-for": "192.0.2.92" },
        body: JSON.stringify(body),
      }), checkoutEnv(), async () => {
        throw new Error("watcher must not be called");
      });
      expect(response.status).toBe(400);
    }

    let watcherBody = "";
    const response = await handleCryptoQuote(new Request("http://test/v1/crypto/quote", {
      method: "POST",
      headers: { "content-type": "application/json", "x-forwarded-for": "192.0.2.93" },
      body: JSON.stringify({ plan: "pro", payment_method: "btc", delivery_public_key_spki: keys.publicKey }),
    }), checkoutEnv(), async (_input, init) => {
      watcherBody = String(init?.body ?? "");
      const watcherInvoice = JSON.parse(String(init?.body)) as { invoice_id: string };
      return Response.json({
        invoice_id: watcherInvoice.invoice_id,
        address: `bc1q${"q".repeat(38)}`,
      });
    });
    expect(response.status).toBe(200);
    const invoice = await response.json() as { invoice_id: string; amount_usd_cents: number };
    expect(invoice.amount_usd_cents).toBe(500);
    expect(JSON.parse(watcherBody)).not.toHaveProperty("amount_usd_cents");
    const stored = await env.DB.prepare(
      "SELECT plan, amount_usd_cents FROM crypto_invoices_v2 WHERE invoice_id = ?",
    ).bind(invoice.invoice_id).first<{ plan: string; amount_usd_cents: number }>();
    expect(stored).toEqual({ plan: "pro", amount_usd_cents: 500 });
  });

  it("rejects non-object JSON bodies before reading checkout fields", async () => {
    for (const body of ["null", "[]", '"pro"']) {
      const response = await handleCryptoQuote(new Request("http://test/v1/crypto/quote", {
        method: "POST",
        headers: { "content-type": "application/json", "x-forwarded-for": "192.0.2.94" },
        body,
      }), checkoutEnv(), async () => {
        throw new Error("watcher must not be called");
      });
      expect(response.status).toBe(400);
    }
  });

  it("fails closed per asset before contacting the watcher", async () => {
    const keys = await deliveryKeys();
    for (const [asset, override] of [
      ["btc", { CRYPTO_BTC_ENABLED: undefined }],
      ["xmr", { CRYPTO_XMR_ENABLED: "false" }],
    ] as const) {
      let watcherCalls = 0;
      const disabledEnv = checkoutEnv(override);
      const response = await handleCryptoQuote(new Request("http://test/v1/crypto/quote", {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "x-forwarded-for": `192.0.2.${asset === "btc" ? 95 : 96}`,
        },
        body: JSON.stringify({
          plan: "pro",
          payment_method: asset,
          delivery_public_key_spki: keys.publicKey,
        }),
      }), disabledEnv, async () => {
        watcherCalls += 1;
        throw new Error("disabled asset must not reach watcher");
      });
      expect(response.status).toBe(503);
      expect(watcherCalls).toBe(0);
    }
  });

  it("rejects wrong amount, asset, confirmations, and signature before issuing", async () => {
    const keys = await deliveryKeys();
    const invoice = await quote("btc", keys.publicKey);
    const valid = await settlementEvidence(invoice, "btc", 2);
    const cases: Array<{ evidence: WatcherSettlementEvidence; badSignature?: boolean; status: number }> = [
      { evidence: { ...valid, amount_atomic: (BigInt(valid.amount_atomic) - 1n).toString() }, status: 409 },
      { evidence: await settlementEvidence(invoice, "xmr", 10), status: 409 },
      { evidence: { ...valid, confirmations: 1 }, status: 409 },
      { evidence: valid, badSignature: true, status: 401 },
    ];
    for (const testCase of cases) {
      const headers = await settlementHeaders(testCase.evidence);
      if (testCase.badSignature) headers["x-osl-settlement-signature"] = base64(new Uint8Array(64));
      const response = await SELF.fetch("http://test/v1/internal/crypto/settle", {
        method: "POST",
        headers,
        body: JSON.stringify(testCase.evidence),
      });
      expect(response.status).toBe(testCase.status);
    }
    const row = await env.DB.prepare(
      "SELECT status FROM crypto_invoices_v2 WHERE invoice_id = ?",
    ).bind(invoice.invoice_id).first<{ status: string }>();
    expect(row?.status).toBe("pending");
  });

  it("deduplicates replay and concurrent callbacks", async () => {
    const keys = await deliveryKeys();
    const invoice = await quote("btc", keys.publicKey);
    const evidence = await settlementEvidence(invoice, "btc", 2);
    const headers = await settlementHeaders(evidence);
    const send = () => SELF.fetch("http://test/v1/internal/crypto/settle", {
      method: "POST",
      headers,
      body: JSON.stringify(evidence),
    });
    const concurrent = await Promise.all([send(), send()]);
    expect(concurrent.map((response) => response.status)).toEqual([200, 200]);
    const retry = await send();
    expect(retry.status).toBe(200);
    await expect(retry.json()).resolves.toMatchObject({ duplicate: true });
    const counts = await env.DB.prepare(
      `SELECT
         (SELECT COUNT(*) FROM licenses WHERE subscription_id = ?) AS licenses,
         (SELECT COUNT(*) FROM crypto_settlement_events_v2 WHERE invoice_id = ?) AS events`,
    ).bind(`crypto_${invoice.invoice_id}`, invoice.invoice_id).first<{
      licenses: number; events: number;
    }>();
    expect(counts).toEqual({ licenses: 1, events: 1 });
  });

  it("delivers the encrypted activation once and destroys it after acknowledgement", async () => {
    const commerceBefore = await getCommerceSummary(env.DB);
    const keys = await deliveryKeys();
    const invoice = await quote("btc", keys.publicKey);
    const evidence = await settlementEvidence(invoice, "btc", 2);
    const settled = await SELF.fetch("http://test/v1/internal/crypto/settle", {
      method: "POST",
      headers: await settlementHeaders(evidence),
      body: JSON.stringify(evidence),
    });
    expect(settled.status).toBe(200);
    const commerceAfter = await getCommerceSummary(env.DB);
    expect(commerceAfter.successful_payments).toBe(commerceBefore.successful_payments + 1);
    expect(commerceAfter.gross_cents).toBe(commerceBefore.gross_cents + 500);

    const wrongClaim = await SELF.fetch("http://test/v1/crypto/status", {
      method: "POST",
      headers: { "content-type": "application/json", "x-forwarded-for": "192.0.2.121" },
      body: JSON.stringify({ invoice_id: invoice.invoice_id, claim_token: "x".repeat(43) }),
    });
    expect(wrongClaim.status).toBe(403);

    const ready = await SELF.fetch("http://test/v1/crypto/status", {
      method: "POST",
      headers: { "content-type": "application/json", "x-forwarded-for": "192.0.2.122" },
      body: JSON.stringify({ invoice_id: invoice.invoice_id, claim_token: invoice.claim_token }),
    });
    expect(ready.status).toBe(200);
    const delivery = await ready.json() as { status: string; encrypted_license: string };
    expect(delivery.status).toBe("delivery_ready");
    const encrypted = Uint8Array.from(atob(delivery.encrypted_license), (character) =>
      character.charCodeAt(0));
    const plaintext = new TextDecoder().decode(await crypto.subtle.decrypt(
      { name: "RSA-OAEP" },
      keys.privateKey,
      encrypted,
    ));
    expect(plaintext).toMatch(/^OSL-[0-9A-HJKMNP-TV-Z]{4}(?:-[0-9A-HJKMNP-TV-Z]{4}){3}$/);

    const acknowledged = await SELF.fetch("http://test/v1/crypto/status", {
      method: "POST",
      headers: { "content-type": "application/json", "x-forwarded-for": "192.0.2.123" },
      body: JSON.stringify({
        invoice_id: invoice.invoice_id,
        claim_token: invoice.claim_token,
        acknowledge_delivery: true,
      }),
    });
    expect(acknowledged.status).toBe(200);
    await expect(acknowledged.json()).resolves.toMatchObject({ status: "acknowledged" });

    const gone = await SELF.fetch("http://test/v1/crypto/status", {
      method: "POST",
      headers: { "content-type": "application/json", "x-forwarded-for": "192.0.2.124" },
      body: JSON.stringify({ invoice_id: invoice.invoice_id, claim_token: invoice.claim_token }),
    });
    expect(gone.status).toBe(410);
    const stored = await env.DB.prepare(
      "SELECT encrypted_license, acknowledged_at FROM crypto_invoices_v2 WHERE invoice_id = ?",
    ).bind(invoice.invoice_id).first<{ encrypted_license: string | null; acknowledged_at: number | null }>();
    expect(stored?.encrypted_license).toBeNull();
    expect(stored?.acknowledged_at).not.toBeNull();
  });

  it("freezes confirmations at quote time and rejects impossible observation times", async () => {
    const keys = await deliveryKeys();
    const invoice = await quote("btc", keys.publicKey);
    const stored = await env.DB.prepare(
      "SELECT created_at, confirmations_required, price_locked_at FROM crypto_invoices_v2 WHERE invoice_id = ?",
    ).bind(invoice.invoice_id).first<{
      created_at: number; confirmations_required: number; price_locked_at: number;
    }>();
    expect(stored?.confirmations_required).toBe(2);
    expect(stored?.price_locked_at).toBeGreaterThan(0);

    const beforeCreation = await settlementEvidence(invoice, "btc", 2);
    beforeCreation.observed_at = (stored?.created_at ?? 1) - 1;
    const early = await handleCryptoSettlement(new Request(
      "http://test/v1/internal/crypto/settle",
      {
        method: "POST",
        headers: await settlementHeaders(beforeCreation),
        body: JSON.stringify(beforeCreation),
      },
    ), checkoutEnv({ CRYPTO_BTC_CONFIRMATIONS: "999" }));
    expect(early.status).toBe(409);

    const future = await settlementEvidence(invoice, "btc", 2);
    future.observed_at = Math.floor(Date.now() / 1000) + 301;
    const futureResponse = await handleCryptoSettlement(new Request(
      "http://test/v1/internal/crypto/settle",
      {
        method: "POST",
        headers: await settlementHeaders(future),
        body: JSON.stringify(future),
      },
    ), checkoutEnv({ CRYPTO_BTC_CONFIRMATIONS: "999" }));
    expect(futureResponse.status).toBe(409);

    const valid = await settlementEvidence(invoice, "btc", 2);
    const accepted = await handleCryptoSettlement(new Request(
      "http://test/v1/internal/crypto/settle",
      {
        method: "POST",
        headers: await settlementHeaders(valid),
        body: JSON.stringify(valid),
      },
    ), checkoutEnv({ CRYPTO_BTC_CONFIRMATIONS: "999" }));
    expect(accepted.status).toBe(200);
  });

  it("rejects assigning one on-chain payment reference to a second invoice", async () => {
    const keys = await deliveryKeys();
    const firstInvoice = await quote("btc", keys.publicKey);
    const secondInvoice = await quote("btc", keys.publicKey);
    const sharedReference = await sha256Hex("btc:single-on-chain-payment-reference");
    const firstEvidence = await settlementEvidence(firstInvoice, "btc", 2, sharedReference);
    const first = await SELF.fetch("http://test/v1/internal/crypto/settle", {
      method: "POST",
      headers: await settlementHeaders(firstEvidence),
      body: JSON.stringify(firstEvidence),
    });
    expect(first.status, await first.clone().text()).toBe(200);

    const secondEvidence = await settlementEvidence(secondInvoice, "btc", 2, sharedReference);
    const second = await SELF.fetch("http://test/v1/internal/crypto/settle", {
      method: "POST",
      headers: await settlementHeaders(secondEvidence),
      body: JSON.stringify(secondEvidence),
    });
    expect(second.status).toBe(409);
    await expect(second.json()).resolves.toMatchObject({
      error: "payment reference is already assigned to another invoice",
    });
    const secondStored = await env.DB.prepare(
      "SELECT status FROM crypto_invoices_v2 WHERE invoice_id = ?",
    ).bind(secondInvoice.invoice_id).first<{ status: string }>();
    expect(secondStored?.status).toBe("pending");
    const secondLicense = await env.DB.prepare(
      "SELECT 1 AS present FROM licenses WHERE subscription_id = ?",
    ).bind(`crypto_${secondInvoice.invoice_id}`).first();
    expect(secondLicense).toBeNull();
  });

  it("gives BTC and XMR the same $5 lifetime Pro entitlement as Stripe", async () => {
    for (const asset of ["btc", "xmr"] as const) {
      const keys = await deliveryKeys();
      const invoice = await quote(asset, keys.publicKey);
      const evidence = await settlementEvidence(invoice, asset, asset === "btc" ? 2 : 10);
      const response = await SELF.fetch("http://test/v1/internal/crypto/settle", {
        method: "POST",
        headers: await settlementHeaders(evidence),
        body: JSON.stringify(evidence),
      });
      expect(response.status, await response.clone().text()).toBe(200);
      const entitlement = await env.DB.prepare(
        `SELECT status, current_period_end, cancel_at_period_end
           FROM subscriptions WHERE subscription_id = ?`,
      ).bind(`crypto_${invoice.invoice_id}`).first<{
        status: string; current_period_end: number | null; cancel_at_period_end: number;
      }>();
      expect(entitlement).toEqual({
        status: "ACTIVE",
        current_period_end: null,
        cancel_at_period_end: 0,
      });
      const stored = await env.DB.prepare(
        "SELECT plan, amount_usd_cents FROM crypto_invoices_v2 WHERE invoice_id = ?",
      ).bind(invoice.invoice_id).first<{ plan: string; amount_usd_cents: number }>();
      expect(stored).toEqual({ plan: "pro", amount_usd_cents: 500 });
    }
  });

  it("old email/txid/manual-confirm routes are no longer exposed", async () => {
    for (const path of ["/v1/crypto/submit", "/v1/admin/crypto/confirm"]) {
      const response = await SELF.fetch(`http://test${path}`, {
        method: "POST", headers: { "content-type": "application/json" }, body: "{}",
      });
      expect(response.status).toBe(404);
    }
  });
});
