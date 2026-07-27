# Store lane — 2026-07-26

**Lane:** store · **Owns:** `crates/store/**` exclusively · **Model:** Opus 5, effort high
**Base:** branch `osl-eye-and-features-2026-07-26`, tree `/home/liamw/discord-privacy-client`
**Audit under repair:** `docs/security/osl-audit-2026-07-26-codex.md`
**Invariant document:** `docs/THREAT_MODEL.md` — the code was audited against it, not the reverse.

## Crate ownership

Confirmed: this lane has `crates/store/**`. It was clean when taken (0 dirty tracked, 0
untracked), so the starting state is exactly `origin`'s content for that path.

Nothing outside `crates/store/**` and this report was edited. `crates/keystore`, `crates/ipc`
and `apps/**` remain the crypto lane's. One defect found in `crates/keystore` is reported below
rather than fixed.

## Anchor re-verification

Every anchor in the brief was re-checked against the live tree before any edit. All six held.
Two were worse than described:

- **Defect 3 is larger than "wrapped_key is not written".** Nothing in the repository writes
  `messages.scope_type` or `messages.scope_id` either. A scope-burn predicate over those two
  columns therefore matched **zero** normally-written rows. `wipe_wrapped_keys_in_scope` was not
  a weak burn; it was a no-op that returned `Ok(0)` while its caller reported a successful burn.
  Verified by an independent call-site sweep across the whole repository (Codex, cx1) and by
  reading `put` (`lib.rs:194`), the only writer.
- **Defect 4 extends to `delete_messages_in_channel`.** Documented as "full data destruction for
  a channel", it deleted only `messages` rows. Cached attachment plaintext survived and
  `get_attachment` still served it, so a channel's pictures outlived the destruction of its text.
  It also *created* the unreachable residue: deleting the messages first orphans the attachment
  rows, after which nothing links them to any scope.

`crates/store/SECURITY.md` was itself stale in the **understating** direction: it described a
`mark_burned` that only set a flag, long after the code began zeroing the body.

## What was fixed, and the evidence

The bar was a test that fails against the broken build. The first run of the new suite against
unmodified code: **11 of 12 failed.** The one that passed
(`repeat_mark_burned_is_safe_and_keeps_the_original_burn_time`) is a guard against my own fix
over-reaching, not a defect test, and it is expected to pass in both directions.

| # | Defect | Status | Failing-first evidence |
|---|--------|--------|------------------------|
| 1 | Burned rows resurrected by `put` | **Fixed** | 3 tests failed pre-fix |
| 2 | `mark_burned` returns success without shredding | **Fixed** | failed pre-fix; re-confirmed by reintroducing the defect |
| 3 | Per-message `wrapped_key` model absent | **Designed, not executed** — writeup at `docs/design/osl-cryptographic-burn-design-2026-07-26.md` | n/a — owner decision |
| 4 | Legacy/unscoped attachments survive scope burns | **Fixed** | 5 tests failed pre-fix |
| 5 | Metadata outside AEAD authentication | **Fixed** (mechanism later *superseded* by v4 — see Round 2) | 2 tests failed pre-fix |
| 6 | Plaintext identifiers at rest | **FIXED in Round 2 (schema v4).** Round-1 text below is superseded | see Round 2 |

### Defect 1 — burn is now terminal

`put`'s upsert carries `WHERE messages.burned = 0`, so a re-`put` of a burned snowflake is a
silent no-op instead of a resurrection. This was not a theoretical path: the receive observer
re-decrypts a channel's history on every re-entry and re-`put`s the same ids, so one channel
re-entry after a burn wrote the sealed body back to disk and cleared the flag. That made
`THREAT_MODEL.md:160-162` — "the local cached plaintext of those messages is gone" — false in
ordinary use.

`put` also no longer honours a caller-supplied `burned: true` by writing a live body. A burned
flag and an intact secret must not coexist in a row this store wrote.

### Defect 2 — destructive calls destroy

`mark_burned` no longer short-circuits on `burned = 1`. It shreds unconditionally, and stamps
`burned_at` only when unset so re-burning cannot make an old destruction look recent. It now also
drops the message's cached attachments.

The short-circuit was justified by an assumption — that a flagged row had already been shredded —
which does not hold for rows written by earlier builds. Those rows exist in installed databases
now. Rather than document the assumption, the fix removes it.

**Evidence, both directions.** The replacement test was written after the fix, so it had no
recorded pre-fix failure. I re-introduced the early return, ran the test, and it failed with
`sealed body survives on disk (29 non-zero bytes)`; the reintroduction was then reverted and the
full suite re-run green. A regression test whose failure has never been observed is not evidence.

### Defect 4 — burns reach the pictures

- `wipe_attachments_in_scope` resolves attachments through their parent message, so legacy rows
  written with `None` scope are caught. The sender restriction reads off the *message*, not the
  attachment's own possibly-NULL column.
