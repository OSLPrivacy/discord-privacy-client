-- 0035: persist the ordinary-lane self-recycling count for each signed
-- control-inbox POST receipt.
--
-- The endpoint already returns explicit 429 JSON for durable recipient
-- backpressure. Successful same-sender recycling was still silent: a sender
-- could receive 201 without knowing the server had discarded one of its own
-- older queued rows to keep the per-pair lane bounded. This additive receipt
-- field makes the outcome replay-stable after a lost response.

ALTER TABLE control_inbox_requests
  ADD COLUMN inbox_eviction_count INTEGER NOT NULL DEFAULT 0
    CHECK (inbox_eviction_count >= 0);

INSERT INTO worker_schema_capabilities (capability, version)
VALUES ('control_inbox_eviction_signal', 1);
