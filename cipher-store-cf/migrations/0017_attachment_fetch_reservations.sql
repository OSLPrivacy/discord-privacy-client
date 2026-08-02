-- The attachment lane has a longer fixed retry window because a complete
-- attachment may be a multipart/ranged transfer.  It is still one claim per
-- object, never one claim per HTTP range request.
ALTER TABLE attachment_objects
  ADD COLUMN single_fetch INTEGER NOT NULL DEFAULT 0
  CHECK(single_fetch IN (0, 1));

ALTER TABLE attachment_objects
  ADD COLUMN reserved_until INTEGER;

CREATE INDEX idx_attachment_objects_reservation
  ON attachment_objects(reserved_until)
  WHERE single_fetch = 1;