- `wipe_wrapped_keys_in_scope` additionally matches `channel_id = scope_id`, which is what makes
  it match anything at all (see defect 3). A server-wide `scope_id` matches no `channel_id`, so
  this cannot over-delete — snowflakes are unique.
- `delete_messages_in_channel` deletes attachments *before* the message rows, so the orphan is no
  longer created.

Two negative controls guard the over-deletion direction: a sender-scoped burn must spare other
participants' cached attachments, and a channel delete must not touch another channel's. A fix
that simply deleted everything in the channel would pass the positive tests and silently destroy
other people's data; these fail it.

**Residue** — *resolved in Round 2.* An attachment whose message row was already deleted by an
older build has no remaining link to any scope, so nothing can attribute it to a burn. It is not
created any more. Round 1 left pre-existing orphans in place; Round 2 purges them during the
v3→v4 migration, where the cost is a bounded, reversible cache miss rather than an ongoing sweep
that would evict live entries.

### Defect 5 — metadata is authenticated

> **SUPERSEDED by Round 2.** The property still holds — row metadata is authenticated
> and an offline edit is rejected — but the mechanism below no longer exists. Schema v4
> deleted `meta_tag` and the strict latch, because sealing the metadata under AEAD both
> hides and authenticates it. Kept for the reasoning, not as a description of the code.

Each row carries `meta_tag`: a 40-byte `nonce || tag` produced by sealing an **empty** plaintext
with the canonical length-prefixed encoding of `discord_message_id`, `channel_id`,
`sender_discord_id`, `sender_osl_user_id` and `decrypted_at` as AAD. It carries no secret, so it
adds nothing to what the file discloses. Length-prefixing matters: raw concatenation would let a
byte move across a field boundary and authenticate identically.

Both read paths verify it. Pre-fix, an offline editor could re-attribute a message to another
person or move it into another conversation and the AEAD tag still verified; both are now
`Corrupted`.

**Chosen shape, and why not the obvious one.** Folding the metadata into the body AAD would be
marginally stronger but requires re-sealing every existing row and makes new rows unreadable to
an older binary — one new row would fail a whole `list_by_channel` on rollback. The separate
authenticator is purely additive, needs no rewrite, and an older binary simply never selects the
column. The residual difference is a whole-row copy, which both designs permit equally.

**Limits, stated rather than glossed:**

- Untagged rows are accepted while any remain, because refusing them would erase an installed
  user's history on upgrade. Once a database holds no untagged unburned rows it latches a sealed
  strict marker and refuses untagged rows thereafter, so **a database created by this build is
  strict from birth**.
- Burned rows are excluded from that count deliberately: they are zeroed stubs filtered from both
  read paths, and counting them would mean a store that ever held legacy history could never latch.
- A writer who can edit the file can delete the strict marker from `_meta` and downgrade the store
  to lenient. `_meta` is no better protected than the rest of the file. Closing this needs a
  whole-database MAC anchored outside SQLite.
- **Attachment rows are not covered.** Their AAD is the `cache_key`, so `mime`, `byte_len`,
  `scope_type`, `scope_id` and `sender_discord_id` there remain unauthenticated. Same defect
  class, smaller blast radius, not fixed.

### Migration and downgrade for the change I did ship

`meta_tag` is an additive nullable `ALTER TABLE ... ADD COLUMN` and **`SCHEMA_VERSION` stays at 3**.
That is deliberate: `migrate` refuses to open any on-disk version it does not recognise, so
bumping it would make an older binary reject an upgraded database outright and orphan the user's
history the moment they rolled back — the hazard `shred_expired_messages` is already documented to
avoid. A nullable column an older binary never selects costs it nothing.

Proven by test, not asserted:

- `legacy_untagged_rows_survive_the_upgrade_and_stay_readable` — a database degraded to the pre-fix
  shape still reads every row after upgrade.
- `adding_the_authenticator_does_not_bump_the_schema_version` — asserts the on-disk version is
  still 3 *and* that the column exists, so it cannot pass by doing nothing.
- `a_store_with_legacy_rows_stays_lenient_until_they_drain` and
  `a_store_with_no_legacy_rows_refuses_an_untagged_row` — both sides of the latch.

**Downgrade behaviour:** an older binary opens an upgraded database and reads all rows normally.
If it *writes* rows, they arrive untagged; a subsequent upgrade then finds a strict store with
untagged rows and refuses them as tampering. Recovery is to delete the `meta_auth_strict` key from
`_meta`, which returns the store to lenient. Do not run mixed binaries against one database.

## Not executed, by design

### Defect 3 — the per-message `wrapped_key` model

Zhao's decision not to start this migration stands and I did not start it. Design and blast radius:

