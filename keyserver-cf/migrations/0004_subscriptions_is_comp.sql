-- Comped / manually-minted subscription marker.
--
-- Problem: comped/beta subscriptions (scripts/mint-beta-keys.ts) are
-- minted as status='ACTIVE' and never receive a Stripe webhook, so
-- they never transition to CANCELLED/GRACE. The hourly sweepExpired
-- cron only promotes CANCELLED/GRACE rows, so a --days 90 beta key
-- stays ACTIVE and valid forever past its current_period_end.
--
-- This column lets the cron expire comped ACTIVE rows past their
-- period end WITHOUT touching Stripe-originated ACTIVE rows (which
-- can briefly hold a past current_period_end during normal renewal
-- lag and must never be swept — Stripe's webhook owns their expiry).
--
--   0 = Stripe-originated (or crypto-paid). Sweep must NOT touch
--       these while ACTIVE.
--   1 = comped / manually-minted. Sweep promotes to EXPIRED once
--       current_period_end has passed.
--
-- DEFAULT 0 so every existing row is correctly treated as a
-- non-comped (Stripe/crypto) subscription. NOT NULL because the
-- sweep predicate must never see NULL.

ALTER TABLE subscriptions
  ADD COLUMN is_comp INTEGER NOT NULL DEFAULT 0;
