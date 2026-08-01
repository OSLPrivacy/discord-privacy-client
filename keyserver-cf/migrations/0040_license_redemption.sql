-- Prepaid redemption state. These columns are nullable so existing issued
-- licenses remain valid and unredeemed until an explicit redemption.
ALTER TABLE licenses ADD COLUMN redeemed_at INTEGER;
ALTER TABLE licenses ADD COLUMN grant_seconds INTEGER;
ALTER TABLE licenses ADD COLUMN expires_at INTEGER;
ALTER TABLE licenses ADD COLUMN redemption_binding TEXT;
