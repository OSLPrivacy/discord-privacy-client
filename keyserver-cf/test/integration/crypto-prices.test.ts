import { env } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";
import {
  getLatestSnapshot,
  MAX_PRICE_AGE_SECONDS,
  refreshPriceSnapshots,
} from "../../src/lib/crypto-prices.js";
import type { Env } from "../../src/env.js";

describe("crypto price freshness", () => {
  beforeEach(async () => {
    await env.DB.prepare("DELETE FROM crypto_price_snapshots").run();
  });

  it("refreshes both positive prices and returns a current snapshot", async () => {
    const prices = await refreshPriceSnapshots(
      env as Env,
      async (_input, init) => {
        expect(init?.signal).toBeInstanceOf(AbortSignal);
        expect(init?.signal?.aborted).toBe(false);
        return Response.json({
          error: [],
          result: {
            "BTC/USD": { a: ["60010.0"], b: ["60000.0"] },
            "XMR/USD": { a: ["151.0"], b: ["150.0"] },
          },
        });
      },
    );
    expect(prices).toEqual({ btc: "60000.0", xmr: "150.0" });
    expect((await getLatestSnapshot(env.DB, "btc"))?.price_usd).toBe("60000.0");
    expect((await getLatestSnapshot(env.DB, "xmr"))?.price_usd).toBe("150.0");
  });

  it("rejects snapshots older than fifteen minutes", async () => {
    const now = 2_000_000_000;
    await env.DB.prepare(
      `INSERT INTO crypto_price_snapshots (asset, snapshot_date, price_usd, fetched_at)
       VALUES ('btc', '2033-05-18', '60000', ?)`,
    )
      .bind(now - MAX_PRICE_AGE_SECONDS - 1)
      .run();
    expect(await getLatestSnapshot(env.DB, "btc", now)).toBeNull();
  });

  it("accepts the fifteen-minute boundary but rejects future-dated rows", async () => {
    const now = 2_000_000_000;
    await env.DB.prepare(
      `INSERT INTO crypto_price_snapshots (asset, snapshot_date, price_usd, fetched_at)
       VALUES ('btc', '2033-05-18', '60000', ?)`,
    )
      .bind(now - MAX_PRICE_AGE_SECONDS)
      .run();
    expect(await getLatestSnapshot(env.DB, "btc", now)).not.toBeNull();

    await env.DB.prepare(
      "UPDATE crypto_price_snapshots SET fetched_at = ? WHERE asset = 'btc'",
    )
      .bind(now + 61)
      .run();
    expect(await getLatestSnapshot(env.DB, "btc", now)).toBeNull();
  });

  it("does not persist non-positive upstream prices", async () => {
    const prices = await refreshPriceSnapshots(
      env as Env,
      async () => Response.json({
        error: [],
        result: {
          "BTC/USD": { a: ["1"], b: ["0"] },
          "XMR/USD": { a: ["1"], b: ["-1"] },
        },
      }),
    );
    expect(prices).toEqual({});
    expect(await getLatestSnapshot(env.DB, "btc")).toBeNull();
    expect(await getLatestSnapshot(env.DB, "xmr")).toBeNull();
  });

  it("rejects crossed or implausibly wide markets and API errors", async () => {
    const badMarket = await refreshPriceSnapshots(
      env as Env,
      async () => Response.json({
        error: [],
        result: {
          "BTC/USD": { a: ["59000"], b: ["60000"] },
          "XMR/USD": { a: ["200"], b: ["100"] },
        },
      }),
    );
    expect(badMarket).toEqual({});

    const apiError = await refreshPriceSnapshots(
      env as Env,
      async () => Response.json({ error: ["EGeneral:Temporary lockout"], result: {} }),
    );
    expect(apiError).toEqual({});
  });
});