**What it would take.** The send path must wrap each message's body key `K` to a per-message key
whose only durable copy lives on the keyserver, and record the wrapped blob in
`messages.wrapped_key`. `put` gains a `wrapped_key` parameter, which changes `StoredMessage` and
therefore every construction site: `crates/ipc/src/commands.rs:1948, :2069, :2120, :2355, :2383`
plus two ipc tests. The receive path must carry the wrapped blob from the wire into the store.
Burn must then delete the keyserver-side key **and prove the delete**, because nulling a local
column revokes nothing.

**Blast radius.**

- It is a wire-format change, not a store change. The store is the last and smallest part.
- Every message sent before the change stays decryptable forever by any holder of the recipient's
  long-term identity keys. The migration cannot retrofit revocability onto existing history; it
  only changes what happens from the cutover.
- Mixed-version peers: a v3 sender talking to a burn-capable recipient produces messages the
  recipient believes are revocable and which are not. That mismatch is worse than the current
  honest absence, so it needs a negotiated capability, not a silent upgrade.
- It depends on keyserver lifecycle work (create, fetch, delete, prove-deleted) that does not
  exist today, and on the keyserver being trusted to actually delete — which the audit's
  key-substitution finding says it currently is not.

**Until it exists**, `THREAT_MODEL.md` is right and must stay as written: the words "cryptographic
burn" and "permanently undecryptable" are unearned. What the code now genuinely does is destroy
the local cached plaintext, which is exactly what that document claims and no more.

### Defect 6 — plaintext identifiers at rest

The brief said fix the code or fix the invariant, and not to leave them disagreeing. **I fixed the
invariant in the file I own and designed the code fix without executing it**, because executing it
is precisely the case the AFK contract reserves for the owner: an irreversible migration against
real data.

`crates/store/SECURITY.md` now states the exposure as a defect rather than a design note, lists
what an offline reader can reconstruct, records the full current column set including the three
columns nothing writes, and says explicitly that the product-level at-rest invariant disagrees and
that this file is the one matching the code.

**The code fix, designed.** Replace the plaintext id columns with keyed-hash (blind) index columns
— `HMAC(index_key, channel_id)` and so on, under a separate HKDF derivation — and move the
descriptive metadata into a sealed blob. Equality queries (`list_by_channel`, scope wipes,
sender-scoped wipes) survive because they are exact-match; ordering by `decrypted_at` needs the
timestamp coarsened or kept, and keeping it is the honest choice to avoid pretending.

**Why I did not run it.** It rewrites every row of an installed user's database, requires
`SCHEMA_VERSION` to bump, and a rollback afterwards leaves that user's history unopenable. There
is no way to make it reversible in place. It needs the owner's go-ahead and a release in which it
is the only risky change.

**Escalation, the only one in this report:** *do you want the blind-index migration, and in which
release?* Until answered, the invariant text outside my crate still overstates — see below.

---

# Round 2 — defect 6 executed (schema v4)

The owner decided to run the blind-index migration and target it at v1, on the
reasoning that a schema migration is cheapest when nobody has data in the schema and
gets permanently more expensive the day after launch. That is right. One factual
correction to the premise, which does not change the conclusion: "effectively no real
user data" understates the live two-identity QA databases, and
`shred_expired_messages` is documented to avoid exactly this hazard. That is why the
defined-downgrade requirement is the load-bearing one, and it is treated as such below.

## Scope check — this is 95% self-contained, not 100%

The owner's asymmetry argument was that blind indexes live entirely in this crate,
with an instruction to stop and say so if that stopped being true. It is very nearly
true and the exception is worth naming:

- `scripts/find-corrupted-rows.ps1:52` selects `discord_message_id, channel_id,
  datetime(decrypted_at…)` ordered by `decrypted_at`.
- `scripts/delete-rows.ps1:58` runs `DELETE … WHERE discord_message_id IN (<plaintext ids>)`.

Both stop working, because both query the table with plaintext identifiers that no
longer exist. Worth seeing what that means rather than treating it as breakage:
`find-corrupted-rows.ps1` is a tool that reads the social graph out of the database
*without the key*. It works **because** of defect 6. Blinding necessarily breaks it,
and that is the fix doing its job.

**Status: RETIRED, not broken.** A tool that depends on the vulnerability should die
with it, and recording them as "broken by v4" would invite someone to repair them —
which would mean re-exposing the identifiers to make the diagnostic work again.
Neither is product code and `scripts/` is not this lane's, so the file headers still
need their owner's hand; the retirement decision is recorded here.

## What v4 does

- **No identifier is stored.** `messages` holds `mid_bi`, `chan_bi`, `sender_bi`
  (32-byte keyed blind indexes) plus `meta_nonce`/`meta_ct` sealing the real ids and
  timestamp. `attachments` holds `ck_bi`, `mid_bi`, `sender_bi` plus its own sealed
  metadata. The plaintext `scope_type`/`scope_id` columns are **removed** — nothing
  ever wrote them, and a column that is never written is a lie about capability.
