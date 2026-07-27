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

The blind-index key is a separate HKDF-SHA256 derivation from the
same `identity_secret`:

```
index_key = HKDF-SHA256(salt = "", ikm = identity_secret, info = "osl-message-store-index-v1")
blind_index = HKDF-SHA256(salt = index_key, ikm = field_value, info = per-field domain)
```

The per-field domains are distinct for message id, channel id,
sender id and attachment cache key. That prevents the same string
appearing in two roles from producing the same blind index.

A canary row in `_meta` (`canary_nonce`, `canary_ct`) holds a
fixed plaintext sealed under the same key. On open we attempt
to unseal; failure surfaces as `StoreError::Sealer` and the
caller is denied access without ever touching the message rows.
This catches the "wrong identity_secret" case at open time
rather than producing a misleading `Corrupted` on first read.

## What's on disk

Dumping `messages.sqlite` with `sqlite3 .schema` shows exactly:

- `_meta(key, value)` — `schema_version`, `canary_nonce` and
  `canary_ct`. A transient `vacuum_pending` marker also lives here
  between a committed migration and the `VACUUM` that follows it; it
  is deleted once that completes, and a store that crashed in between
  finishes the job on next open. See the migration section for what
  `VACUUM` does and does not do.
- `messages(mid_bi, chan_bi, sender_bi, meta_nonce, meta_ct,
  ciphertext, nonce, seq, burned, burned_at, wrapped_key)` —
  `mid_bi`, `chan_bi` and `sender_bi` are 32-byte keyed blind
  indexes. The real Discord message id, channel id, sender Discord
  id, sender OSL user id and `decrypted_at` timestamp live in
  `meta_ct`, sealed with `meta_nonce`. The message body lives in
  `ciphertext`. `seq` is an opaque monotonic ordering counter; it
  replaces the old plaintext `decrypted_at` ordering index.
- `attachments(ck_bi, mid_bi, sender_bi, meta_nonce, meta_ct,
  ciphertext, nonce, seq)` — `ck_bi`, `mid_bi` and `sender_bi` are
  blind indexes. The real cache key, message id, random filename,
  MIME type, byte length, creation timestamp, optional scope fields
  and optional sender id live in the sealed metadata blob. The
  attachment bytes live in `ciphertext`.
- `idx_messages_chan_seq` — `(chan_bi, seq DESC)`, used by
  `list_by_channel`.
- `idx_attachments_mid` — `mid_bi`, used to find/delete cached
  attachments for a message.
- `idx_attachments_seq` — `seq`, used to trim the oldest attachment
  rows.

**No FTS tables, no tokenized plaintext, no other plaintext
content surface.** Deliberately. (See "Search" below.)

### The metadata exposure, stated as a defect and not as a design note

Schema v4 fixes the old plaintext metadata exposure, but it does not
make the database opaque.

What is now hidden from an offline reader without the
`identity_secret`:

- Discord message ids.
- Discord channel ids.
- Sender Discord ids.
- Sender OSL user ids.
- Exact message and attachment timestamps.
- Attachment random filenames.
- Attachment MIME types.
- Attachment byte lengths as metadata fields.

What remains visible:

- The number of message rows and attachment rows.
- That two rows share a channel or sender, because blind indexes are
  deterministic. The value is hidden; equality is not.
- Frequency analysis over groups of equal blind indexes.
- The size of each sealed body. For messages this leaks message
  length; for attachments it leaks attachment size even though the
  sealed `byte_len` metadata field is hidden.
- The relative order of rows via `seq`.
- Which message rows are burned, and their `burned_at` value when set.

Plainly: the social graph's labels are gone; its shape is not. Do not
describe the social graph as protected without that qualification.

The old plaintext channel/time index is gone. `list_by_channel` looks
up the channel by `chan_bi` and sorts by `seq DESC`. `seq` exists
because sealing `decrypted_at` otherwise would force a full-channel
decrypt just to sort.

## Search

v1 has no search. The public API is `get` + `list_by_channel`
only. This is a deliberate privacy property: a forensic dump of
`messages.sqlite` reveals no message body content, only the remaining
non-content surfaces listed above.

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

Corrected 2026-07-26. The previous version of this section described
a `mark_burned` that only set a flag; it had not been true for some
time, and the section understated what the code does.

- Overwrites `ciphertext` and `nonce` with zeroes in place, and
  nulls `wrapped_key`.
