-- F1.3 schema: crypto manual-payment flow.
--
-- The flow is form-submission style — user pays the displayed
-- address, fills a form with their txid + email, and we manually
-- confirm + issue a license. No on-chain monitoring (V2.1).
--
-- Prices are quoted at form-display time from the latest snapshot.
-- The snapshot table holds 1 row per day per asset (BTC, XMR).
-- Fetched daily by the 00:00 UTC cron; falls back to the prior
-- day when the day's fetch fails (CoinGecko outage etc.).

CREATE TABLE crypto_pending_payments (
  payment_id TEXT PRIMARY KEY,             -- our own UUID
  payment_method TEXT NOT NULL CHECK (payment_method IN ('btc', 'xmr')),
  plan TEXT NOT NULL CHECK (plan IN ('monthly', 'yearly')),
  amount_usd_cents INTEGER NOT NULL,
  amount_native TEXT NOT NULL,             -- string to preserve precision
  address TEXT NOT NULL,
  customer_email TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN (
    'quoted', 'awaiting', 'confirmed', 'expired', 'manually_resolved'
  )),
  txid TEXT,                               -- set when user submits, or admin overrides
  created_at INTEGER NOT NULL,
  resolved_at INTEGER
);
CREATE INDEX idx_crypto_status ON crypto_pending_payments (status);
CREATE INDEX idx_crypto_email ON crypto_pending_payments (customer_email);

CREATE TABLE crypto_price_snapshots (
  asset TEXT NOT NULL,                     -- 'btc' | 'xmr'
  snapshot_date TEXT NOT NULL,             -- 'YYYY-MM-DD' UTC
  price_usd TEXT NOT NULL,                 -- string for precision
  fetched_at INTEGER NOT NULL,
  PRIMARY KEY (asset, snapshot_date)
);
