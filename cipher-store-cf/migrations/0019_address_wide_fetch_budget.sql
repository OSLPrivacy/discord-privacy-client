-- TASK 6330: one exact address-wide budget for every ciphertext release path.
--
-- One row is one request admitted to an underlying payload store.  Rows are
-- deliberately short lived and the address is an HMAC, never plaintext.  A
-- request id and caller class are retained for the same one-window lifetime so
-- an operator can reconcile the gate with independent edge/R2 access logs.
CREATE TABLE IF NOT EXISTS fetch_budget_events (
  event_id TEXT PRIMARY KEY NOT NULL,
  address_key TEXT NOT NULL,
  request_id TEXT NOT NULL,
  caller TEXT NOT NULL CHECK (caller IN ('blob-fetch', 'attachment-fetch', 'link-fetch')),
  observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0)
);

CREATE INDEX IF NOT EXISTS idx_fetch_budget_events_address_time
  ON fetch_budget_events(address_key, observed_at_ms);

CREATE INDEX IF NOT EXISTS idx_fetch_budget_events_observed_at
  ON fetch_budget_events(observed_at_ms);