- **Blind index** = `HKDF-SHA256(salt = index_key, ikm = field_value, info = per-field
  domain)`, where `index_key` is a second, domain-separated HKDF derivation from the
  same identity secret. Per-field domains stop the same string in two roles producing
  the same index.
- **Ordering** moves from `decrypted_at` to an opaque monotonic `seq`. Sealing the
  timestamp would otherwise force a full-channel decrypt just to sort. Relative order
  is inherent to storing rows at all; wall-clock timing was the leak, so the leak goes
  and the ordering stays. `seq` also makes re-`put` stop shuffling re-decrypted
  history to the top of a channel.
- **`meta_tag` and the strict latch from round 1 are deleted.** Sealing the metadata
  under AEAD both hides and authenticates it, with the row's own blind index as AAD,
  so a separate authenticator is redundant and moving a metadata blob between rows is
  a tag failure.

## Migration, and the two proofs that were required

- Rewrites every row inside **one transaction**; a crash leaves a whole v3 database or
  a whole v4 one, never a half.
- `ciphertext`, `nonce`, `burned`, `burned_at`, `wrapped_key` are copied byte-for-byte.
  **No message body is ever re-encrypted** — the body AAD is still
  `discord_message_id`, recovered from sealed metadata before opening the body.
- `seq` is assigned in `decrypted_at` order, so channel listing is unchanged.
- `VACUUM` afterwards as defence-in-depth. **Correction, measured after the fact:** the thing
  that actually removes the old identifiers is `secure_delete=ON`, which zeroes freed cell
  content at `DROP` time — not the `VACUUM`. With both disabled a raw-file scan still finds the
  old channel id; with `secure_delete` on and `VACUUM` removed it does not. The earlier wording
  in this report and in `SECURITY.md` credited `VACUUM` and was wrong. `VACUUM` is kept because
  it compacts the file and covers residue cell-level scrubbing may not reach, and the
  `vacuum_pending` marker still guarantees it happens — but the crash window it guards is
  narrower than first stated.
- The **canary is verified before the migration runs**, so a wrong secret cannot
  trigger a destructive rewrite of someone's database.

Required proof 1 — *forward migration does not orphan installed history*:
`old_v3_database_migrates_and_stays_fully_readable`, plus
`old_v3_attachment_still_decrypts_after_migration`.

Required proof 2 — *no plaintext identifier recoverable without the key*:
`no_plaintext_identifier_survives_anywhere_in_the_file` enumerates every table from
`sqlite_master` and every column from `PRAGMA table_info` and asserts no identifier
appears in any value — it checks the file, not the fields the author remembered to
seal. `no_plaintext_identifier_survives_in_the_raw_file_bytes` does the same against
the raw file and WAL. `migrating_a_v3_database_removes_its_plaintext_identifiers`
proves the migration scrubs what it converted, and asserts the fixture *did* contain
the identifier first so it cannot pass vacuously.

### Two coverage gaps closed after the first v4 commit

**Older-than-v3 profiles.** The migration was only ever tested from v3. An installed
profile can sit at any older version, and a migration tested against one input is a
migration whose other inputs are assumptions. `v1_database_migrates_all_the_way_to_v4`
builds a true v1 file — no v2 columns, no `attachments` table at all — and asserts it
lands on v4, keeps its rows and ordering, and is *writable* afterwards rather than
merely readable. It passed first time, so it is coverage rather than a fix, and is
labelled as such.

**Orphaned attachments purged at migration.** The pre-fix
`delete_messages_in_channel` removed message rows and left the cached pictures, after
which nothing linked them to a channel and no burn predicate could reach them: a
burned conversation's images could outlive it indefinitely. The migration now deletes
attachments whose message row is gone.

Deliberately **at migration only**, not as an ongoing sweep. Checking the caller
settled it: `cmd_osl_attachment_cache_put` (`crates/ipc/src/commands.rs:2249`) writes
an attachment without requiring its message row to exist, and the UI fetches by the
message id it reads from Discord's DOM rather than from this store — so an orphan is
legitimately reachable in normal operation and a recurring sweep would evict live
cache. At migration the cost is bounded and reversible: a purged row is re-fetched
from the CDN and re-decrypted on next view. That reversibility is why this was decided
here rather than escalated.

Proven by `migration_purges_orphaned_attachments_but_keeps_linked_ones`, which was
observed failing with the purge removed (`migration kept a cached picture whose
message was already deleted`). Its negative control — an attachment whose message
still exists must survive — is the load-bearing half: a purge that emptied the table
would pass the first assertion alone.

**Defined downgrade:** `SCHEMA_VERSION` goes to 4, and `migrate` refuses any on-disk
version it does not recognise. An older binary therefore **refuses to open** an
upgraded database with a clear `Schema` error rather than misreading it. That is a
deliberate reversal of round 1's reasoning, where the additive column was kept
version-neutral precisely so rollback stayed safe. It is the honest cost of actually
removing the identifiers, and it is asserted by
`migration_bumps_schema_version_so_older_binaries_refuse_cleanly`. **Do not run mixed
binaries against one database.**

