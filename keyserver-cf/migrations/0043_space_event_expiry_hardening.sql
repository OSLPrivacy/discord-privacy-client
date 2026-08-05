-- T21-C1 reserved keyserver migrations 0041..0043 for the ciphertext-only
-- Space lane: "event queue, invite capability state, and expiry/index
-- hardening". 0041 was written; this is 0043, the expiry/index hardening.
-- 0042 (invite capability state) is still unwritten and is NOT taken here.
--
-- D-273: `lease_until` is the reservation half of the reservation-plus-ACK
-- shape owner decision D15 requires ("delete on ACKNOWLEDGED receipt, never on
-- transmission"). The drain sets it instead of deleting, so a dropped response
-- delays redelivery instead of destroying the event; a row leaves storage only
-- on an acknowledgement or at `expires_at`.
--
-- T21-C5 prohibition UNCHANGED: this column is delivery state. It is not an
-- account identifier, a roster, a Space identifier, an event kind, or a sender
-- attribute, and none of those may ever be added here. It says when a delivery
-- attempt stops being visible -- nothing about who is in what Space.
ALTER TABLE space_event_queue ADD COLUMN lease_until INTEGER NOT NULL DEFAULT 0;

-- The drain's predicate is (recipient_tag, lease_until, created_at); without
-- this index it degrades to a scan of the tag's whole backlog.
CREATE INDEX IF NOT EXISTS idx_space_event_queue_lease
  ON space_event_queue(recipient_tag, lease_until, created_at);
