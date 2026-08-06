import { env } from "cloudflare:test";
import { beforeAll, describe, expect, it } from "vitest";
import { handleCryptoQuote } from "../../src/endpoints/crypto-checkout.js";
import { handleCryptoSettlement } from "../../src/endpoints/crypto-settlement.js";
import {
  settlementCanonical,
  sha256Hex,
  type WatcherSettlementEvidence,
} from "../../src/lib/crypto-watcher-auth.js";
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
    `${paymentMethod}:${invoice.invoice_id}:task3195-payment-reference`,
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

async function deliveryPublicKey(): Promise<string> {
  const pair = await crypto.subtle.generateKey(
    { name: "RSA-OAEP", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" },
    true,
    ["encrypt", "decrypt"],
  ) as CryptoKeyPair;
  return base64(new Uint8Array(
    await crypto.subtle.exportKey("spki", pair.publicKey) as ArrayBuffer,
  ));
}

function checkoutEnv(): Env {
  const result = Object.create(env) as Env;
  for (const [key, value] of Object.entries({
    CRYPTO_BTC_ENABLED: "true",
    CRYPTO_XMR_ENABLED: "true",
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

const redemptionReady = () => true;

async function quote(
  publicKey: string,
): Promise<{ invoice_id: string; claim_token: string; amount_atomic: string; expires_at: number }> {
  const response = await handleCryptoQuote(new Request("http://test/v1/crypto/quote", {
    method: "POST",
    headers: { "content-type": "application/json", "x-forwarded-for": "192.0.2.195" },
    body: JSON.stringify({
      plan: "pro",
      payment_method: "btc",
      delivery_public_key_spki: publicKey,
    }),
  }), checkoutEnv(), async (_input, init) => {
    const watcherInvoice = JSON.parse(String(init?.body)) as { invoice_id: string };
    return Response.json({
      invoice_id: watcherInvoice.invoice_id,
      address: `bc1q${"q".repeat(38)}`,
    });
  }, redemptionReady);
  expect(response.status).toBe(200);
  return await response.json() as {
    invoice_id: string; claim_token: string; amount_atomic: string; expires_at: number;
  };
}

async function settle(
  evidence: WatcherSettlementEvidence,
  headers?: Record<string, string>,
): Promise<Response> {
  return await handleCryptoSettlement(new Request(
    "http://test/v1/internal/crypto/settle",
    {
      method: "POST",
      headers: headers ?? await settlementHeaders(evidence),
      body: JSON.stringify(evidence),
    },
  ), checkoutEnv(), undefined, redemptionReady);
}

async function codeCount(invoiceIds: readonly string[]): Promise<number> {
  const placeholders = invoiceIds.map(() => "?").join(", ");
  const row = await env.DB.prepare(
    `SELECT COUNT(*) AS count FROM licenses
      WHERE subscription_id IN (${placeholders})`,
  ).bind(...invoiceIds.map((invoiceId) => `crypto_${invoiceId}`)).first<{ count: number }>();
  return row?.count ?? 0;
}

beforeAll(async () => {
  await seedTodayPrice("btc", "60000");
});

describe("TASK3195 forged callback signature refusal", () => {
  it("keeps a forged different-account callback from minting a second code and records bad-signature", async () => {
    const publicKey = await deliveryPublicKey();
    const paidInvoice = await quote(publicKey);
    const forgedInvoice = await quote(publicKey);
    expect(forgedInvoice.invoice_id).not.toBe(paidInvoice.invoice_id);

    const invoiceIds = [paidInvoice.invoice_id, forgedInvoice.invoice_id] as const;
    const before = await codeCount(invoiceIds);
    console.log(`TASK3195 before_code_count=${before}`);
    expect(before).toBe(0);

    const paidEvidence = await settlementEvidence(paidInvoice, "btc", 2);
    const paid = await settle(paidEvidence);
    expect(paid.status, await paid.clone().text()).toBe(200);

    const afterGood = await codeCount(invoiceIds);
    console.log(`TASK3195 after_good_code_count=${afterGood}`);
    expect(afterGood).toBe(1);

    const forgedEvidence = await settlementEvidence(forgedInvoice, "btc", 2);
    const forged = await settle(forgedEvidence, await settlementHeaders(paidEvidence));
    expect(forged.status).toBe(401);
    await expect(forged.json()).resolves.toMatchObject({
      error: "invalid watcher signature",
    });

    const afterForged = await codeCount(invoiceIds);
    console.log(`TASK3195 after_forged_code_count=${afterForged}`);
    expect(afterForged).toBe(1);

    const refusal = await env.DB.prepare(
      `SELECT reason FROM crypto_settlement_refusals
        WHERE invoice_id_hash = ?
        ORDER BY attempted_at DESC
        LIMIT 1`,
    ).bind(await sha256Hex(forgedInvoice.invoice_id)).first<{ reason: string }>();
    console.log(`TASK3195 forged_refusal_reason=${refusal?.reason ?? "missing"}`);
    expect(refusal?.reason).toBe("bad-signature");
  });
});