- Sets `messages.burned = 1` so `get` returns `None` and
  `list_by_channel` filters the row out. The zeroed row remains as
  a burn-acknowledgment stub.
- Stamps `burned_at`, but only if it was not already set, so
  re-burning cannot make an old destruction look recent.
- Deletes that message's cached attachments. A burn that destroyed
  the text and left the decrypted picture on disk would not be a
  burn.
- Runs `PRAGMA wal_checkpoint(TRUNCATE)` so the pre-burn page images
  are not left recoverable beside the database, and **fails loudly**
  if a live reader prevents that checkpoint rather than reporting a
  completed shred.

It shreds unconditionally — including on a row already flagged
`burned = 1`. It used to return `Ok(())` the moment it saw that flag,
on the assumption that such a row had already been shredded. That
assumption was false for rows written by earlier builds, so the call
could report a completed destruction while the sealed body sat
intact on disk.

**Burn is terminal.** A subsequent `put` of the same
`discord_message_id` will not restore the row: the upsert carries
`WHERE messages.burned = 0`. This matters because re-`put` is
ordinary behaviour — the receive observer re-decrypts a channel's
history on re-entry — and before that predicate existed, one channel
re-entry after a burn wrote the sealed body straight back to disk.

### What burn still does not do

`secure_delete=ON` and the truncating checkpoint remove the obvious
copies, but this is not a guarantee against physical media
forensics: SSD wear-levelling and filesystem journaling may retain
pre-burn pages that SQLite can no longer address.

Burn is **local destruction only**. It destroys this device's cached
plaintext. It does not revoke anyone else's ability to decrypt the
ciphertext Discord still holds, because the send path wraps to
long-term recipient keys and no per-message key is held anywhere
revocable. See `docs/THREAT_MODEL.md` § "Revocability" — and do not
describe this as "cryptographic burn".

## Sealed metadata authentication

There is no `meta_tag` column and no `meta_auth_strict` latch in
schema v4. They are obsolete.

Metadata is hidden and authenticated by the same AEAD operation:
`meta_nonce` + `meta_ct` seal the canonical, length-prefixed metadata
blob. For message rows, the metadata AAD is the row's own `mid_bi`.
For attachment rows, the metadata AAD is the row's own `ck_bi`.

That means an offline editor who moves a metadata blob between rows,
or edits one in place, gets an AEAD tag failure on read. A separate
authenticator would be redundant because the sealed blob already
authenticates the metadata it hides.

The message body AAD is still the real `discord_message_id`, recovered
from sealed metadata before opening the body. That is why the v3 to v4
migration can leave every message body ciphertext untouched.

## Schema v4 migration

The v3 to v4 migration rewrites every row inside one SQLite
transaction. It builds v4 tables beside the old tables, computes blind
indexes, seals metadata, drops the old tables, renames the v4 tables,
recreates the indexes and then runs `VACUUM`.

For message rows, `ciphertext`, `nonce`, `burned`, `burned_at` and
`wrapped_key` are copied byte-for-byte. Message bodies are never
re-encrypted, because the body AAD remains `discord_message_id`.
For attachment rows, `ciphertext` and `nonce` are copied
byte-for-byte while the metadata is sealed into the new v4 shape.

`seq` is assigned in legacy `decrypted_at ASC, rowid ASC` order, so
newest-first channel listing stays in the same order after migration.
Attachment `seq` is assigned in legacy `created_at ASC, rowid ASC`
order.

The canary is verified before migration runs. A wrong
`identity_secret` therefore cannot trigger a destructive rewrite.

**What actually scrubs the old identifiers is `secure_delete=ON`, not
`VACUUM`.** This was measured rather than assumed: with both disabled, a
raw-file scan after migration still finds the old plaintext channel id;
with `secure_delete=ON` and `VACUUM` removed, it does not. SQLite zeroes
freed cell content at `DROP` time, so the identifiers are gone before the
file is ever rewritten. The `VACUUM` is defence-in-depth — it compacts
the file and covers residue that cell-level scrubbing may not reach, such
as overflow pages and freelist structure — and the `vacuum_pending`
marker exists so a crash cannot skip it. Do not describe `VACUUM` as the
thing that removes the identifiers.

## Threading & concurrency

`MessageStore` holds a single `rusqlite::Connection` behind a
`Mutex`. Concurrent callers serialise at the lock; SQLite WAL
mode reduces write-lock contention with future readers if we
add multi-conn pooling later.