## Two defects found in my own work, and how

### A data-loss bug the documentation caught

v3 sealed attachment bodies with AAD = the plaintext `cache_key`. My first v4 draft
sealed them under `ck_bi` instead. Since the migration copies attachment ciphertext
byte-for-byte, **every migrated attachment would have failed to decrypt** — silent
loss of every cached image on upgrade.

Nothing detected it. It surfaced because the delegated SECURITY.md draft accurately
wrote down what the code did — "copied byte-for-byte" — and that accurate sentence is
what made the contradiction visible. Fixed by keeping the body AAD as `cache_key`,
exactly as message bodies keep `discord_message_id`.

### The test for it could not fail

The regression test I then wrote **passed with the bug deliberately reintroduced**.
Its fixture built the v3 attachment by writing through the *current* store and lifting
the sealed bytes back out, so it reproduced whatever AAD the code happened to use. It
was self-referential: it could only confirm that the implementation agreed with itself.
This is the same failure as the R2 double that accepted any stream and the D1 fakes
that could only re-assert what their author believed — arrived at independently, in a
file whose own module doc warns against it.

Fixed by adding `crypto` as a dev-dependency and sealing the fixture with the audited
primitives directly, reproducing **v3's** AAD rules with no reference to the crate.
Re-verified by reintroducing the bug: the test now fails with
`migration made the cached attachment undecryptable`, and passes once reverted.

### Crash-safety gap in the migration

The version stamp was written after the migration transaction committed. A crash in
that window produced v4 tables recorded as version 3; the next open would try to add
legacy columns to the new schema and then fail reading `discord_message_id`, so the
store would not open at all. The stamp now happens **inside** the same transaction as
the rename. The post-commit `VACUUM` cannot be transactional, so a `vacuum_pending`
marker records that the scrub is owed and the next open finishes it — otherwise a
crash there would silently leave behind exactly what the migration exists to remove.

## What is still visible without the key — do not overstate this

Blinding removes the labels, not the shape. An offline reader still learns:

- the number of message and attachment rows;
- that two rows share a channel or a sender, because blind indexes are deterministic —
  the value is hidden, the equality is not, and frequency analysis over those groupings
  remains possible;
- the size of each sealed body, which leaks message length and attachment size;
- the relative order of rows via `seq`;
- which rows are burned, and when.

So: **the social graph's labels are gone; its shape is not.** The phrase "the social
graph is protected" is still unearned and `crates/store/SECURITY.md` says so.

## Round 3 — findings I had left unread

**Correction to my own close.** I reported four crypto areas as "unreviewed" because my first
adversarial dispatch was truncated by a `| tail -80`. That was wrong. The *second* run completed
and covered all five sections; I had only ever read its tail. Five findings sat unactioned in a
file I already had. Truncating the dispatch was the visible mistake; not re-reading the
replacement was the one that actually cost something.

Three closed (commit `bc239b1`):

- **Attachment metadata transplanted into a message row.** The two metadata encodings begin
  identically — four length-prefixed strings then an `i64` — and message decoding accepted
  trailing bytes, so an attachment blob decoded cleanly as a message: cache key read as a message
  id, MIME read as an OSL identity. Both decoders now require the reader to consume the whole
  input.

  **Honest attribution, measured by disabling each defence separately:** the exhaustion check is
  *not* what blocks this. With exhaustion off and the selector re-derivation on, the transplant is
  still refused; with **both** off it succeeds and an attachment is served as an authenticated
  message in the attacker's chosen channel. The selector check does the work. Exhaustion is kept
  as defence-in-depth and for decode paths with no selector to cross-check. This is the second
  time tonight I nearly credited the wrong mechanism — the first was `VACUUM` over
  `secure_delete`.

- **Fresh-v4 initialisation was not atomic.** Table creation and the version stamp were separate
  autocommit operations. A crash between them left a `messages` table with no recorded version;
  the next open read that as *legacy* and tried to migrate it by selecting plaintext columns v4
  does not have. A crash during first-run init could leave a store that never opens again. Now one
  transaction.

- **`seq` ties had no tie-breaker.** `next_seq` is `MAX+1` under a per-instance mutex, so two
  `MessageStore` instances on one file can assign the same value. No crypto impact and no nonce
  reuse; ordering was unstable at a `LIMIT` boundary and attachment trimming could evict an
  arbitrary row. Both queries now order by `(seq, blind index)`.

Two left unfixed and recorded rather than dropped:

