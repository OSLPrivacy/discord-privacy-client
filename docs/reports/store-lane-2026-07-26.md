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
| 3 | Per-message `wrapped_key` model absent | **Designed, not executed** | n/a — owner decision |
| 4 | Legacy/unscoped attachments survive scope burns | **Fixed** | 5 tests failed pre-fix |
| 5 | Metadata outside AEAD authentication | **Fixed** | 2 tests failed pre-fix |
| 6 | Plaintext identifiers at rest | **Docs reconciled; code fix designed, not executed** | n/a — owner decision |

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

**Residue, unfixed and stated:** an attachment whose message row was already deleted by an older
build has no remaining link to any scope. Nothing can attribute it to a burn. It is not created
any more, but pre-existing orphans will persist. Deleting all orphans globally would evict
unrelated channels' caches, so I did not.

### Defect 5 — metadata is authenticated

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
  → 17 passed (burn_defects_test), 17 passed (store_test), 0 failed, 0 ignored.  34 total.

flock /tmp/osl-cargo.lock -c "cargo clippy -p store --all-targets"   → clean, no warnings
cargo fmt -p store -- --check                                        → clean
flock /tmp/osl-cargo.lock -c "cargo check -p store --target x86_64-pc-windows-gnu"
  → Finished. store cross-compiles for the shipping target.
```

**On the test count:** 34 is measured, not inherited. `crates/store/Cargo.toml` declares no
`[features]`, so there is no feature gate that could silently exclude a module from `-p store` —
the failure mode that hid `qa_selftest_request` from the workspace runs does not exist here. All
17 pre-existing tests still pass; none were modified.

**The mandatory hub check did not pass, and not because of this lane:**

```
flock /tmp/osl-cargo.lock -c "cd apps/osl-hub && cargo check --features desktop \
  --bin osl-privacy-hub --target x86_64-pc-windows-gnu"
  → error[E0433]: unresolved module `tracing` — crates/keystore/src/sealer.rs:569, :592
```

Attributed rather than assumed. `crates/keystore` does not depend on `store`, and
`cargo check -p keystore --target x86_64-pc-windows-gnu` fails identically on its own. The
compile aborts in keystore before reaching hub code, so **I cannot yet prove the hub builds with
these changes** — I can only prove the store compiles for that target, that the public API is
byte-identical, and that the blocker is upstream of me. Re-run the hub check once keystore is
fixed. I did not fix keystore: it is not my crate, and quietly editing another lane's file is how
two lanes end up in the same rebase.

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
| **Hub builds with these changes** | **not established — blocked by keystore** | **Not earned** |
| **Burn is cryptographic / revokes recipient access** | **false; unchanged by this work** | **Not earned** |
| **`messages.sqlite` hides the social graph** | **false; exposure now documented, not fixed** | **Not earned** |

## Resume here

**State:** defects 1, 2, 4, 5 fixed with failing-first evidence and committed. Defects 3 and 6
deliberately not executed; both need an owner decision. `crates/store/**` is green, formatted, and
lint-clean; the crate is otherwise untouched by other lanes.

**Next, in order:**

1. **Blocked, not mine:** re-run the hub gate once the crypto lane adds `tracing` to
   `crates/keystore/Cargo.toml`. Command is in Verification above. Until then no lane can prove a
   hub build.
2. **Awaiting decision (defect 6):** the blind-index migration. Do not start it without an explicit
   yes and a release slot — it is irreversible against real user data.
3. **Awaiting decision (defect 3):** per-message `wrapped_key`. Needs keyserver lifecycle work
   first; the store is the last part, not the first.
4. **Unblocked and small:** extend the metadata authenticator to `attachments` rows (`mime`,
   `byte_len`, scope, sender). Same helper, same additive shape, no version bump.
5. **Unblocked and small:** the pre-existing orphan attachments described under defect 4. Needs a
   decision on whether a one-time scan may delete attachments with no surviving message row.
6. **For the document owners:** correct the product-level at-rest invariant to match the store, or
   decide item 2 and make the invariant true.

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
5. Two problems are deliberately left alone because fixing them safely is the owner's call: the
   database still shows an intruder *who* you talk to and *when*, though not what you said — and
   burning still cannot reach the copy Discord already has.
