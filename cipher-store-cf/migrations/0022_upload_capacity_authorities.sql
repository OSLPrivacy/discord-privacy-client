-- TASK 5215: issuer-backed, identity-free upload capacity.
--
-- Reservations arrive independently of upload credentials.  The store keeps
-- no customer, processor, or other identity column.  A pending claim
-- holds capacity before the Class A write; acceptance moves that same debit to
-- spent.  Accepted claims are permanent accounting records, so deleting the
-- ciphertext can never replenish capacity.

CREATE TABLE upload_capacity_reservations (
  reservation_id TEXT PRIMARY KEY NOT NULL
    CHECK(length(reservation_id) = 32 AND reservation_id NOT GLOB '*[^0-9a-f]*'),
  grant_id TEXT UNIQUE NOT NULL
    CHECK(length(grant_id) = 32 AND grant_id NOT GLOB '*[^0-9a-f]*'),
  authority TEXT NOT NULL CHECK(authority IN ('monthly-included', 'sold')),
  capacity_bytes INTEGER NOT NULL CHECK(capacity_bytes > 0),
  spent_bytes INTEGER NOT NULL DEFAULT 0 CHECK(spent_bytes >= 0),
  base_expiry INTEGER NOT NULL,
  effective_expiry INTEGER NOT NULL CHECK(effective_expiry >= base_expiry),
  outage_extension_seconds INTEGER NOT NULL DEFAULT 0
    CHECK(outage_extension_seconds >= 0
      AND effective_expiry = base_expiry + outage_extension_seconds),
  signed_reservation TEXT UNIQUE NOT NULL,
  created_at INTEGER NOT NULL,
  CHECK(spent_bytes <= capacity_bytes)
) STRICT;

CREATE TABLE upload_capacity_claims (
  object_id TEXT PRIMARY KEY NOT NULL
    CHECK(length(object_id) = 32 AND object_id NOT GLOB '*[^0-9a-f]*'),
  grant_id TEXT NOT NULL REFERENCES upload_capacity_reservations(grant_id),
  debit_bytes INTEGER NOT NULL CHECK(debit_bytes >= 1000000),
  ciphertext_bytes INTEGER NOT NULL CHECK(ciphertext_bytes > 0),
  state TEXT NOT NULL CHECK(state IN ('pending', 'accepted')),
  class_a_writes INTEGER NOT NULL DEFAULT 0 CHECK(class_a_writes IN (0, 1)),
  claimed_at INTEGER NOT NULL,
  accepted_at INTEGER,
  CHECK((state = 'pending' AND accepted_at IS NULL)
     OR (state = 'accepted' AND accepted_at IS NOT NULL))
) STRICT;

CREATE INDEX idx_upload_capacity_claims_grant_state
  ON upload_capacity_claims(grant_id, state);
