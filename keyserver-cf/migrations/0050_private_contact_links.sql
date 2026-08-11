-- TASK 0310a: authoritative, expiring, revocable private contact links.
--
-- Bearer and revocation capabilities are stored only as SHA-256 digests.  The
-- encrypted/opaque contact bundle is released only by the conditional update
-- that wins the one terminal transition from `live`.
CREATE TABLE private_contact_links (
    link_sha256 TEXT PRIMARY KEY NOT NULL
        CHECK(length(link_sha256) = 64),
    revoke_sha256 TEXT NOT NULL UNIQUE
        CHECK(length(revoke_sha256) = 64),
    contact_bundle TEXT NOT NULL
        CHECK(length(contact_bundle) BETWEEN 1 AND 12288),
    issued_at_unix_seconds INTEGER NOT NULL,
    expires_at_unix_seconds INTEGER NOT NULL,
    terminal_state TEXT NOT NULL DEFAULT 'live'
        CHECK(terminal_state IN ('live', 'redeemed', 'revoked', 'expired')),
    terminal_at_unix_seconds INTEGER,
    CHECK(expires_at_unix_seconds > issued_at_unix_seconds),
    CHECK(expires_at_unix_seconds - issued_at_unix_seconds <= 86400),
    CHECK(
        (terminal_state = 'live' AND terminal_at_unix_seconds IS NULL)
        OR
        (terminal_state <> 'live' AND terminal_at_unix_seconds IS NOT NULL)
    )
);

CREATE INDEX idx_private_contact_links_live_expiry
    ON private_contact_links(terminal_state, expires_at_unix_seconds);
