import { env } from "cloudflare:test";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { handleCryptoQuote } from "../../src/endpoints/crypto-checkout.js";
import { handleCryptoDonationQuote } from "../../src/endpoints/crypto-donation-checkout.js";
import { handleCryptoDonationStatus } from "../../src/endpoints/crypto-donation-status.js";
import {
  handleCryptoSettlement,
  sweepAnonymousCryptoInvoices,
} from "../../src/endpoints/crypto-settlement.js";
import type { Env } from "../../src/env.js";
import { getCommerceSummary } from "../../src/lib/commerce-metrics.js";
import {
  settlementCanonical,
  sha256Hex,
  type WatcherSettlementEvidence,
} from "../../src/lib/crypto-watcher-auth.js";
import { notifyTelegramForCryptoDonation } from "../../src/lib/telegram.js";

const TEST_ED25519_SEED = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";

function bytesFromHex(value: string): Uint8Array {
  return Uint8Array.from(value.match(/../g) ?? [], (pair) => Number.parseInt(pair, 16));
}

function base64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function checkoutEnv(overrides: Partial<Env> = {}): Env {
  const result = Object.create(env) as Env;
  for (const [key, value] of Object.entries({
    CRYPTO_BTC_ENABLED: "true",
    CRYPTO_XMR_ENABLED: "true",
    ...overrides,
  })) {
    Object.defineProperty(result, key, {
      configurable: true,
      enumerable: true,
      writable: true,
      value,
    });
  }
  return result;
}

async function seedPrice(asset: "btc" | "xmr", price: string, fetchedAt?: number): Promise<void> {
  const today = new Date().toISOString().slice(0, 10);
  await env.DB.prepare(
    `INSERT INTO crypto_price_snapshots (asset, snapshot_date, price_usd, fetched_at)
     VALUES (?, ?, ?, ?)
     ON CONFLICT(asset, snapshot_date) DO UPDATE SET
       price_usd = excluded.price_usd, fetched_at = excluded.fetched_at`,
  ).bind(asset, today, price, fetchedAt ?? Math.floor(Date.now() / 1000)).run();
}

