# `crates/store` security posture

This crate persists decrypted Discord messages to a local SQLite
file. It is one of two places in the v1 alpha codebase that holds
plaintext (the other being live in-memory state during decrypt
+ render). The privacy properties are described below; design
decisions that limit the at-rest surface are spelled out so users
on a duress-relevant threat model understand what's actually on
disk.

## At-rest encryption

Each `messages` row stores XChaCha20-Poly1305 ciphertext + a
per-row 24-byte random nonce. The data key is HKDF-SHA256
derived from the caller-supplied 32-byte `identity_secret`:

```
key = HKDF-SHA256(salt = "", ikm = identity_secret, info = "osl-message-store-v1")
```

The AAD for each row is its `discord_message_id` UTF-8 bytes,
binding the ciphertext to its row identity (an attacker who
shuffles `ciphertext` / `nonce` blobs across rows triggers AEAD
tag failure rather than recovering cross-row plaintext).

A canary row in `_meta` (`canary_nonce`, `canary_ct`) holds a
fixed plaintext sealed under the same key. On open we attempt
to unseal; failure surfaces as `StoreError::Sealer` and the
caller is denied access without ever touching the message rows.
This catches the "wrong identity_secret" case at open time
rather than producing a misleading `Corrupted` on first read.

## What's on disk

Dumping `messages.sqlite` with `sqlite3 .schema` shows exactly:

- `_meta(key, value)` — `schema_version` + the sealed canary.
- `messages(discord_message_id, channel_id, sender_discord_id,
  sender_osl_user_id, ciphertext, nonce, decrypted_at, burned)`
  — row metadata is plaintext (Discord ids, OSL user ids,
  channel ids, the unix-seconds timestamp) so SQLite can run
  `WHERE channel_id = ? ORDER BY decrypted_at DESC`. The
  message body lives only in `ciphertext` and is never written
  unencrypted.
- `idx_messages_channel` — the `(channel_id, decrypted_at DESC)`
  index used by `list_by_channel`. Indexes the same plaintext
  ids; no message body content.

**No FTS tables, no tokenized plaintext, no other plaintext
content surface.** Deliberately. (See "Search" below.)

## Search

v1 has no search. The public API is `get` + `list_by_channel`
only. This is a deliberate privacy property: a forensic dump of
`messages.sqlite` reveals no message body content, only the
metadata listed above.

The earlier prototype shipped a contentless FTS5 virtual table
(`messages_fts`) for full-text search. FTS5's "contentless" mode
still stores the full token stream on disk (it has to — that's
how MATCH queries work), so a forensic dump reconstructed every
word that had ever appeared in any decrypted message, in order,
per document. That tradeoff was rejected and FTS5 was removed
before the store wired into the IPC decrypt path.

**v1.5** plans: decrypt-and-scan. The store gains a
`scan_by_channel(channel, query)` method that walks the encrypted
`messages` rows, unseals each in memory, and runs a substring or
regex match. Plaintext lives only in the calling thread's memory
for the duration of the scan; nothing new lands on disk. Latency
scales linearly with the channel's history, which for two-peer
dogfood traffic is fine.

**v2** plans (only if v1.5 latency proves unworkable): blind-
indexed encrypted search. Tokens are HMAC'd under a search key
(separate HKDF derivation off `identity_secret`) before
insertion into an FTS-shaped index. MATCH queries hash query
terms with the same HMAC. The index then contains opaque hash
strings, not plaintext tokens. Tradeoff: the porter stemmer
isn't available, so search becomes case-sensitive exact-token.
Acceptable for v2.

## What `mark_burned` does

- Sets `messages.burned = 1` so `get` returns `None` and
  `list_by_channel` filters the row out.
- Leaves the encrypted `ciphertext` blob in place. Burned-
  then-recovered ciphertext is still secret without the data
  key — no plaintext recovery without the key — but the row is
  structurally present. v1 keeps it for the burn-acknowledgment
  audit trail; a future "burn-with-shred" mode could `UPDATE`
  the ciphertext to a fresh random blob.

`mark_burned` is **not** a crypto-shred. SQLite's WAL + b-tree
page reuse does not guarantee secure overwrite. A page-level
forensic recovery may still surface ciphertext fragments —
which, again, are still encrypted and useless without the
identity secret.

## Threading & concurrency

`MessageStore` holds a single `rusqlite::Connection` behind a
`Mutex`. Concurrent callers serialise at the lock; SQLite WAL
mode reduces write-lock contention with future readers if we
add multi-conn pooling later.
