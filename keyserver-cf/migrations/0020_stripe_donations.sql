-- Privacy-minimal one-time donations. A donation is keyed only by Stripe's
-- opaque PaymentIntent identifier; no donor name, email, Customer id, card
-- data, IP address, free-form message, account id, or entitlement is stored.

CREATE TABLE donation_events (
  donation_id TEXT PRIMARY KEY,
  provider TEXT NOT NULL CHECK (provider = 'stripe'),
  provider_reference TEXT NOT NULL UNIQUE,
  amount_usd_cents INTEGER NOT NULL CHECK (
    amount_usd_cents IN (500, 2000, 5000)
  ),
  currency TEXT NOT NULL CHECK (currency = 'usd'),
  occurred_at INTEGER NOT NULL
);

CREATE INDEX idx_donation_events_occurred
  ON donation_events (occurred_at);
