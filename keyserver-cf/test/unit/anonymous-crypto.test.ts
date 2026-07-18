import { describe, expect, it, vi } from "vitest";
import type { Env } from "../../src/env.js";
import { createWatcherInvoice } from "../../src/lib/anonymous-crypto.js";

describe("crypto watcher transport", () => {
  const invoiceId = `cpay_${"a".repeat(32)}`;
  const env = {
    CRYPTO_WATCHER_URL: "https://watcher.example",
    CRYPTO_WATCHER_REQUEST_SECRET: "s".repeat(32),
  } as Env;

  it("rejects a non-HTTPS watcher before sending invoice metadata", async () => {
    const fetcher = vi.fn();
    const insecureEnv = {
      CRYPTO_WATCHER_URL: "http://watcher.example",
      CRYPTO_WATCHER_REQUEST_SECRET: "s".repeat(32),
    } as Env;

    await expect(createWatcherInvoice(insecureEnv, {
      invoice_id: invoiceId,
      payment_method: "btc",
      amount_atomic: "1000",
      expires_at: Math.floor(Date.now() / 1000) + 600,
    }, fetcher)).rejects.toThrow("crypto watcher URL must use HTTPS");
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("requires the watcher to echo the exact invoice id", async () => {
    await expect(createWatcherInvoice(env, {
      invoice_id: invoiceId,
      payment_method: "btc",
      amount_atomic: "1000",
      expires_at: Math.floor(Date.now() / 1000) + 600,
    }, async () => Response.json({
      invoice_id: `cpay_${"b".repeat(32)}`,
      address: `bc1q${"q".repeat(38)}`,
    }))).rejects.toThrow("mismatched invoice id");
  });

  it("rejects wrong-network and malformed payment addresses", async () => {
    for (const [payment_method, address] of [
      ["btc", "tb1qtestnet0000000000000000000000000000000"],
      ["xmr", `9${"1".repeat(94)}`],
    ] as const) {
      await expect(createWatcherInvoice(env, {
        invoice_id: invoiceId,
        payment_method,
        amount_atomic: "1000",
        expires_at: Math.floor(Date.now() / 1000) + 600,
      }, async () => Response.json({ invoice_id: invoiceId, address })))
        .rejects.toThrow("invalid address");
    }
  });

  it("accepts mainnet-shaped BTC and XMR watcher responses", async () => {
    for (const [payment_method, address] of [
      ["btc", `bc1q${"q".repeat(38)}`],
      ["xmr", `8${"1".repeat(94)}`],
    ] as const) {
      await expect(createWatcherInvoice(env, {
        invoice_id: invoiceId,
        payment_method,
        amount_atomic: "1000",
        expires_at: Math.floor(Date.now() / 1000) + 600,
      }, async () => Response.json({ invoice_id: invoiceId, address })))
        .resolves.toEqual({ address });
    }
  });
});
