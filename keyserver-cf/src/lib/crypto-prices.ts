/// BTC/XMR price snapshots via Kraken's public spot ticker.
///
/// A five-minute cron refreshes both assets. Quotes fail closed when the
/// newest snapshot is more than fifteen minutes old, so an upstream price
/// outage cannot silently produce a materially stale invoice.

import type { Env } from "../env.js";

const KRAKEN_TICKER_URL =
  "https://api.kraken.com/0/public/Ticker?pair=XBTUSD%2CXMRUSD&assetVersion=1";

export interface PriceSnapshot {
  asset: "btc" | "xmr";
  snapshot_date: string; // YYYY-MM-DD
  price_usd: string; // string for precision
  fetched_at: number;
}

export const PRICE_REFRESH_SECONDS = 5 * 60;
export const MAX_PRICE_AGE_SECONDS = 15 * 60;
const MAX_FUTURE_SKEW_SECONDS = 60;

function todayUtcDate(): string {
  return new Date().toISOString().slice(0, 10);
}

/** Fetch and persist both assets. Repeated refreshes update the day's row. */
export async function refreshPriceSnapshots(
  env: Env,
  fetcher: typeof fetch = fetch,
): Promise<{ btc?: string; xmr?: string }> {
  let res: Response;
  try {
    res = await fetcher(KRAKEN_TICKER_URL, {
      method: "GET",
      headers: { accept: "application/json" },
      signal: AbortSignal.timeout(5_000),
    });
  } catch {
    console.error("[crypto-prices] Kraken fetch failed");
    return {};
  }
  if (!res.ok) {
    console.error(`[crypto-prices] Kraken failed (${res.status})`);
    return {};
  }
  let body: {
    error?: unknown;
    result?: Record<string, { a?: unknown; b?: unknown }>;
  };
  try {
    body = await res.json();
  } catch {
    console.error("[crypto-prices] Kraken returned non-JSON");
    return {};
  }
  if (!Array.isArray(body.error) || body.error.length !== 0 || !body.result) {
    console.error("[crypto-prices] Kraken returned an API error");
    return {};
  }
  const out: { btc?: string; xmr?: string } = {};
  const date = todayUtcDate();
  const now = Math.floor(Date.now() / 1000);
  const btcPrice = conservativeBid(body.result["BTC/USD"]);
  const xmrPrice = conservativeBid(body.result["XMR/USD"]);
  if (btcPrice) {
    const price = btcPrice;
    out.btc = price;
    await persistSnapshot(env.DB, {
      asset: "btc",
      snapshot_date: date,
      price_usd: price,
      fetched_at: now,
    });
  }
  if (xmrPrice) {
    const price = xmrPrice;
    out.xmr = price;
    await persistSnapshot(env.DB, {
      asset: "xmr",
      snapshot_date: date,
      price_usd: price,
      fetched_at: now,
    });
  }
  return out;
}

/** Require a sane two-sided market and value incoming coin at the bid, which
 * is conservative for a merchant that may later sell the received asset. */
function conservativeBid(ticker: { a?: unknown; b?: unknown } | undefined): string | null {
  if (!ticker || !Array.isArray(ticker.a) || !Array.isArray(ticker.b)) return null;
  const askText = ticker.a[0];
  const bidText = ticker.b[0];
  if (
    typeof askText !== "string" ||
    typeof bidText !== "string" ||
    !/^\d+(?:\.\d+)?$/.test(askText) ||
    !/^\d+(?:\.\d+)?$/.test(bidText)
  ) {
    return null;
  }
  const ask = Number(askText);
  const bid = Number(bidText);
  if (!Number.isFinite(ask) || !Number.isFinite(bid) || bid <= 0 || ask < bid) return null;
  if ((ask - bid) / bid > 0.05) return null;
  return bidText;
}

async function persistSnapshot(db: D1Database, snap: PriceSnapshot): Promise<void> {
  await db
    .prepare(
      `INSERT INTO crypto_price_snapshots (asset, snapshot_date, price_usd, fetched_at)
       VALUES (?, ?, ?, ?)
       ON CONFLICT(asset, snapshot_date) DO UPDATE SET
         price_usd = excluded.price_usd,
         fetched_at = excluded.fetched_at`,
    )
    .bind(snap.asset, snap.snapshot_date, snap.price_usd, snap.fetched_at)
    .run();
}

/** Return only a recent snapshot. Future-dated rows also fail closed. */
export async function getLatestSnapshot(
  db: D1Database,
  asset: "btc" | "xmr",
  nowSeconds = Math.floor(Date.now() / 1000),
): Promise<PriceSnapshot | null> {
  const row = await db
    .prepare(
      `SELECT asset, snapshot_date, price_usd, fetched_at
         FROM crypto_price_snapshots
        WHERE asset = ?
        ORDER BY fetched_at DESC
        LIMIT 1`,
    )
    .bind(asset)
    .first<PriceSnapshot>();
  if (!row) return null;
  const age = nowSeconds - Number(row.fetched_at);
  if (
    !Number.isSafeInteger(Number(row.fetched_at)) ||
    age < -MAX_FUTURE_SKEW_SECONDS ||
    age > MAX_PRICE_AGE_SECONDS
  ) {
    return null;
  }
  return row;
}

/** Convert USD cents → native asset amount (string, 8 dp for BTC,
 *  12 dp for XMR). Caller must round/format appropriately. */
export function usdCentsToNative(
  usdCents: number,
  priceUsdPerCoin: string,
  asset: "btc" | "xmr",
): string {
  const price = Number(priceUsdPerCoin);
  if (!Number.isFinite(price) || price <= 0) {
    throw new Error(`invalid price: ${priceUsdPerCoin}`);
  }
  const usd = usdCents / 100;
  const native = usd / price;
  const dp = asset === "btc" ? 8 : 12;
  return native.toFixed(dp);
}
