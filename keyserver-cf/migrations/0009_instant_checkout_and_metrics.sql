-- Instant, browser-bound Stripe fulfillment plus aggregate operator metrics.
-- License plaintext is never stored. The browser supplies an RSA-OAEP public
-- key and a high-entropy claim capability before checkout; only ciphertext and
-- the capability hash reach D1.

-- Do not add a unique subscription index here. Older deployments may have
-- issued more than one code for a subscription, and an unconditional unique
-- index would make the production migration fail. Instant checkout retries
-- converge on the pre-generated license hash instead.

CREATE TABLE stripe_event_claims (
  event_id TEXT PRIMARY KEY,
  event_type TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('processing', 'completed')),
  claimed_at INTEGER NOT NULL,
  completed_at INTEGER
);

CREATE INDEX idx_stripe_event_claims_status_time
  ON stripe_event_claims (status, claimed_at);

-- Stripe does not guarantee delivery order. Keep the newest authoritative
-- subscription state even when it arrives before checkout completion.
CREATE TABLE stripe_subscription_observations (
  subscription_id TEXT PRIMARY KEY,
  customer_id TEXT,
  status TEXT NOT NULL CHECK (status IN (
    'PENDING', 'ACTIVE', 'CANCELLED', 'GRACE', 'REVOKED', 'EXPIRED'
  )),
  status_precedence INTEGER NOT NULL,
  current_period_end INTEGER,
  cancel_at_period_end INTEGER NOT NULL DEFAULT 0,
  event_created INTEGER NOT NULL,
  event_type TEXT NOT NULL
);

CREATE TABLE stripe_checkout_claims (
  session_id TEXT PRIMARY KEY,
  claim_hash TEXT NOT NULL UNIQUE,
  delivery_public_key_spki TEXT NOT NULL,
  encrypted_license TEXT NOT NULL,
  license_hash TEXT NOT NULL,
  subscription_id TEXT UNIQUE,
  status TEXT NOT NULL CHECK (status IN (
    'pending', 'delivery_ready', 'expired'
  )),
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  delivered_at INTEGER
);

CREATE INDEX idx_stripe_checkout_claims_expiry
  ON stripe_checkout_claims (expires_at, status);

-- One row means a user began an installer download. This deliberately avoids
-- IP addresses, user agents, cookies, fingerprints, and account identifiers.
CREATE TABLE download_events (
  event_id TEXT PRIMARY KEY,
  artifact TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE INDEX idx_download_events_created
  ON download_events (created_at);

-- Aggregate-safe facts extracted only from verified live Stripe webhooks.
-- Customer names, emails, card details, and payment-method data are excluded.
CREATE TABLE commerce_events (
  event_id TEXT PRIMARY KEY,
  event_type TEXT NOT NULL,
  stripe_object_id TEXT NOT NULL,
  amount_cents INTEGER NOT NULL DEFAULT 0,
  currency TEXT NOT NULL,
  occurred_at INTEGER NOT NULL,
  livemode INTEGER NOT NULL CHECK (livemode = 1)
);

CREATE INDEX idx_commerce_events_type_time
  ON commerce_events (event_type, occurred_at);

CREATE UNIQUE INDEX idx_commerce_events_type_object
  ON commerce_events (event_type, stripe_object_id);
