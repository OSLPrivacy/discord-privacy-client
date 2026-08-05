/// D-259 — the `*/5 * * * *` scheduled branch, executed for real.
///
/// Before this file nothing ran the five-minute branch, which is the only
/// reason the suite was hermetic: `src/index.ts` called `refreshPriceSnapshots`
/// with the default real `fetch`. That function swallows every fetch failure
/// and returns `{}`, so a test written against the real network would have
/// PASSED on a DNS failure without reaching its assertion.
///
/// Every assertion here is therefore starved-input-proof: each one is on a row
/// that only exists if the response was actually fetched, parsed and persisted.
/// If the fetcher never runs, or throws, or returns junk, these go red — they
/// cannot be satisfied by the swallow-and-return-`{}` path.

import { env } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";
import worker from "../../src/index.js";
import { getLatestSnapshot } from "../../src/lib/crypto-prices.js";
import type { Env } from "../../src/env.js";

const FIVE_MINUTE_CRON = "*/5 * * * *";
const HOURLY_CRON = "17 * * * *";

function controller(cron: string): ScheduledController {
  return {
    cron,
    scheduledTime: Date.now(),
    noRetry() {},
  } as ScheduledController;
}

interface FetcherLog {
  urls: string[];
  fetcher: typeof fetch;
}

/** A fetcher that records every call and answers with a valid Kraken ticker. */
function recordingKrakenFetcher(
  body: unknown = {
    error: [],
    result: {
      "BTC/USD": { a: ["60010.0"], b: ["60000.0"] },
      "XMR/USD": { a: ["151.0"], b: ["150.0"] },
    },
  },
): FetcherLog {
  const urls: string[] = [];
  const fetcher = (async (input: RequestInfo | URL) => {
    urls.push(typeof input === "string" ? input : String(input));
    return Response.json(body);
  }) as unknown as typeof fetch;
  return { urls, fetcher };
}

async function runScheduled(
  cron: string,
  priceFetcher?: typeof fetch,
): Promise<void> {
  await worker.scheduled(
    controller(cron),
    env as Env,
    {} as ExecutionContext,
    { priceFetcher },
  );
}

describe("scheduled five-minute price refresh (D-259)", () => {
  beforeEach(async () => {
    await env.DB.prepare("DELETE FROM crypto_price_snapshots").run();
  });

  it("executes the */5 branch and persists both fetched prices", async () => {
    const log = recordingKrakenFetcher();

    await runScheduled(FIVE_MINUTE_CRON, log.fetcher);

    // The branch really ran, and it really called out (once) to Kraken.
    expect(log.urls).toHaveLength(1);
    expect(log.urls[0]).toContain("api.kraken.com");

    // These rows exist ONLY if the response was parsed and persisted. On a
    // swallowed fetch error `refreshPriceSnapshots` returns `{}` and writes
    // nothing, so this assertion cannot be satisfied by the failure path.
    expect((await getLatestSnapshot(env.DB, "btc"))?.price_usd).toBe("60000.0");
    expect((await getLatestSnapshot(env.DB, "xmr"))?.price_usd).toBe("150.0");
  });

  it("fails closed rather than persisting a price when the fetch throws", async () => {
    let called = 0;
    const throwing = (async () => {
      called += 1;
      throw new TypeError("network unreachable");
    }) as unknown as typeof fetch;

    // The scheduled handler must absorb the failure (crons are retried), but
    // it must NOT invent a snapshot.
    await runScheduled(FIVE_MINUTE_CRON, throwing);

    expect(called).toBe(1);
    expect(await getLatestSnapshot(env.DB, "btc")).toBeNull();
    expect(await getLatestSnapshot(env.DB, "xmr")).toBeNull();
  });

  it("does not refresh prices on a cron that is not */5", async () => {
    const log = recordingKrakenFetcher();

    await runScheduled(HOURLY_CRON, log.fetcher);

    expect(log.urls).toEqual([]);
    expect(await getLatestSnapshot(env.DB, "btc")).toBeNull();
  });

  it("rejects an implausibly wide market instead of quoting it", async () => {
    const log = recordingKrakenFetcher({
      error: [],
      result: {
        // 10% spread — past the 5% ceiling in `conservativeBid`.
        "BTC/USD": { a: ["66000.0"], b: ["60000.0"] },
        "XMR/USD": { a: ["151.0"], b: ["150.0"] },
      },
    });

    await runScheduled(FIVE_MINUTE_CRON, log.fetcher);

    expect(log.urls).toHaveLength(1);
    expect(await getLatestSnapshot(env.DB, "btc")).toBeNull();
    // The good side of the same response still lands, which proves the run
    // reached the persist stage rather than bailing out early.
    expect((await getLatestSnapshot(env.DB, "xmr"))?.price_usd).toBe("150.0");
  });
});
