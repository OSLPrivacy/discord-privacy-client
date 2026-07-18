# Offline controls and OSL opened receipts

Burn, expiry, and opened-receipt controls use bounded opaque signed records.
They contain commitments and timing/replay metadata only—no message plaintext,
service name, account handle, or chat title. The encrypted local queue retries
after reconnect; the receiver journal makes identical delivery idempotent and
rejects control-id or nonce reuse. Neither structure may silently evict live
records when full.

The default timed-message clock begins at the first authenticated local
open/decrypt. A fixed absolute expiry is an advanced option and may make an
unread message unrecoverable. Expiry destroys local decrypt keys and plaintext
caches and requests deletion of OSL's encrypted blob. Native carrier history,
provider copies, screenshots, exports, and backups remain unchanged.

Opened receipts mean “authenticated local open in OSL,” never native-service
Seen. The requester/sender must have Pro. A Free recipient may participate, but
must explicitly allow receipts for that friend and chat and can revoke the
grant. Group state is tracked separately per recipient. Declined or unknown
permission is unavailable; an offline consenting recipient is pending. OSL
never infers an opened receipt from presence, focus, scrolling, or native UI.

These are domain contracts and tests. Network transport and UI are not wired.
