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

- `_meta(key, value)` — `schema_version`, the sealed canary, and the
  sealed `meta_auth_strict` marker.
- `messages(discord_message_id, channel_id, sender_discord_id,
  sender_osl_user_id, ciphertext, nonce, decrypted_at, burned,
  burned_at, wrapped_key, scope_type, scope_id, meta_tag)`
  — row metadata is plaintext (Discord ids, OSL user ids,
  channel ids, the unix-seconds timestamp) so SQLite can run
  `WHERE channel_id = ? ORDER BY decrypted_at DESC`. The
  message body lives only in `ciphertext` and is never written
  unencrypted. `wrapped_key`, `scope_type` and `scope_id` are
  present but **no code in this repository has ever written them**;
  they are NULL on every row.
- `attachments(cache_key, discord_message_id, random_filename, mime,
  ciphertext, nonce, byte_len, created_at, scope_type, scope_id,
  sender_discord_id)` — the decrypted attachment bytes are sealed;
  the linkage, filename, MIME type, byte count and timestamp are
  plaintext.
- `idx_messages_channel` — the `(channel_id, decrypted_at DESC)`
  index used by `list_by_channel`. Indexes the same plaintext
  ids; no message body content.

**No FTS tables, no tokenized plaintext, no other plaintext
content surface.** Deliberately. (See "Search" below.)

### The metadata exposure, stated as a defect and not as a design note

Message *bodies* are sealed. The **social graph is not.** Someone who
reads this file offline — a disk thief, a backup, a forensic tool —
can enumerate, without ever holding the store key:

- which Discord accounts and which OSL identities are talking,
- which channels/conversations they are talking in,
- when each message was decrypted, and therefore activity timing,
- which messages carried attachments, of what type and what size,
- which conversations were burned and when.

Correlated against Discord's own local cache, that reconstructs the
protected relationship graph. This is the exposure the audit records
as *"Plaintext message/social metadata is persisted"*.

This file has always described that accurately. The **product-level
at-rest invariant does not**, and the two disagree. Until one of them
moves, this file is the one that matches the code — do not cite the
stronger claim.

Fixing the code rather than the claim means keyed-hash (blind) index
columns replacing the plaintext ids, plus a sealed metadata blob. That
is a rewrite of every installed row and is **not** something to start
without the owner's decision, because rolling back afterwards orphans
the user's history. The design and its blast radius are written up in
`docs/reports/store-lane-2026-07-26.md`; it is deliberately not
started.

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

## Row metadata authentication

Added 2026-07-26 in response to the audit finding that
security-relevant metadata sat outside AEAD authentication.

The body AAD is the row's `discord_message_id` only, so
`channel_id`, `sender_discord_id`, `sender_osl_user_id` and
`decrypted_at` were unauthenticated: someone who could write the
file could re-attribute a message to another person, or move it into
another conversation, and the AEAD tag still verified.

Each row now carries `meta_tag`, a 40-byte `nonce || tag`
authenticator produced by sealing an *empty* plaintext with the
canonical, length-prefixed encoding of those fields as AAD. It holds
no secret, so it adds nothing to what the file discloses. Both read
paths verify it and return `Corrupted` on mismatch.

Limits, stated plainly:

- Rows written before this existed carry no tag. They are accepted,
  because refusing them would erase an installed user's history on
  upgrade. Once a database holds no untagged unburned rows it latches
  a sealed strict marker and refuses untagged rows from then on, so a
  database created by this build is strict from birth.
- A writer who can edit the file can also delete that marker from
  `_meta`, downgrading the store to lenient. `_meta` is no better
  protected than the rest of the file. Closing this needs a
  whole-database MAC anchored outside SQLite.
- `burned` / `burned_at` are outside the tag. Flipping `burned`
  exposes nothing, because burn zeroes the body it would expose.
- Attachment rows are **not** covered. Their AAD is the `cache_key`,
  so `mime`, `byte_len`, `scope_type`, `scope_id` and
  `sender_discord_id` on those rows remain unauthenticated.

## Threading & concurrency

`MessageStore` holds a single `rusqlite::Connection` behind a
`Mutex`. Concurrent callers serialise at the lock; SQLite WAL
mode reduces write-lock contention with future readers if we
add multi-conn pooling later.
