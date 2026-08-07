-- 0048: account-scoped discovery card publish/take-back.
--
-- The public lookup tuple from 0047 stays blinded. These nullable owner columns
-- are only for the writer's own publish/take-back calls and are not returned by
-- the read endpoint.

ALTER TABLE discovery_cards ADD COLUMN writer_account_id TEXT;
ALTER TABLE discovery_cards ADD COLUMN writer_app_id TEXT;
ALTER TABLE discovery_cards ADD COLUMN writer_setting TEXT;

CREATE INDEX idx_discovery_cards_writer
  ON discovery_cards (writer_account_id);

INSERT INTO worker_schema_capabilities (capability, version)
VALUES ('blinded_discovery_card_publish_takeback', 1);
