# Burn contract

Burn is local cryptographic erasure with three explicit scopes: the current
chat, one linked service account, or the entire active OSL identity. Every burn
requires review and confirmation of the exact scope and options. Changing an
option invalidates that confirmation.

A completed local burn destroys local decryption capability, key mappings, and
caches for the selected scope. The user may also choose to forget locally cached
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
