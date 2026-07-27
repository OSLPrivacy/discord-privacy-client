# `crates/store` security posture

This crate persists decrypted Discord messages to a local SQLite
file. It is one of two places in the v1 alpha codebase that holds
plaintext (the other being live in-memory state during decrypt
+ render). The privacy properties are described below; design
decisions that limit the at-rest surface are spelled out so users
on a duress-relevant threat model understand what's actually on
disk.

## At-rest encryption

The store master key is HKDF-SHA256 derived from the caller-supplied
32-byte `identity_secret`:

```
key = HKDF-SHA256(salt = "", ikm = identity_secret, info = "osl-message-store-v1")
```

Each live message and each live attachment gets its own unique random
32-byte content key. Bodies are sealed under those keys; the content
keys are separately sealed under the store master key. The master key
does not directly encrypt live message or attachment bodies.

Message body, wrapper and metadata use separate AAD domains and bind the
message selector plus `content_version`. Attachment metadata, body and
wrapper likewise use separate v6 domains and bind the attachment selector,
owning-message selector, stable `seq`, and `content_version`. The body also
commits to the canonical metadata (including MIME and byte length), and the
wrapper commits to both that metadata and the exact body nonce/ciphertext.
Cross-row, cross-type, owner, order, version, metadata and partial-envelope
transplants therefore fail authentication before bytes are returned.

Schema v7 keeps a separately encrypted and authenticated per-message
attachment manifest. It commits to the ordered attachment selectors, count,
content versions, metadata, body ciphertext and wrappers. For writes made by
v7, deletion of a whole expected row is corruption rather than a cache miss.
The manifest describes this local cache only; it is not an authoritative list
of attachments at Discord or another provider. Inventories created while
migrating older stores are explicitly marked incomplete because the old schema
cannot prove whether an already-absent row was never cached or deleted before
migration. It is honest from the observed migration snapshot forward, not
retroactively complete.

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

A **missing** canary is only accepted on a genuinely new file. If the
store already has a `schema_version` or a `messages` table, an absent
canary means it was removed, and `open` refuses. Treating that as
first-run would let anyone who can delete two `_meta` rows open the
database under a secret of their choosing — and because the migration
runs immediately afterwards, a legacy file would be rewritten with its
metadata sealed under the attacker's key while its message bodies
stayed under the owner's, committing a store neither secret can fully
open. The canary's two rows are also written in one transaction, so a
crash cannot leave a half-canary that bricks every later open.

## What's on disk

Dumping `messages.sqlite` with `sqlite3 .schema` shows exactly:

- `_meta(key, value)` — `schema_version`, `canary_nonce` and
  `canary_ct`. Transient `vacuum_pending` and
  `shred_checkpoint_pending` markers also live here
  between a committed migration and the `VACUUM` that follows it; it
  is deleted once that completes, and a store that crashed in between
  finishes the job on next open. See the migration section for what
  `VACUUM` does and does not do.
- `messages(mid_bi, chan_bi, sender_bi, meta_nonce, meta_ct,
  ciphertext, nonce, seq, burned, content_version,
  wrapped_key_nonce, wrapped_key)` —
  `mid_bi`, `chan_bi` and `sender_bi` are 32-byte keyed blind
  indexes. The real Discord message id, channel id, sender Discord
  id, sender OSL user id and `decrypted_at` timestamp live in
  `meta_ct`, sealed with `meta_nonce`. The message body lives in
  `ciphertext`; the per-record content-key envelope lives in
  `wrapped_key_nonce` / `wrapped_key`. `seq` is an opaque monotonic
  ordering counter; it replaces the old plaintext `decrypted_at`
  ordering index.
- `attachments(ck_bi, mid_bi, sender_bi, meta_nonce, meta_ct,
  ciphertext, nonce, seq, burned, content_version,
  wrapped_key_nonce, wrapped_key)` — `ck_bi`, `mid_bi` and `sender_bi` are
  blind indexes. The real cache key, message id, random filename,
  MIME type, byte length, creation timestamp, optional scope fields
  and optional sender id live in the sealed metadata blob. The
  attachment bytes live in `ciphertext` under the per-record content
  key; its master-key envelope lives in `wrapped_key_nonce` /
  `wrapped_key`. Burned rows are selector-only audit stubs with zeroed
  metadata/body nonces and ciphertexts and no wrapper.
- `attachment_manifests(mid_bi, complete, generation, nonce, ciphertext)` —
  one sealed local-cache inventory per live message or attachment owner.
  `complete = 1` is used for future writes; migrated inventories use
  `complete = 0`. The encrypted contents bind count, canonical order,
  selectors, versions and commitments to each live attachment envelope.
- `idx_messages_chan_seq` — `(chan_bi, seq DESC)`, used by
  `list_by_channel`.
- `idx_attachments_mid` — `mid_bi`, used to find/delete cached
  attachments for a message.
- `idx_attachments_seq` — `seq`, used to trim the oldest attachment
  rows.

