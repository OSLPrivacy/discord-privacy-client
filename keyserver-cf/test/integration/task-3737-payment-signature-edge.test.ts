import { env } from "cloudflare:test";
import { beforeAll, describe, expect, it } from "vitest";
import { handleCryptoQuote } from "../../src/endpoints/crypto-checkout.js";
import { dispatchForDormantPaymentTest } from "../../src/index.js";
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

async function changedSignatureHeaders(
  evidence: WatcherSettlementEvidence,
): Promise<Record<string, string>> {
  const headers = await settlementHeaders(evidence);
  const signature = Uint8Array.from(
    atob(headers["x-osl-settlement-signature"] ?? ""),
    (character) => character.charCodeAt(0),
  );
  signature[0] ^= 1;
  headers["x-osl-settlement-signature"] = base64(signature);
  return headers;
}

async function settlementEvidence(
  invoice: { invoice_id: string; amount_atomic: string; expires_at: number },
  paymentReferenceLabel: string,
): Promise<WatcherSettlementEvidence> {
  const paymentReferenceCommitment = await sha256Hex(
    `btc:${invoice.invoice_id}:task3737:${paymentReferenceLabel}`,
  );
  return {
    event_id: `evt_${await sha256Hex(
      `${invoice.invoice_id}:btc:${paymentReferenceCommitment}`,
    )}`,
    invoice_id: invoice.invoice_id,
    payment_method: "btc",
    amount_atomic: invoice.amount_atomic,
    confirmations: 2,
    observed_at: Math.min(invoice.expires_at - 1, Math.floor(Date.now() / 1000)),
    payment_reference_commitment: paymentReferenceCommitment,
  };
}

async function seedTodayPrice(): Promise<void> {
  const today = new Date().toISOString().slice(0, 10);
  await env.DB.prepare(
    `INSERT INTO crypto_price_snapshots (asset, snapshot_date, price_usd, fetched_at)
     VALUES ('btc', ?, '60000', strftime('%s','now'))
     ON CONFLICT(asset, snapshot_date) DO UPDATE SET
       price_usd = excluded.price_usd, fetched_at = excluded.fetched_at`,
  ).bind(today).run();
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
    CRYPTO_PRO_USD_CENTS: "500",
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

const testCtx = {
  waitUntil(promise: Promise<unknown>): void {
    void promise.catch(() => undefined);
  },
  passThroughOnException(): void {},
} as ExecutionContext;

async function quoteBuyer(
  ipOctet: number,
): Promise<{ invoice_id: string; amount_atomic: string; expires_at: number }> {
  const publicKey = await deliveryPublicKey();
  const response = await handleCryptoQuote(new Request("http://test/v1/crypto/quote", {
    method: "POST",
    headers: { "content-type": "application/json", "x-forwarded-for": `192.0.2.${ipOctet}` },
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
  expect(response.status, await response.clone().text()).toBe(200);
  return await response.json() as {
    invoice_id: string; amount_atomic: string; expires_at: number;
  };
}

async function settleAtEdge(
  evidence: WatcherSettlementEvidence,
  headers: Record<string, string>,
): Promise<Response> {
  return await dispatchForDormantPaymentTest(new Request(
    "http://test/v1/internal/crypto/settle",
    {
      method: "POST",
      headers,
      body: JSON.stringify(evidence),
    },
  ), checkoutEnv(), testCtx, redemptionReady);
}

async function codeCount(invoiceIds: readonly string[]): Promise<number> {
  const placeholders = invoiceIds.map(() => "?").join(", ");
  const row = await env.DB.prepare(
    `SELECT COUNT(*) AS count FROM licenses
      WHERE subscription_id IN (${placeholders})`,
  ).bind(...invoiceIds.map((invoiceId) => `crypto_${invoiceId}`)).first<{ count: number }>();
  return row?.count ?? 0;
}

async function errorText(response: Response): Promise<string> {
  const body = await response.json() as { error?: unknown };
  return typeof body.error === "string" ? body.error : "";
}

beforeAll(async () => {
  await seedTodayPrice();
});

describe("TASK3737 payment settlement signature at the edge", () => {
  it("mints one code for a valid callback and none for changed or missing signatures", async () => {
    const validBuyer = await quoteBuyer(201);
    const changedSignatureBuyer = await quoteBuyer(202);
    const missingSignatureBuyer = await quoteBuyer(203);
    const invoiceIds = [
      validBuyer.invoice_id,
      changedSignatureBuyer.invoice_id,
      missingSignatureBuyer.invoice_id,
    ] as const;
    expect(new Set(invoiceIds).size).toBe(3);
    console.log(`TASK3737 buyer_invoice_ids=${invoiceIds.join(",")}`);

    const before = await codeCount(invoiceIds);
    console.log(`TASK3737 before_code_count=${before}`);
    expect(before).toBe(0);

    const validEvidence = await settlementEvidence(validBuyer, "valid");
    const valid = await settleAtEdge(validEvidence, await settlementHeaders(validEvidence));
    expect(valid.status, await valid.clone().text()).toBe(200);

    const afterValid = await codeCount(invoiceIds);
    console.log(`TASK3737 after_valid_code_count=${afterValid}`);
    expect(afterValid).toBe(1);

    const changedEvidence = await settlementEvidence(changedSignatureBuyer, "changed-signature");
    const changed = await settleAtEdge(
      changedEvidence,
      await changedSignatureHeaders(changedEvidence),
    );
    const changedError = await errorText(changed);
    console.log(`TASK3737 changed_signature_refusal=${changedError}`);
    expect(changed.status).toBe(401);
    expect(changedError).toContain("signature");

    const afterChanged = await codeCount(invoiceIds);
    console.log(`TASK3737 after_changed_signature_code_count=${afterChanged}`);
    expect(afterChanged).toBe(1);

    const missingEvidence = await settlementEvidence(missingSignatureBuyer, "missing-signature");
    const missingHeaders = await settlementHeaders(missingEvidence);
    delete missingHeaders["x-osl-settlement-signature"];
    const missing = await settleAtEdge(missingEvidence, missingHeaders);
    const missingError = await errorText(missing);
    console.log(`TASK3737 missing_signature_refusal=${missingError}`);
    expect(missing.status).toBe(401);
    expect(missingError).toContain("signature");

    const afterMissing = await codeCount(invoiceIds);
    console.log(`TASK3737 after_missing_signature_code_count=${afterMissing}`);
    expect(afterMissing).toBe(1);
  });
});