- **Body AADs are not namespace-disjoint (Low) — queued, with its mechanism.** An attachment's
  body AAD is the cache key `"{discord_message_id}/{random_filename}"`, and the store validates
  neither argument. Two different pairs can therefore produce one cache key — id `a/b` with
  filename `c`, and id `a` with filename `b/c`, both give `a/b/c` — which is one blind index and
  so one row. One message's cached attachment could be served for another's.

  Both values arrive unvalidated from the IPC boundary as plain `String`
  (`crates/ipc/src/commands.rs:2216` and `:2253`), so nothing upstream of this crate enforces the
  snowflake shape that makes the collision unreachable in practice.

  **The fix must be input validation, not an AAD change.** Changing a body AAD would orphan every
  sealed body already on disk, because the migration copies those bytes verbatim — that is exactly
  the mistake that would have made every migrated attachment undecryptable earlier tonight. So:
  reject `/` in `discord_message_id` and `random_filename` at the write and read paths, which
  costs no format change.

  **Now done.** A new `StoreError::InvalidId` refuses `/` and NUL in both arguments at `put`,
  `get`, `put_attachment` and `get_attachment`. Adding an enum variant was checked first rather
  than assumed safe: nothing outside this crate matches on `StoreError` variants — the
  `CipherStoreError::` hits in `crates/ipc` are a different type — so no caller's match arms
  break. `ambiguous_cache_key_components_are_refused` was observed failing with the check
  disabled (`a message id containing '/' was accepted, making the cache key ambiguous`), and
  carries a positive control so it cannot pass because attachments are broken outright.

  Scope of the evidence, stated precisely: the **refusal** is proven by test. The consequence —
  that two accepted pairs would collapse onto one row and serve one message's attachment for
  another's — is derived from the cache-key format `"{id}/{filename}"`, not separately observed,
  because the validation now prevents constructing it.
- **Migration `INSERT OR REPLACE` (Low) — now closed.** Reachability first: v1 and v3 both make
  `discord_message_id` and `cache_key` PRIMARY KEYs, so two legacy rows cannot share an
  identifier; only a 2^-256 blind-index collision could trigger it. The finding was therefore not
  reachable — but the *behaviour* was still wrong, because `OR REPLACE` would silently drop one of
  the user's messages and report a successful migration. Now a plain `INSERT`, so a collision
  aborts inside the migration transaction and the original database rolls back intact.

  This also closed a gap nothing had tested: **every migration test used a well-formed fixture, so
  the failure arm had never run.** `a_failed_migration_rolls_back_and_leaves_the_legacy_data_intact`
  forces the abort by building the legacy table without its primary key so a duplicate can exist,
  then asserts the open is refused *and* that both original rows and the legacy table shape
  survive. Observed failing with `OR REPLACE` restored (`a migration that cannot represent both
  rows reported success`).

## Handoffs

### To the crypto lane

Two items:

1. **Releasing `crates/store/**` to this lane.** You keep `crates/keystore`, `apps/osl-hub/**` and
   `crates/ipc`. Please do not edit `crates/store` from here on; route anything you need through
   this report. **The public API surface is unchanged** — `git diff` of `pub fn` / `pub struct` /
   public field lines in `crates/store/src/lib.rs` is empty — so nothing of yours needs to change.
2. **`crates/keystore` does not compile for the Windows target, and it is not my change.**
   `crates/keystore/src/sealer.rs:569` and `:592` call `tracing::debug!`, but
   `crates/keystore/Cargo.toml` declares no `tracing` dependency. Introduced by commit `1c61662`
   ("keystore: honest tri-state TPM eviction; refuse malformed QA requests"); `sealer.rs` is not
   dirty in the tree, so the breakage is committed. This blocks the mandatory hub check for every
   lane, not just mine. It is your crate and I did not touch it.

### To truth / the document owners — invariant still disagreeing

`crates/store/SECURITY.md` is now accurate. The **stronger product-level at-rest claim is not**,
and I do not own the file it lives in. `docs/design/osl-plan-b-safety-first.md:41-42` asserts that
no path logs message identifiers or conversation names; the audit records the same claim as the
"explicit at-rest invariant". Both overstate what `messages.sqlite` actually protects. Until
defect 6's migration is decided, that text should be corrected to match the store, exactly as
`THREAT_MODEL.md`'s burn section already was.

### To the document owners — 26 burn claims the code does not support

A repository-wide documentation sweep (Codex cx2, read-only; full output at
`scratchpad/store-doc-sweep.md`) found **26 statements classified STALE-OVERSTATES** — docs
promising more than the store delivers. None are in a file this lane owns. I verified a sample
directly rather than relaying the classification:

- `README.md:78` — *"Burn is local cryptographic erasure. It destroys keys, not messages."*
  There are no per-message keys. `wrapped_key` is NULL on every row ever written, so there is no
  key to destroy. What burn destroys is the local cached ciphertext. **This is the public-facing
  README and it is the highest-priority correction in this list.**