async function settlementHeaders(
  evidence: WatcherSettlementEvidence,
): Promise<Record<string, string>> {
  const prefix = bytesFromHex("302e020100300506032b657004220420");
  const seed = bytesFromHex(TEST_ED25519_SEED);
  const pkcs8 = new Uint8Array(prefix.length + seed.length);
  pkcs8.set(prefix);
  pkcs8.set(seed, prefix.length);
  const privateKey = await crypto.subtle.importKey(
    "pkcs8",
    pkcs8,
    { name: "Ed25519" },
    false,
    ["sign"],
  );
  const timestamp = Math.floor(Date.now() / 1000);
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

async function evidenceFor(
  invoice: { invoice_id: string; amount_atomic: string; expires_at: number },
  asset: "btc" | "xmr",
  confirmations: number,
  reference?: string,
): Promise<WatcherSettlementEvidence> {
  const commitment = reference ?? await sha256Hex(`${asset}:${invoice.invoice_id}:donation-output`);
  return {
    event_id: `evt_${await sha256Hex(`${invoice.invoice_id}:${asset}:${commitment}`)}`,
    invoice_id: invoice.invoice_id,
    payment_method: asset,
    amount_atomic: invoice.amount_atomic,
    confirmations,
    observed_at: Math.min(invoice.expires_at - 1, Math.floor(Date.now() / 1000)),
    payment_reference_commitment: commitment,
  };
}

function watcher(asset: "btc" | "xmr", status = 200): typeof fetch {
  return async (_input, init) => {
    if (status !== 200) return Response.json({ error: "unavailable" }, { status });
    const body = JSON.parse(String(init?.body)) as { invoice_id: string };
    return Response.json({
      invoice_id: body.invoice_id,
      address: asset === "btc" ? `bc1q${"q".repeat(38)}` : `8${"1".repeat(94)}`,
    });
  };
}

async function donationQuote(
  asset: "btc" | "xmr" = "btc",
  amountUsdCents = 2_000,
): Promise<{
  invoice_id: string;
  claim_token: string;
  payment_method: "btc" | "xmr";
  amount_atomic: string;
  amount_usd_cents: number;
  expires_at: number;
}> {
  const response = await handleCryptoDonationQuote(new Request(
    "https://keyserver.test/v1/donations/crypto/quote",
    {
      method: "POST",
      headers: { "content-type": "application/json", "x-forwarded-for": "192.0.2.140" },
      body: JSON.stringify({ payment_method: asset, amount_usd_cents: amountUsdCents }),
    },
  ), checkoutEnv(), watcher(asset));
  expect(response.status, await response.clone().text()).toBe(200);
  const body = await response.json() as {
    invoice_id: string;
    claim_token: string;
    payment_method: "btc" | "xmr";
    address: string;
    amount_native: string;
    amount_atomic: string;
    amount_usd_cents: number;
    price_locked_at: number;
    expires_at: number;
    confirmations_required: number;
  };
  expect(Object.keys(body).sort()).toEqual([
    "address",
    "amount_atomic",
    "amount_native",
    "amount_usd_cents",
    "claim_token",
    "confirmations_required",
    "expires_at",
    "invoice_id",
    "payment_method",
    "price_locked_at",
  ]);
  return body;
}

async function rsaPublicKey(): Promise<string> {
  const pair = await crypto.subtle.generateKey(
    { name: "RSA-OAEP", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" },
    true,
    ["encrypt", "decrypt"],
  ) as CryptoKeyPair;
  return base64(new Uint8Array(
    await crypto.subtle.exportKey("spki", pair.publicKey) as ArrayBuffer,
  ));
}

beforeEach(async () => {
  await seedPrice("btc", "60000");
  await seedPrice("xmr", "150");
});

describe("anonymous node-verified crypto donations", () => {
  it("accepts only an exact asset and bounded integer-cent body", async () => {
    for (const body of [
      null,
      [],
      { payment_method: "btc" },
      { payment_method: "btc", amount_usd_cents: 99 },
      { payment_method: "btc", amount_usd_cents: 1_000_001 },
      { payment_method: "btc", amount_usd_cents: 100.5 },
      { payment_method: "doge", amount_usd_cents: 100 },
      { payment_method: "btc", amount_usd_cents: 100, email: "donor@example.test" },
      { payment_method: "btc", amount_usd_cents: 100, delivery_public_key_spki: "x" },
      { payment_method: "btc", amount_usd_cents: 100, plan: "pro" },
    ]) {
      const response = await handleCryptoDonationQuote(new Request(
        "https://keyserver.test/v1/donations/crypto/quote",
        {
          method: "POST",
          headers: { "content-type": "application/json", "x-forwarded-for": "192.0.2.141" },
          body: JSON.stringify(body),
        },
      ), checkoutEnv(), async () => {
        throw new Error("invalid input reached watcher");
      });
      expect(response.status).toBe(400);
    }
    for (const amount of [100, 1_000_000]) {
      expect((await donationQuote("btc", amount)).amount_usd_cents).toBe(amount);
    }
  });

  it("fails closed for disabled assets, stale prices, and bad watcher responses", async () => {
    const disabled = await handleCryptoDonationQuote(new Request(
      "https://keyserver.test/v1/donations/crypto/quote",
      {
        method: "POST",
        headers: { "content-type": "application/json", "x-forwarded-for": "192.0.2.142" },
        body: JSON.stringify({ payment_method: "btc", amount_usd_cents: 500 }),
      },
    ), checkoutEnv({ CRYPTO_BTC_ENABLED: "false" }), async () => {
      throw new Error("disabled asset reached watcher");
    });
    expect(disabled.status).toBe(503);

    await seedPrice("btc", "60000", Math.floor(Date.now() / 1000) - 901);
    const stale = await handleCryptoDonationQuote(new Request(
      "https://keyserver.test/v1/donations/crypto/quote",
      {
        method: "POST",
        headers: { "content-type": "application/json", "x-forwarded-for": "192.0.2.143" },
        body: JSON.stringify({ payment_method: "btc", amount_usd_cents: 500 }),
      },
    ), checkoutEnv(), async () => {
      throw new Error("stale quote reached watcher");
    });
    expect(stale.status).toBe(503);

    await seedPrice("btc", "60000");
    const badWatcher = await handleCryptoDonationQuote(new Request(
      "https://keyserver.test/v1/donations/crypto/quote",
      {
        method: "POST",
        headers: { "content-type": "application/json", "x-forwarded-for": "192.0.2.144" },
        body: JSON.stringify({ payment_method: "btc", amount_usd_cents: 500 }),
      },
    ), checkoutEnv(), watcher("btc", 503));
    expect(badWatcher.status).toBe(503);
  });

  it("stores no donor, address, transaction, license, or entitlement columns", async () => {
    const invoice = await donationQuote("xmr", 5_000);
    const row = await env.DB.prepare(
      `SELECT payment_method, amount_usd_cents, amount_atomic, status
         FROM crypto_donation_invoices WHERE invoice_id = ?`,
    ).bind(invoice.invoice_id).first();
    expect(row).toMatchObject({ payment_method: "xmr", amount_usd_cents: 5_000, status: "pending" });
    const columns = await env.DB.prepare("PRAGMA table_info(crypto_donation_invoices)")
      .all<{ name: string }>();
    const names = columns.results.map((column) => column.name);
    for (const forbidden of [
      "email", "name", "message", "address", "transaction_id", "payment_reference",
      "delivery_public_key_spki", "encrypted_license", "account_id", "subscription_id",
    ]) {
      expect(names).not.toContain(forbidden);
    }
    const durableColumns = await env.DB.prepare("PRAGMA table_info(crypto_donation_events)")
      .all<{ name: string }>();
    expect(durableColumns.results.map((column) => column.name)).toEqual([
      "donation_id", "payment_method", "amount_usd_cents", "settled_at",
    ]);
  });

  it("records one donation without any Pro or commerce entitlement writes", async () => {
    const before = await getCommerceSummary(env.DB);
    const invoice = await donationQuote("btc", 2_000);
    const evidence = await evidenceFor(invoice, "btc", 2);
    const response = await handleCryptoSettlement(new Request(
      "https://keyserver.test/v1/internal/crypto/settle",
      {
        method: "POST",
        headers: await settlementHeaders(evidence),
        body: JSON.stringify(evidence),
      },
    ), checkoutEnv({ LICENSE_HMAC_SECRET: undefined }));
    expect(response.status, await response.clone().text()).toBe(200);
    await expect(response.json()).resolves.toMatchObject({ status: "recorded", duplicate: false });

    const counts = await env.DB.prepare(
      `SELECT
        (SELECT COUNT(*) FROM crypto_donation_events WHERE donation_id = ?) AS donations,
        (SELECT COUNT(*) FROM subscriptions WHERE subscription_id = ?) AS subscriptions,
        (SELECT COUNT(*) FROM licenses WHERE subscription_id = ?) AS licenses,
        (SELECT COUNT(*) FROM crypto_commerce_events WHERE subscription_id = ?) AS commerce`,
    ).bind(
      `crypto_${invoice.invoice_id}`,
      `crypto_${invoice.invoice_id}`,
      `crypto_${invoice.invoice_id}`,
      `crypto_${invoice.invoice_id}`,
    ).first();
    expect(counts).toEqual({ donations: 1, subscriptions: 0, licenses: 0, commerce: 0 });
    const after = await getCommerceSummary(env.DB);
    expect(after.verified_donations).toBe(before.verified_donations + 1);
    expect(after.donation_gross_cents).toBe(before.donation_gross_cents + 2_000);
    expect(after.successful_payments).toBe(before.successful_payments);
    expect(after.gross_cents).toBe(before.gross_cents);
  });

  it("rejects underpayment, wrong asset, insufficient confirmations, and late observations", async () => {
    const invoice = await donationQuote("btc", 500);
    const valid = await evidenceFor(invoice, "btc", 2);
    const cases: WatcherSettlementEvidence[] = [
      { ...valid, amount_atomic: (BigInt(valid.amount_atomic) - 1n).toString() },
      await evidenceFor(invoice, "xmr", 10),
      { ...valid, confirmations: 1 },
      { ...valid, observed_at: invoice.expires_at + 1 },
    ];
    for (const evidence of cases) {
      const response = await handleCryptoSettlement(new Request(
        "https://keyserver.test/v1/internal/crypto/settle",
        {
          method: "POST",
          headers: await settlementHeaders(evidence),
          body: JSON.stringify(evidence),
        },
      ), checkoutEnv());
      expect(response.status).toBe(409);
    }
    const invalidHeaders = await settlementHeaders(valid);
    invalidHeaders["x-osl-settlement-signature"] = base64(new Uint8Array(64));
    const invalidSignature = await handleCryptoSettlement(new Request(
      "https://keyserver.test/v1/internal/crypto/settle",
      {
        method: "POST",
        headers: invalidHeaders,
        body: JSON.stringify(valid),
      },
    ), checkoutEnv());
    expect(invalidSignature.status).toBe(401);
  });

  it("deduplicates concurrent callbacks and returns exact recorded acknowledgements", async () => {
    const invoice = await donationQuote("xmr", 1_000);
    const evidence = await evidenceFor(invoice, "xmr", 10);
    const send = async () => await handleCryptoSettlement(new Request(
      "https://keyserver.test/v1/internal/crypto/settle",
      {
        method: "POST",
        headers: await settlementHeaders(evidence),
        body: JSON.stringify(evidence),
      },
    ), checkoutEnv());
    const responses = await Promise.all([send(), send()]);
    expect(responses.map((response) => response.status)).toEqual([200, 200]);
    for (const response of responses) {
      await expect(response.json()).resolves.toMatchObject({ ok: true, status: "recorded" });
    }
    const retry = await send();
    await expect(retry.json()).resolves.toEqual({ ok: true, duplicate: true, status: "recorded" });
    const count = await env.DB.prepare(
      "SELECT COUNT(*) AS count FROM crypto_donation_events WHERE donation_id = ?",
    ).bind(`crypto_${invoice.invoice_id}`).first<{ count: number }>();
    expect(count?.count).toBe(1);
  });

  it("fails closed if a durable donation id conflicts with the invoice terms", async () => {
    const invoice = await donationQuote("btc", 700);
    await env.DB.prepare(
      `INSERT INTO crypto_donation_events
        (donation_id, payment_method, amount_usd_cents, settled_at)
       VALUES (?, 'btc', 701, ?)`,
    ).bind(`crypto_${invoice.invoice_id}`, Math.floor(Date.now() / 1000)).run();
    const evidence = await evidenceFor(invoice, "btc", 2);
    const response = await handleCryptoSettlement(new Request(
      "https://keyserver.test/v1/internal/crypto/settle",
      {
        method: "POST",
        headers: await settlementHeaders(evidence),
        body: JSON.stringify(evidence),
      },
    ), checkoutEnv());
    expect(response.status).toBe(503);
    const stored = await env.DB.prepare(
      "SELECT status FROM crypto_donation_invoices WHERE invoice_id = ?",
    ).bind(invoice.invoice_id).first<{ status: string }>();
    expect(stored?.status).toBe("paid");
    const entitlement = await env.DB.prepare(
      "SELECT 1 AS present FROM licenses WHERE subscription_id = ?",
    ).bind(`crypto_${invoice.invoice_id}`).first();
    expect(entitlement).toBeNull();
  });

  it("uses one global chain-reference boundary across donations and Pro", async () => {
    const donation = await donationQuote("btc", 500);
    const reference = await sha256Hex("one-output-cannot-buy-and-donate");
    const donationEvidence = await evidenceFor(donation, "btc", 2, reference);
    const accepted = await handleCryptoSettlement(new Request(
      "https://keyserver.test/v1/internal/crypto/settle",
      {
        method: "POST",
        headers: await settlementHeaders(donationEvidence),
        body: JSON.stringify(donationEvidence),
      },
    ), checkoutEnv());
    expect(accepted.status).toBe(200);

    const publicKey = await rsaPublicKey();
    const proResponse = await handleCryptoQuote(new Request(
      "https://keyserver.test/v1/crypto/quote",
      {
        method: "POST",
        headers: { "content-type": "application/json", "x-forwarded-for": "192.0.2.145" },
        body: JSON.stringify({
          plan: "pro",
          payment_method: "btc",
          delivery_public_key_spki: publicKey,
        }),
      },
    ), checkoutEnv(), watcher("btc"));
    expect(proResponse.status).toBe(200);
    const pro = await proResponse.json() as {
      invoice_id: string; amount_atomic: string; expires_at: number;
    };
    const proEvidence = await evidenceFor(pro, "btc", 2, reference);
    const replay = await handleCryptoSettlement(new Request(
      "https://keyserver.test/v1/internal/crypto/settle",
      {
        method: "POST",
        headers: await settlementHeaders(proEvidence),
        body: JSON.stringify(proEvidence),
      },
    ), checkoutEnv());
    expect(replay.status).toBe(409);
    await expect(replay.json()).resolves.toMatchObject({
      error: "payment reference is already assigned to another invoice",
    });
    const entitlement = await env.DB.prepare(
      "SELECT 1 AS present FROM licenses WHERE subscription_id = ?",
    ).bind(`crypto_${pro.invoice_id}`).first();
    expect(entitlement).toBeNull();

    const foreignKeys = await env.DB.prepare("PRAGMA foreign_key_list(crypto_payment_references_v2)")
      .all();
    expect(foreignKeys.results).toEqual([]);
  });

  it("keeps durable totals after transient invoice and settlement cleanup", async () => {
    const invoice = await donationQuote("xmr", 2_500);
    const evidence = await evidenceFor(invoice, "xmr", 10);
    const settled = await handleCryptoSettlement(new Request(
      "https://keyserver.test/v1/internal/crypto/settle",
      {
        method: "POST",
        headers: await settlementHeaders(evidence),
        body: JSON.stringify(evidence),
      },
    ), checkoutEnv());
    expect(settled.status).toBe(200);
    const before = await getCommerceSummary(env.DB);
    await env.DB.batch([
      env.DB.prepare(
        "UPDATE crypto_donation_invoices SET cleanup_at = 1 WHERE invoice_id = ?",
      ).bind(invoice.invoice_id),
      env.DB.prepare(
        "UPDATE crypto_settlement_events_v2 SET processed_at = 1 WHERE invoice_id = ?",
      ).bind(invoice.invoice_id),
    ]);
    await sweepAnonymousCryptoInvoices(env.DB);
    const transients = await env.DB.prepare(
      `SELECT
        (SELECT COUNT(*) FROM crypto_donation_invoices WHERE invoice_id = ?) AS invoices,
        (SELECT COUNT(*) FROM crypto_settlement_events_v2 WHERE invoice_id = ?) AS settlements`,
    ).bind(invoice.invoice_id, invoice.invoice_id).first();
    expect(transients).toEqual({ invoices: 0, settlements: 0 });
    expect(await getCommerceSummary(env.DB)).toEqual(before);
  });

  it("validates status capabilities without leaking invoice existence", async () => {
    const invoice = await donationQuote("btc", 300);
    const request = (body: unknown) => new Request(
      "https://keyserver.test/v1/donations/crypto/status",
      {
        method: "POST",
        headers: { "content-type": "application/json", "x-forwarded-for": "192.0.2.146" },
        body: JSON.stringify(body),
      },
    );
    const pending = await handleCryptoDonationStatus(request({
      invoice_id: invoice.invoice_id,
      claim_token: invoice.claim_token,
    }), checkoutEnv());
    await expect(pending.json()).resolves.toEqual({
      invoice_id: invoice.invoice_id,
      status: "pending",
      payment_method: "btc",
      amount_usd_cents: 300,
      expires_at: invoice.expires_at,
    });
    const wrong = await handleCryptoDonationStatus(request({
      invoice_id: invoice.invoice_id,
      claim_token: "x".repeat(43),
    }), checkoutEnv());
    expect(wrong.status).toBe(403);
    const unknown = await handleCryptoDonationStatus(request({
      invoice_id: `cdon_${"f".repeat(32)}`,
      claim_token: invoice.claim_token,
    }), checkoutEnv());
    expect(unknown.status).toBe(403);
    const unexpected = await handleCryptoDonationStatus(request({
      invoice_id: invoice.invoice_id,
      claim_token: invoice.claim_token,
      acknowledge_delivery: true,
    }), checkoutEnv());
    expect(unexpected.status).toBe(400);
  });

  it("sends operators an aggregate-only crypto donation alert", async () => {
    const fetcher = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
      const body = JSON.parse(String(init?.body)) as { text: string };
      expect(body.text).toContain("OSL crypto donation verified");
      expect(body.text).toContain("via Bitcoin");
      expect(body.text).toContain("Verified donations:");
      expect(body.text).not.toMatch(/cdon_|address|transaction|claim|license/i);
      return Response.json({ ok: true });
    });
    await notifyTelegramForCryptoDonation(checkoutEnv({
      TELEGRAM_BOT_TOKEN: "test_bot_token_for_crypto_donation_alerts",
      TELEGRAM_OPERATOR_CHAT_IDS: "6916231544",
    }), "btc", 2_000, fetcher as typeof fetch);
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
});
