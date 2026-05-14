/// BTC/XMR price snapshot via CoinGecko free tier (no auth).
///
/// Daily 00:00 UTC cron fetches /api/v3/simple/price?ids=bitcoin,
/// monero&vs_currencies=usd, inserts one row per asset into
/// crypto_price_snapshots. Quote endpoint reads the most recent
/// snapshot (today preferred; falls back to the prior day on
/// fetch failure so a CoinGecko outage doesn't block the quote
/// form).

import type { Env } from "../env.js";

const COINGECKO_URL =
  "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin,monero&vs_currencies=usd";

export interface PriceSnapshot {
  asset: "btc" | "xmr";
  snapshot_date: string; // YYYY-MM-DD
  price_usd: string; // string for precision
  fetched_at: number;
}

function todayUtcDate(): string {
  return new Date().toISOString().slice(0, 10);
}

function yesterdayUtcDate(): string {
  const d = new Date();
  d.setUTCDate(d.getUTCDate() - 1);
  return d.toISOString().slice(0, 10);
}

/** Hit CoinGecko + persist. Idempotent: re-runs on the same date
 *  overwrite via ON CONFLICT. Logged + swallowed on failure so the
 *  cron handler doesn't crash; the prior day's snapshot remains
 *  available as fallback. */
export async function runDailyPriceSnapshot(
  env: Env,
  fetcher: typeof fetch = fetch,
): Promise<{ btc?: string; xmr?: string }> {
  let res: Response;
  try {
    res = await fetcher(COINGECKO_URL, { method: "GET" });
  } catch (err) {
    console.error("[crypto-prices] CoinGecko fetch failed:", err);
    return {};
  }
  if (!res.ok) {
    console.error(`[crypto-prices] CoinGecko ${res.status}: ${await res.text()}`);
    return {};
  }
  let body: { bitcoin?: { usd?: number }; monero?: { usd?: number } };
  try {
    body = await res.json();
  } catch {
    console.error("[crypto-prices] CoinGecko returned non-JSON");
    return {};
  }
  const out: { btc?: string; xmr?: string } = {};
  const date = todayUtcDate();
  const now = Math.floor(Date.now() / 1000);
  if (typeof body.bitcoin?.usd === "number") {
    const price = body.bitcoin.usd.toString();
    out.btc = price;
    await persistSnapshot(env.DB, {
      asset: "btc",
      snapshot_date: date,
      price_usd: price,
      fetched_at: now,
    });
  }
  if (typeof body.monero?.usd === "number") {
    const price = body.monero.usd.toString();
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

/** Return today's price if available, else yesterday's, else null. */
export async function getLatestSnapshot(
  db: D1Database,
  asset: "btc" | "xmr",
): Promise<PriceSnapshot | null> {
  const today = todayUtcDate();
  const yest = yesterdayUtcDate();
  // ORDER BY snapshot_date DESC + LIMIT 1 would also work; using a
  // bounded `IN (...)` keeps the index path simple.
  const row = await db
    .prepare(
      `SELECT asset, snapshot_date, price_usd, fetched_at
         FROM crypto_price_snapshots
        WHERE asset = ?
          AND snapshot_date IN (?, ?)
        ORDER BY snapshot_date DESC
        LIMIT 1`,
    )
    .bind(asset, today, yest)
    .first<PriceSnapshot>();
  return row ?? null;
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