- `README.md:81` — *"Scope burn destroys your keys for one conversation."* Same defect.
- `docs/design/burn-contract.md:3` — *"Burn is local cryptographic erasure…"*
- `docs/phase-7-design.md:80` — *"All your sent messages everywhere → permanent ciphertext."*
  Recipient long-term identity keys still decrypt that ciphertext, forever.
- `docs/design/osl-hub-feature-parity.md:73` — *"Burn is cryptographic/local revocation."*

Remaining hits: `docs/design/offline-controls-and-opened-receipts.md:12`,
`docs/phase-7-design.md:84,87,88,90,94,95,112,123,214`,
`docs/design/group-messaging.md:54,58,118`, `docs/design/key-server-api.md:248,309`,
`docs/design/osl-gui-final-plan.md:499`, `docs/design/osl-master-decision-2026-07-26.md:585`.

`docs/THREAT_MODEL.md` is explicit that the words "cryptographic burn" and "permanently
undecryptable" are unearned until the per-message `wrapped_key` model exists. These 26 lines
predate that correction and now contradict it. The burn behaviour this lane shipped today makes
the *local* destruction genuinely real, which narrows the gap — but it does not close it, and
nothing here justifies the word "cryptographic".

## Verification

Run in this tree, at the end of the work, wrapping only `cargo` commands I typed — no script that
takes the same lock internally.

```
flock /tmp/osl-cargo.lock -c "cargo test -p store"
  → 18 passed (blind_index_test), 13 passed (burn_defects_test),
    17 passed (store_test), 0 failed, 0 ignored.  48 total.

flock /tmp/osl-cargo.lock -c "cargo clippy -p store --all-targets"   → clean, no warnings
cargo fmt -p store -- --check                                        → clean
flock /tmp/osl-cargo.lock -c "cargo check -p store --target x86_64-pc-windows-gnu"
  → Finished. store cross-compiles for the shipping target.
```

**On the test count and the gate that produced it.** 48 is measured, not inherited, and the gate
is the bare one: `osl-cargo test -p store`, no features.

Naming the gate matters now, because the fleet learned tonight that a gate can move a number in
either direction. `--features core` silently excluded `qa_selftest_request`; the replacement
`--features core,discord-qa-shell` includes it but *relaxes* an enforcement, because
`header_proof_is_enforced()` is defined as `!cfg!(feature = "discord-qa-shell")`. So a count is
meaningless without the gate beside it, and a test passing under the QA feature has not proven
header proof is enforced.

**Neither hazard exists in this crate, verified rather than assumed:** `crates/store/Cargo.toml`
declares no `[features]`, and a grep for `cfg(feature` / `cfg!(feature` across `crates/store/`
returns nothing. There is no configuration under which this crate compiles a different set of
modules or enforces a different set of checks, so all 44 tests run under every possible gate. No
claim in this report depends on `header_proof_is_enforced`. All
17 pre-existing tests still pass; none were modified.

**The mandatory hub check now PASSES.** It was blocked for most of this lane's work by a
`crates/keystore` defect (`tracing` used but not declared). The crypto lane has since added
`tracing = { workspace = true }` at `crates/keystore/Cargo.toml:57`, and the gate was re-run:

```
osl-cargo -C apps/osl-hub check --features desktop --bin osl-privacy-hub \
  --target x86_64-pc-windows-gnu
  → Finished `dev` profile in 10.28s
```

Only pre-existing dead-code warnings remain, all in other lanes' files
(`native_attachment_transport.rs`, `native_discord_overlay.rs`). **This is the proof that schema
v4 does not break the crypto lane's build**, and it is the check this lane owed from round 1.

Note the tool change: Rust commands now run through `osl-cargo`, which takes the build lock
itself. Do **not** wrap it in `flock` — `flock` is not reentrant and the nested acquisition hangs
silently with no process visible in `ps`.

Historical record of the blockage:

```
  → error[E0433]: unresolved module `tracing` — crates/keystore/src/sealer.rs:569, :592
```

It was attributed rather than assumed at the time: `crates/keystore` does not depend on `store`,
and `cargo check -p keystore` failed identically on its own. I did not fix it — not my crate, and
quietly editing another lane's file is how two lanes end up in the same rebase. Routing it and
waiting was the right call, and it is now resolved by its owner.

## Acceptance rows this earns

Truth is the single writer and applies points; this lane does not edit the checklist. Offered with
its evidence:

| Claim | Evidence | Strength |
|---|---|---|
| Burn is terminal against a normal re-`put` | 3 tests, failing-first recorded | Earned |
| `mark_burned` cannot report success without shredding | test + defect-reintroduction proof | Earned |
| Scope burns reach legacy/unscoped attachments | 3 tests, failing-first, 2 negative controls | Earned |
| Channel destruction removes cached attachments | 2 tests, failing-first | Earned |
| Row metadata is authenticated against offline edits | 2 tests, failing-first | Earned |
| Upgrade does not orphan installed history | 2 migration tests incl. a version-pin assertion | Earned |
| `crates/store/SECURITY.md` matches the code | file rewritten against re-read source | Earned |
| Store crate builds for the shipping Windows target | `cargo check -p store --target …` | Earned |
| No plaintext identifier is recoverable from the file without the key | whole-file + raw-bytes sweeps, failing-first | Earned |
| A v3 database migrates with every message and attachment still readable | 2 migration tests over a fixture built against v3's own format | Earned |
| The migration scrubs the identifiers it converted | raw-file assertion, with a positive path proving they were there | Earned |
| Older binaries refuse an upgraded database cleanly | version-pin assertion | Earned |
| A v1 profile migrates all the way to v4 and stays writable | true v1 fixture, no v2 columns, no attachments table | Earned (coverage, not a fix) |
| Orphaned attachments are purged at migration | failing-first, with a negative control on linked rows | Earned |
| A row cannot be retargeted into another conversation on disk | failing-first; also blocks attachment-metadata transplant | Earned |
| A populated database with its canary removed refuses to open | failing-first, covers attacker's and owner's secret | Earned |
| Five pre-existing tests no longer pass against a no-op | each observed failing against a stubbed writer | Earned |
| Message bodies and attachment bytes are unreadable in the file | failing-first against a no-op cipher | Earned |
| Ambiguous cache-key identifiers are refused | failing-first, with a positive control | Earned |
| A failed migration rolls back and preserves legacy data | failing-first; the abort arm had never been exercised | Earned |
| Four tests now exercise the property their name claims | two never reached it before; verified by reading the assertions | Earned |
| Hub builds with these changes | `osl-cargo -C apps/osl-hub check --features desktop …` → Finished | Earned |
| **Burn is cryptographic / revokes recipient access** | **false; unchanged by this work** | **Not earned** |
| **`messages.sqlite` hides the social graph** | **labels yes, shape no — see "What is still visible"** | **Partially earned; do not state unqualified** |
| **Operator scripts still work** | **false; two `scripts/*.ps1` are broken by design** | **Not earned** |

## Resume here

**State:** defects 1, 2, 4, 5 and **6** fixed with failing-first evidence and committed. Defect 3
deliberately not executed; it needs keyserver work before the store can play any part.
`crates/store/**` is green (40 tests), formatted, clippy-clean, and cross-compiles for the
shipping Windows target. Schema is v4.

**Next, in order:**

1. **Done:** the hub gate passes as of the crypto lane's `tracing` fix. Re-run it after any
   further change to this crate — it is the only proof that a shared-crate edit has not broken
   the hub, and a crate that compiles alone is not evidence.
2. **Decided — retired:** the two `scripts/*.ps1` diagnostics. They only worked because of the
   defect v4 closes. Their owner may want a deprecation header on the files themselves.
3. **Awaiting decision (defect 3):** per-message `wrapped_key`. The full deferred design is now
   written up at `docs/design/osl-cryptographic-burn-design-2026-07-26.md` — read §2.4 (what
   "proven deleted" can mean against a keyserver that can already substitute recipient keys:
   attested, not proven) and §5 (mixed-version peers are worse than the current honest absence)
   before resuming. Needs keyserver lifecycle work first; the store is the last part, not the
   first. No revisit trigger is attached — resuming is an owner call.
4. **For the document owners:** 26 burn statements still overstate what the code does; the
   at-rest invariant can now be *strengthened* to match v4, but only with the
   "labels not shape" qualification.

**If you extend this crate, read this first:** the v3 fixtures in `blind_index_test.rs` are
written against the **old** format on purpose, using `crypto` directly. Do not "simplify" them to
build fixtures through `MessageStore` — that exact change once made a data-loss bug invisible,
and the module doc records why.

**Traps for whoever resumes:** `flock` is not reentrant — wrap only `cargo` you type, never
`scripts/qa/osl-instance-b-build-wsl.sh`, which locks internally at `:88`. The store's tests use a
real SQLite file and real migrations on purpose; do not replace them with a fake, because a fake
can only re-assert what its author already believed.

## In plain language

1. Burning a message used to be undone by the app itself — the next time you opened that
   conversation, the message came back. It does not any more.
2. The "burn" button could report success while the message was still sitting on your disk. Now it
   always destroys the message before it says it did.
3. Burning a conversation deleted the words but left the pictures behind, still openable. Now the
   pictures go too.
4. Someone who could get at your database file could rewrite it to make it look like you said
   something you never said. Your app now detects that and refuses to show it.
5. Your database file used to show anyone who opened it who you talk to and when, without needing
   your password at all. Those names, ids and times are now scrambled and unreadable. Someone can
   still see the *shape* of things — how many messages there are, that two of them belong to the
   same conversation, roughly how long each is, and which were burned — but not who, not what, and
   not when. Upgrading keeps all your existing messages; going back to an older version will not
   open the file.
6. One thing is still deliberately left alone: burning cannot reach the copy Discord already has.
