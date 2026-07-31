# Burn contract

> **Status correction, 2026-07-26.** Burn is **not** cryptographic erasure today, and this
> document previously said it was. `MessageStore::put` never populates `wrapped_key`
> (`crates/store/src/lib.rs:194-206`), so the burn paths that set `wrapped_key = NULL` null a
> column that was already null. What burn does is **state deletion**: local shredding plus
> server-side deletion plus a cooperative request to the peer. The per-message wrapped-key model
> that *would* earn the word "cryptographic" is designed and **deliberately not built** (owner
> decision 2026-07-26: not now). The phrases "cryptographic burn", "destroys keys, not messages"
> and "permanent ciphertext" are banned — see `osl-public-claim-allowlist.md` §D.

Burn is local **state deletion** with three explicit scopes: the current
chat, one linked service account, or the entire active OSL identity. Every burn
requires review and confirmation of the exact scope and options. Changing an
option invalidates that confirmation.

A completed local burn shreds the stored ciphertext and nonce for the selected scope, marks those
rows burned so a later re-sync cannot resurrect them, and drops their cached attachments. That
local destruction is real and was hardened on 2026-07-26. What it does **not** do is destroy the
only decryption capability: because messages are sealed to the recipient's long-term keys, the
carrier still held by the connected service stays readable to any holder of that key material. The user may also choose to forget locally cached
incoming/member messages. Existing executors in `security.rs`, `broker.rs`, and
`identity_registry.rs` perform the destructive work; `burn_contract.rs`
normalizes authorization and honest result semantics around them.

Burn does **not** delete carrier messages from a native service. It cannot erase
provider retention, recipient copies, screenshots, exports, backups, or anything
copied earlier. Account burn may offer operating-system uninstall only afterward
as a separate, separately confirmed action.

Remote friend burn is Pro-only and off until explicitly enabled. Every affected
OSL identity must sign a time-bounded consent grant for the exact opaque scope.
Grants use signed opaque revocation records and monotonic revocation epochs;
revoked or expired consent fails closed. Each
notice is signed, scoped, nonce-bound, and replay checked. Notices carry opaque
commitments and authenticated metadata only—never plaintext, service names,
account handles, chat titles, or native message text.

Identical retries are idempotent. Reusing a burn identifier with another nonce,
or a nonce with another burn, is rejected. Replay journals are hard bounded and
fail closed instead of silently forgetting live replay state. Remote consent and transport are
contract-only today and are not wired to UI or network delivery.
