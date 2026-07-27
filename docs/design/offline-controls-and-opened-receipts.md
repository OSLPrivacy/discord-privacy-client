# Offline controls and OSL opened receipts

> **Status: designed, NOT implemented (2026-07-26).** The per-message wrapped-key model described
> below is not what the shipping code does. `MessageStore::put` never populates `wrapped_key`
> (`crates/store/src/lib.rs:194-206`), so there is no per-message key to destroy and no key to
> withhold. Burn today is **state deletion**: local shredding, server-side deletion, and a
> cooperative request to the peer. Because messages are sealed to the recipient's long-term keys,
> the carrier retained by the connected service stays readable to any holder of that key material.
> Building this model is deliberately deferred. Read everything below as intended design, not as
> current behaviour, and never quote it as a capability. See
> `docs/design/osl-public-claim-allowlist.md` §D.

Burn, expiry, and opened-receipt controls use bounded opaque signed records (designed, not implemented).
They contain commitments and timing/replay metadata only—no message plaintext,
service name, account handle, or chat title. The encrypted local queue retries
after reconnect; the receiver journal makes identical delivery idempotent and
rejects control-id or nonce reuse. Neither structure may silently evict live
records when full.

The default timed-message clock begins at the first authenticated local
open/decrypt (designed, not implemented). A fixed absolute expiry is an advanced option and may make an
unread message unrecoverable. Expiry destroys local decrypt keys and plaintext
caches and requests deletion of OSL's encrypted blob (designed, not implemented). Native carrier history,
provider copies, screenshots, exports, and backups remain unchanged.

Opened receipts mean “authenticated local open in OSL,” never native-service
Seen. The requester/sender must have Pro. A Free recipient may participate, but
must explicitly allow receipts for that friend and chat and can revoke the
grant. Group state is tracked separately per recipient. Declined or unknown
permission is unavailable; an offline consenting recipient is pending. OSL
never infers an opened receipt from presence, focus, scrolling, or native UI.

These are domain contracts and tests. Network transport and UI are not wired.