Both content tables also retain nullable compatibility columns so the exact
last v3 reader reaches its explicit newer-schema refusal:

- `messages(discord_message_id, channel_id, sender_discord_id,
  sender_osl_user_id, decrypted_at, scope_type, scope_id, meta_tag)`.
- `attachments(cache_key, discord_message_id, random_filename, mime,
  byte_len, created_at, scope_type, scope_id, sender_discord_id)`.

Every compatibility value is `NULL` on schema-v8 writes and migrations. These
columns are not selectors or fallback storage.

### Raw SQLite/WAL field census

| Logical field | On-disk representation | Classification |
|---|---|---|
| Message body | `messages.ciphertext` + `nonce`, under a per-message DEK | Protection-required; authenticated ciphertext |
| Attachment bytes | `attachments.ciphertext` + `nonce`, under an independent per-attachment DEK | Protection-required; authenticated ciphertext |
| Filename, MIME, byte length | Inside attachment `meta_ct` | Protection-required; authenticated ciphertext |
| Message, channel, sender Discord, sender OSL/service identifiers | Inside message `meta_ct`; keyed `mid_bi`, `chan_bi`, `sender_bi` permit equality lookup | Labels are protection-required; deterministic equality/frequency is intentional queryable leakage |
| Attachment cache key, owner message, optional scope and sender identifiers | Inside attachment `meta_ct`; keyed `ck_bi`, `mid_bi`, optional `sender_bi` permit lookup/burn | Labels are protection-required; deterministic equality/frequency is intentional queryable leakage |
| Message `decrypted_at`, attachment `created_at` | Inside the respective `meta_ct` | Protection-required; authenticated ciphertext |
| Relative insertion/order | Plain `seq` | Intentional queryable leakage needed for listing and cache trimming |
| Terminal state | Plain `burned` bit | Intentional queryable state needed to make re-put fail closed |
| Exact burn time | Not stored in schema v8 | Protection-required; removed because no Store query used it |
| Per-record DEK wrappers | `wrapped_key_nonce` + `wrapped_key` | Protection-required opaque authenticated envelopes; null after burn |
| Attachment inventory | Manifest body in `ciphertext` + `nonce`; owning `mid_bi`, `complete`, and `generation` remain visible | Inventory entries are protected; owner equality, legacy-coverage state, and update generation remain integrity/structural leakage |
| Blind indexes | `mid_bi`, `chan_bi`, `sender_bi`, `ck_bi` | Keyed and domain-separated, but intentionally deterministic rather than encrypted |
| Recovery/version metadata | Plain `_meta.key`; opaque or fixed-format `_meta.value` for `schema_version`, canary nonce/ciphertext, and transient vacuum/checkpoint markers | Operational metadata, not user content; schema/recovery state is visible |
| Row count, ciphertext length, page/WAL allocation | SQLite structure and blob lengths | Intentional residual leakage; Store does not pad rows or hide database shape |

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
- Which message and attachment rows are burned.
- Manifest owner equality, legacy `complete` coverage, and update
  `generation`.
- Schema version and whether a vacuum/checkpoint recovery action is pending.

Plainly: the social graph's labels are gone; its shape is not. Do not
describe the social graph as protected without that qualification.

The old plaintext channel/time index is gone. `list_by_channel` looks
up the channel by `chan_bi` and sorts by `seq DESC`. `seq` exists
because sealing `decrypted_at` otherwise would force a full-channel
decrypt just to sort.

## Rollback detection: external anchor required

SQLite authentication detects tampering, not a coherent restoration of an
older authenticated database. `MessageStore::open` remains compatible with
existing callers and **does not claim rollback protection**.

`MessageStore::open_anchored` is the explicit protected mode. It requires a
caller-provided `MonotonicAnchor` whose compare-and-advance state survives a
SQLite restore (for example a keystore/TPM-backed counter). The Store derives
a domain-separated opaque store identity and canonical state digest from the
identity secret, records a generation/digest with each committed mutation, and
requires the provider to advance before reporting success. A crash between the
SQLite commit and provider advance is recoverable only for the exact next
generation; a database behind, ahead by more than one generation, or with a
different digest is refused.

The provider is deliberately **implemented-unwired**: this crate supplies no
file-backed fallback, because a second file beside `messages.sqlite` can be
replayed with the database. Until the keystore/platform owner wires a durable
provider and production callers select `open_anchored`, this is test-proven
Store machinery, not shipping rollback protection. It does not protect OS
root compromise, backups held by an attacker, or a provider that itself rolls
back.

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
  nulls both `wrapped_key_nonce` and `wrapped_key`.
- Sets `messages.burned = 1` so `get` returns `None` and
  `list_by_channel` filters the row out. The zeroed row remains as
  a burn-acknowledgment stub.
- Retains only the terminal `burned` bit. Schema v8 no longer stores an exact
  burn timestamp.
- In the same SQLite transaction, zeroes every associated attachment's
  metadata/body ciphertext and nonce, nulls both attachment wrapper
  columns, and marks the selector-only attachment stubs burned. A burn
  that destroyed the text and left a decryptable picture would not be a
  burn.
