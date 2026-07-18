-- Permit privacy-minimal custom one-time donations from $1 through $10,000.
-- SQLite cannot alter a CHECK constraint in place, so rebuild the table while
-- preserving its columns, primary key, unique constraint, and lookup index.

CREATE TABLE donation_events_bounded (
  donation_id TEXT PRIMARY KEY,
  provider TEXT NOT NULL CHECK (provider = 'stripe'),
  provider_reference TEXT NOT NULL UNIQUE,
  amount_usd_cents INTEGER NOT NULL CHECK (
    typeof(amount_usd_cents) = 'integer'
    AND amount_usd_cents >= 100
    AND amount_usd_cents <= 1000000
  ),
  currency TEXT NOT NULL CHECK (currency = 'usd'),
  occurred_at INTEGER NOT NULL
);

INSERT INTO donation_events_bounded (
  donation_id,
  provider,
  provider_reference,
  amount_usd_cents,
  currency,
  occurred_at
)
SELECT
  donation_id,
  provider,
  provider_reference,
  amount_usd_cents,
  currency,
  occurred_at
FROM donation_events;

DROP TABLE donation_events;
ALTER TABLE donation_events_bounded RENAME TO donation_events;

CREATE INDEX idx_donation_events_occurred
  ON donation_events (occurred_at);
