import { describe, expect, it, vi } from "vitest";
import type { Env } from "../../src/env.js";
import { createWatcherInvoice } from "../../src/lib/anonymous-crypto.js";

describe("crypto watcher transport", () => {
  it("rejects a non-HTTPS watcher before sending invoice metadata", async () => {
    const fetcher = vi.fn();
    const env = {
      CRYPTO_WATCHER_URL: "http://watcher.example",
      CRYPTO_WATCHER_REQUEST_SECRET: "s".repeat(32),
    } as Env;

    await expect(createWatcherInvoice(env, {
      invoice_id: `cpay_${"a".repeat(32)}`,
      payment_method: "btc",
      amount_atomic: "1000",
      expires_at: Math.floor(Date.now() / 1000) + 600,
    }, fetcher)).rejects.toThrow("crypto watcher URL must use HTTPS");
    expect(fetcher).not.toHaveBeenCalled();
  });
});