- Deletes the selected attachment manifest in that same transaction. An
  unrelated message's manifest and attachment rows are not rewritten.
- Runs `PRAGMA wal_checkpoint(TRUNCATE)` so the pre-burn page images
  are not left recoverable beside the database, and **fails loudly**
  if a live reader prevents that checkpoint rather than reporting a
  completed physical shred. The transaction writes
  `shred_checkpoint_pending`; if the process dies after commit but before
  truncation, reopen retries the checkpoint before normal use.

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
Attachment puts have the same terminal rule because their zeroed stubs
retain the keyed selector.

### What burn still does not do

`secure_delete=ON` and the truncating checkpoint remove the obvious
copies, but this is not a guarantee against physical media
forensics: SSD wear-levelling and filesystem journaling may retain
pre-burn pages that SQLite can no longer address.

There is also no external monotonic anchor. Restoring a complete older
backup of `messages.sqlite` together with its internally consistent rows
can restore pre-burn wrappers, manifests, or an older same-row version. The
v6/v7 AEAD constructions reject partial stale replay and mixed-version
transplants,
not a full authenticated database rollback. Closing that requires state
outside the rollback domain (for example a hardware-backed or remote
monotonic floor).

The same limit applies to a coherent saved copy of one message's manifest
together with every attachment row that manifest authenticates: without an
external generation floor, the store cannot distinguish that internally valid
older set from the set that was current before the files were edited. Tests
therefore prove partial row/manifest replay refusal; they do not claim
store-only rollback resistance.

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
For v6 attachment rows, metadata AAD binds record type, `ck_bi`,
`mid_bi`, `seq`, and `content_version`.

That means an offline editor who moves a metadata blob between rows,
or edits one in place, gets an AEAD tag failure on read.

**The selector columns are not covered by that AEAD.** `chan_bi` and
`sender_bi` are separate columns and no tag binds them, so sealing the
metadata does not by itself stop someone with write access from
*retargeting* a row — surfacing a message in a conversation it was
never part of, or rewriting `sender_bi` so a sender-scoped burn skips
it. They still cannot read it. The read path therefore re-derives both
selectors from the unsealed metadata and rejects the row on mismatch,
which needs no format change because the plaintext identifiers and the
index key are both in hand after unsealing. A separate
authenticator would be redundant because the sealed blob already
authenticates the metadata it hides.

In schema v5, message metadata, wrapped key and body are independently
authenticated against the row's blind-index selector and content
version. The v4 to v5 migration therefore re-encrypts live message
bodies under fresh content keys.

In schema v6, the analogous attachment envelope additionally commits
to canonical metadata and exact body ciphertext as described above.
Selector recomputation after metadata authentication remains
defence-in-depth and protects selector columns such as `sender_bi`.

## Schema v4 through v8 migrations

The v3 to v4 migration rewrites every row inside one SQLite
transaction. It builds v4 tables beside the old tables, computes blind
indexes, seals metadata, drops the old tables, renames the v4 tables,
recreates the indexes and then runs `VACUUM`.

The subsequent v4 to v5 migration atomically rebuilds `messages`,
decrypts each live v4 body under the verified master key, generates a
fresh content key, re-encrypts the body, and stores the authenticated
wrapper. A live legacy row with a non-NULL wrapper is refused
byte-for-byte before migration because those bytes have no
authenticated format marker. Burned rows remain zeroed and wrapperless.
The v5 to v6 migration atomically rebuilds `attachments`. It
authenticates and decrypts every legacy direct-master body, verifies its
sealed selectors and byte length, gives it a new random content key, and
stores the v6 envelope. The schema stamp changes only in the same
transaction as the table replacement. Duplicate selectors or any
malformed row abort and restore the complete v5 logical state. A
pre-v6 table carrying non-empty or structurally partial attachment
wrapper columns is refused before persistent pragmas because those bytes
have no trustworthy format history.

The v6 to v7 migration creates all observed attachment manifests and stamps
the schema in one `BEGIN IMMEDIATE` transaction. It authenticates every live
v6 attachment before recording it. A failure therefore leaves the complete v6
database intact, never a partial manifest set. Migrated coverage is always
`complete = 0`; absence before the migration is not promoted into proof that
an attachment never existed.

The v7 to v8 migration removes `burned_at` from both content tables and stamps
the new version in one `BEGIN IMMEDIATE` transaction. It records
`vacuum_pending` before commit and completes the scrub on the same open; if the
process stops after commit, the next open retries the vacuum. Message bodies,
attachment envelopes, manifests, selectors, ordering, and terminal flags are
otherwise unchanged.

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

An external SQLite reader can retain a pre-burn snapshot until its read
transaction ends. The burn transaction still presents only the complete
pre-burn or complete post-burn row set; it never exposes a half-shredded
message/attachment set. While that reader is active, WAL truncation
fails by name and leaves the durable recovery marker for retry.
