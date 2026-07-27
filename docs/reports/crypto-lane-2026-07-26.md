# CRYPTO-LANE-2026-07-26 — burn truthfulness, bilateral burn, friend removal, attribution, zeroization

Status/build/worktree/owner:
- Status: `test-proven-only`. Everything below is compile + unit evidence. **No runtime,
  two-identity, or user-visible evidence was produced in this run**, so no acceptance row is
  claimed. See "Checklist rows" at the end.
- Worktree: `/home/liamw/discord-privacy-client`, branch `osl-eye-and-features-2026-07-26`,
  HEAD `fc6b983` (dirty).
- Master control revision read: `OSL-MASTER-2026-07-26-r5` (returning encounter — header, §0.5,
  §2.1 deadline register, and task-linked sections only).
- Exclusive files this lane touched: `apps/osl-hub/src/{security.rs,broker.rs,main.rs}`,
  `apps/osl-hub-ui/src/overlay.ts`, `crates/keystore/{src/lib.rs,src/storage.rs,tests/sealer_test.rs}`.
  No git writes, no deploys, no keyserver migrations, no edits under `docs/` except this report.

## The one-line decisions

- **Task 0 — I finished the half-written work rather than reverting it**, because the missing
  helper was the friend-removal persistence that Task 3 required anyway; reverting would have
  deleted work I was about to rewrite identically.
- **Task 2 — I wired the outbound revocation lane in both directions** (receipt-on-apply and
  queued-notice-per-drain), on the owner's statement that keyserver migration 0027 is deployed,
  and specifically because the queue fails *closed* if that statement is wrong (see below).
- **No acceptance points claimed.** Every row I own is gated on evidence this run did not produce.

## Starting evidence — the baseline genuinely failed first

Reproduced before any edit, `cargo check --features core --lib`:

```
error[E0425]: cannot find function `persist_friend_removal` in this scope  --> src/security.rs:641:5
error[E0509]: cannot move out of type `Identity`, which implements the `Drop` trait --> src/security.rs:419:22
error: could not compile `osl-privacy-hub` (lib) due to 2 previous errors
```

## What the killed tabs had already finished (verified, not assumed)

The dispatch described several defects as open. Re-verifying against current bytes, most were
already fixed by the tabs that died; the line numbers in the dispatch had all moved.

| Dispatch item | Actual state on current bytes |
|---|---|
| Task 1 — resolution returns a bare `Vec`, callers `unwrap_or_default()` | **Already fixed.** `RevocationRecipients { peers, skipped, resolver_failed }` exists (`security.rs:2571`) with `expected()`/`fully_addressed()`; `queue_scope_revocations_locked` starts completeness at `recipients.fully_addressed()`, not `true`; all three callers use `RevocationRecipients::unresolved()` on error. |
| Task 1 — required tests | **Already present and genuine**: `burn_never_reports_complete_after_dropping_an_unaddressable_peer` covers malformed X25519, missing `osl_user_id`, and resolver read error, each asserting `complete == false`. |
| Task 2 — inbound `0x0A` deleted instead of applied | **Already fixed.** `InboundRevocationControl::classify` → `apply_inbound_revocation_row` → `RevocationRowOutcome::{Applied,Unappliable,Deferred}`, and the row is deleted only when `retires_row()`. |
| Task 4 — `security.test.ts` capability drift (3 permissions) | **Already fixed.** All three are present in manifest order; the baseline report's third frontend failure was gone before I touched anything. I verified this by running vitest *before* my changes: 2 failed / 740 passed. |
| Task 5 — `Identity`/`InnerIdentity`/sealer/TPM zeroization | **Already applied in source.** `Identity` has a `Drop` that zeroizes `recovery_entropy` plus a `ZeroizeOnDrop` marker; `InnerIdentity` derives `Zeroize`+`ZeroizeOnDrop`; `save` wraps the serialized secret document in `Zeroizing`; `unseal` returns `Zeroizing<Vec<u8>>` (the `Zeroizing::new(())` bug is gone); the TPM path wraps both the unwrapped data-key vector and the fixed-size copy. |

**This is why the E0509 existed at all**: `Identity` gained a `Drop`, which retroactively made every
existing move-out of one of its fields illegal.

## What I actually changed

### 1. Build repair (Task 0)
- `security.rs` — `export_friend_code` clones `identity.user_id` instead of moving it out of a
  `Drop` type.
- `security.rs` — wrote the missing `persist_friend_removal`, mirroring `add_friend_code`'s
  write-then-rollback idiom. **Ordering is deliberate and is a security choice**: the peer *key* is
  destroyed first and the People record second, because the dangerous half-state is a key with no
  record (OSL would still seal to it and still match it as a sender — the friend is not actually
  removed), while a record with no key is inert. A failed People write restores the previous peer
  map.

### 2. Bilateral burn — the outbound lane (Task 2)
`post_due_revocations` and `post_revocation_frame` were fully written, correct, and **had zero
callers**. Both are now wired inside `drain_peer_inbox_text`:
- **Receipt on apply.** `apply_inbound_revocation_row` now returns `(outcome, Option<ack_b64>)`
  instead of discarding the acknowledgement, and the drain posts the `0x0B` **before** deleting the
  inbox row. Safe on failure: the burn floor is already durable, so the peer re-sends, this device
  re-applies idempotently, and the receipt is produced again.
- **Queued notices per drain.** `post_due_revocations` runs last in the drain, on the same
  authenticated per-peer binding, so a burn notice can never delay or fail the operator's message
  drain.

**Why this is safe even if 0027 is not deployed.** The lane and collapse key are part of the
*signed* canonical POST bytes, so an older Worker reconstructs different bytes and answers `401`.
It fails closed rather than dropping the burn into the ordinary lane where the sender's own next 32
messages would evict it. Every attempt is recorded whether or not the POST succeeded, and only the
peer's `0x0B` clears an entry — so a refusal leaves the notice queued for the next drain. This
satisfies the standing "design for a retryable queue, not a dropped notice" constraint regardless
of which claim about 0027 turns out to be true. I did not edit the migration file.

### 3. Friend removal (Task 3)
`remove_friend` was otherwise complete and I verified each required property against source rather
than assuming it:
- Recipients resolved **before** any trust/key state is removed (a set captured afterwards is empty).
- `withdraw_person_grants` withdraws manual approvals and display policy and drops
  `reach_narrowed_scopes[&person_id]`.
- **`burned_manual_scopes` is not touched.** Burning stays terminal.
- Revocation notices are queued **last**, once per withdrawn scope key.
- `person_dto` call sites: **exactly five**, confirmed (`security.rs:811, 821, 850, 1102, 1180`).
- `remove_hub_friend` is registered in `generate_handler!` (`main.rs:7314`) and its ACL is granted
  only to `hub-local`; not widened.

### 4. Sender attribution (Task 4)
`authenticate_oriented_prose_pointer` proved the orientation and then **threw it away**, so every
opened row reached the renderer unlabelled and `overlay.ts` stamped `direction:"incoming"` and
`author: verifiedFriendIdentity` on all of them. The operator saw their own sent messages presented
as their friend's words.

- The function now returns `(PeerProtectedPayload, PeerWireOrientation)`.
- New `RehydratedRowOrientation { Incoming, Outgoing }` travels through
  `RehydratedNativeDiscordRow` → `RehydratedNativeDiscordRowDto` → the renderer. There is
  deliberately **no `Unknown` variant**: a row whose orientation was not proven has no plaintext
  either, so the pairing is unrepresentable.
- `overlay.ts` parses it strictly (exact key set; text-without-author and author-without-text are
  both refused, which refuses the whole read) and labels from it, with `author` resolving to
  `localIdentity` for an outgoing row.
- **Fail-closed** at both ends: unproven orientation means OSL paints nothing and Discord's own
  carrier row stays visible.

**Correction to the dispatch:** the DTO named in the task (`NativeDiscordCarrierRowDto`, main.rs)
is `#[cfg(feature = "discord-qa-shell")]` and is not the path the eye uses. The live path is
`RehydratedNativeDiscordRowDto`. Also, of the three `direction:"incoming"` sites, only the decoded
transcript one was wrong — the pending-attachment and pending-view-once sites are inbound by
construction and were correctly left alone.

### 5. Zeroization (Task 5) — and the half-applied fallout
The source work was done; **the test suites had never been updated to match it, and neither
suite compiled.** This is why `cargo check --lib` is not evidence: it does not build test targets.
- `crates/keystore/tests/sealer_test.rs`: 8 errors — the test `Sealer` impls still returned
  `Vec<u8>` after the trait moved to `Zeroizing<Vec<u8>>`. Fixed.
- `broker.rs:8037` (test): another E0509 move out of `Identity`. Fixed.
- **Real API defect found and fixed**: `Sealer::unseal` returns `Zeroizing<Vec<u8>>`, which makes
  that type part of keystore's public API, but it was never re-exported — so no caller outside the
  crate could name the return type or implement the trait. Added `pub use zeroize::Zeroizing;`.
- **Added the missing tests** (`crates/keystore/src/storage.rs`, new test module — there was none):
  `zeroizing_an_inner_identity_clears_every_secret_field` and
  `secret_carriers_wipe_themselves_on_drop`.

**Negative control, stated honestly:** the zeroization test does not compile against the pre-fix
code (`InnerIdentity` had no `Zeroize` derive), so its control is **compile-time, not runtime**.
Reading a freed buffer to observe the wipe at runtime is undefined behaviour and was not attempted.
What is asserted is that zeroizing clears *every* field, not only remembered ones.

## Inherited numbers are not evidence

The dispatch gave a keystore baseline of "170 passed / 1 ignored", taken from a dated report. That
number was **fiction on current bytes** — `cargo test -p keystore` did not compile at all, so no
count was reproducible. It is recorded here as a standing rule, not a one-off:

> **Treat every inherited test count as unverified until this lane has personally re-run it, and
> say so in the report where it applies.** A count copied from a dated report describes bytes that
> no longer exist. `cargo check --lib` is not a substitute — it does not build test targets, which
> is exactly how two broken suites survived a green baseline report.

Numbers below are all from runs performed in this session. The 674/739 figures quoted as
"baseline" are inherited and were **not** independently re-verified against the pre-change tree,
except the frontend one, which I did re-run before touching anything (it was already 2 failed /
740 passed, not the 3 failed the report claimed).

## CORRECTION — every Rust count in this report was measured under the wrong feature gate

`apps/osl-hub/src/qa_selftest_request.rs` is gated behind
`#[cfg(all(feature = "core", feature = "discord-qa-shell"))]` (`lib.rs:69`), but the lane-standard
command everyone has been running is `--features core`. **That module — the code deciding whether a
trigger file becomes a harmless status read or the irreversible SEND verb — was compiled out of
every hub test run quoted today.** 680 tests listed, zero from that module. The correct gate runs
731.

I only caught it because two tests I added did not change the total. **That is the symptom to look
for: a count that does not move when you add tests.**

| Command | Reported here originally | Correct gate |
|---|---|---|
| hub lib | 679 passed / 0 failed / 1 ignored (`--features core`) | **729 passed / 1 failed / 1 ignored** (`--features core,discord-qa-shell`) |

The 1 failure is pre-existing and **not this lane's work**:
`native_discord_adapter::tests::the_header_proof_walk_runs_in_both_cfgs_and_only_enforcement_differs`
(`native_discord_adapter.rs:21382`). That file is dirty from another lane mid-edit, so it was left
untouched.

`cargo test -p keystore` (176 passed / 0 failed / 1 ignored) and `cargo test -p ipc` are unaffected —
separate crates with no such gate — and both were run directly in this session rather than inherited.

**This is the fourth false green of the day and they are one family** (an R2 double that accepted
any stream, D1 fakes that could only re-assert their author's beliefs, a release workflow never once
executed, and this gate). Each reported success without ever having done the thing. Standing rule
taken from it: prefer a real runtime to a double; when a double is unavoidable, ask what it would
fail to catch; and assert non-empty on the positive path so the negative path cannot pass vacuously.

## Audit CRITICAL #4 — plaintext at rest: closed, on both halves

**Question routed to this lane:** if attachment part upload was returning HTTP 500 in production,
did any decryptable attachment ever exist — making CRITICAL #4 structurally nil rather than merely
unobserved?

**Answer: confirmed nil, historically — but the client-side evidence alone would have produced a
false refutation, so the working matters.**

The client has *two* upload paths (`cipher_store_client.rs:438`): attachments **≤ 26 MiB**
(`LEGACY_DIRECT_ATTACHMENT_BYTES`) take a single-shot POST with a known `content-length`, and only
those **> 26 MiB** take multipart. Reading the client alone, the small-attachment path looks exempt
from the multipart defect, and the honest conclusion would have been "the exposure was real for
almost every real attachment".

That is wrong, and the server's pre-fix code is what settles it. In `08552e5` **both** paths fed
`boundedAttachmentStream` — a `pipeThrough` `TransformStream` with no known length — the multipart
`uploadPart` at `attachment.ts:273-275` **and the direct `put` at `:366-368`**. R2 requires a known
length for both. So no attachment ciphertext of any size ever reached R2; nothing could be fetched;
nothing could be decrypted to a durable plaintext file.

**Bounds, stated precisely:** `08552e5` is a WIP snapshot, so the age is unbounded below and git
cannot see an earlier variant; this covers the cipher-store transport only; and it says nothing
about images, which the audit treats separately.

**The receiver half was already closed too** — verified in the current tree rather than assumed:
`decrypt_file` and `StagedPlaintext` are gone (only `decrypt_file_to_memory` remains,
`peer_attachment_io.rs:517`); non-image opens return `EXTERNAL_VIEWER_REFUSAL` at
`native_attachment_transport.rs:1109` and `:1243` *before* any token parsing, download, decrypt,
replay consumption or burn; and the picker is gated by `supported_protected_image_mime` so only
PNG and JPEG are offered.

So restoring the transport (cipher-store `0a17547d`, part upload now 201) does **not** arm a dormant
defect. I raised that risk and it turned out the exposure work had already shut the other door this
morning. **Recorded as closed on both halves, not as a risk this lane is carrying.**

## Lane ownership as of end of session

**Owned:** `crates/keystore/**`, `crates/ipc/**` *except* `wire_rn.rs`, `apps/osl-hub/**`,
`apps/osl-hub-ui/src/overlay.ts`, `apps/osl-hub-ui/src/security.test.ts`, `scripts/qa/` (the P2P
rig), and this report.

**Released during the session:** `crates/store/**` → store lane; `crates/ipc/src/wire_rn.rs` →
ratchet lane (B4 capability negotiation / monotone downgrade pin). Neither was edited from here
after release.

**Routed out, not fixed here:**

| Item | Owner | Why not this lane |
|---|---|---|
| `overlay.test.ts` renderer orientation test | frontend-test owner | Not in this lane's file set; the producer contract is pinned by a Rust test instead |
| `native_discord_adapter` failing test (`:21382`) under the correct gate | whoever is mid-edit in that file | Still shows ` M` in `git status` — another lane has it open; editing it would collide |
| `docs/qa/two-identity-p2p-verification.md` §6 items 1, 4, 6 now stale | truth lane | `docs/**` is not this lane's, and all three are stale *because* of work done here |
| 26 statements overstating burn, incl. `README.md:78` | truth lane | Documentary half of the same gap the burn-outbox work narrowed |
| Dispatch wiring for the four new QA verbs | blocked, not owned elsewhere | The browser-profile Tauri commands do not exist in this tree yet; the request vocabulary is done and waiting for them |

## Verification commands and exact results

| Command | Result | Baseline | Verdict |
|---|---|---|---|
| `cargo test -p keystore` | **176 passed, 0 failed, 1 ignored** | 170 / 1 | beats |
| `cargo test -p store` | **17 passed, 0 failed** | — | pass |
| `cargo test --features core --lib -- --test-threads=1` | **679 passed, 0 failed, 1 ignored** | 674 / 0 / 1 | beats |
| `cargo check --features desktop --bin osl-privacy-hub --target x86_64-pc-windows-gnu` | **exit 0**, warnings only (lib 15, bin 8) | exit 0 | pass |
| `npx tsc --noEmit` | **exit 0** | exit 0 | pass |
| `npx vitest run` | **740 passed, 2 failed** | 739 / 3 → 2 | meets |

All cargo commands ran under `flock /tmp/osl-cargo.lock` with `CARGO_BUILD_JOBS=4`. No
`--workspace` or `--all-targets` run.

The 2 remaining frontend failures are the known native-Discord contract failures
(`native-discord-tether.test.ts`, and `overlay.test.ts > applies QA transcript visibility`), owned
by nobody in this lane and left alone as instructed. **The third failure was already gone before I
started** — I take no credit for it.

## New tests added

| Test | File | Proves |
|---|---|---|
| `a_row_this_identity_sent_is_carried_as_outgoing_not_relabelled_incoming` | `broker.rs` | A `SelfToPeer` row carries `Outgoing` end-to-end and serialises as `"orientation":"outgoing"` |
| orientation assertions in `rehydrated_rows_keep_undecodable_rows_instead_of_dropping_them` | `broker.rs` | Orientation exists exactly when plaintext does; exact wire shape pinned |
| `zeroizing_an_inner_identity_clears_every_secret_field` | `keystore/src/storage.rs` | Every secret base64 field is cleared, recovery entropy included |
| `secret_carriers_wipe_themselves_on_drop` | `keystore/src/storage.rs` | `InnerIdentity` and `Identity` are `ZeroizeOnDrop` |

## Required tests NOT delivered — blockers, named honestly

1. **`remove_friend` behavioural tests** (burned-scopes-intact, rollback on a failed People write).
   `security.rs` has **no file-backed test harness** — no temp-dir/unlock helper exists, and
   `GLOBAL_KEYSTORE_TEST_LOCK` is currently dead code. Building one under a shared cargo lane,
   against a process-global storage key, is a change I judged too collision-prone to improvise
   here. The properties are verified by source inspection above, which is weaker than a test.
2. **Inbound `0x0A` applied-before-deletion** has only a **source-shape** test (`broker.rs`, which
   greps its own text for the call order). That is shallow evidence by this project's own standard.
   A behavioural test needs a fake keyserver control-inbox client, which does not exist yet.
3. **Renderer-side orientation test.** `apps/osl-hub-ui/src/overlay.test.ts` is **not in this
   lane's ownership** (I own `overlay.ts` and `security.test.ts` only). The producer contract is
   pinned by the Rust test instead. **Hand-off:** the `overlay.test.ts` owner should add a case
   asserting an `"outgoing"` row renders as outgoing and that text-without-orientation is refused.

## Round 2 — two-identity rig, ipc logging, crypto-shred decision

### `crates/ipc` identifier logging — fixed (this lane now owns the crate)

The stated invariant is that no path logs message identifiers. It was not held: **44 tracing
fields** across `commands.rs` (43) and `migration.rs` (1) interpolated exact identifiers —
`discord_message_id`, `scope`, `scope_storage_key`, `peer`, `sender`, `requester`, `user_id`.

New `crates/ipc/src/log_id.rs` provides `log_id`/`log_id_opt`, and every one of those 44 sites now
emits an opaque token instead. **Why salted, not a plain hash:** snowflakes and message ids are
enumerable 64-bit integers, so `sha256(id)` is trivially reversible by hashing a candidate range.
The salt is 32 random bytes generated once per process and never persisted, so a token cannot be
matched back to a guessed identifier even by someone holding both the log and the candidate list.
Two emissions of the same id within one run produce the same token, so logs stay followable; across
restarts correlation is deliberately lost.

Verified: 4 new tests in `log_id.rs` pass (including one asserting the token is *not* the unsalted
digest); `cargo test -p ipc` fully green; hub Windows desktop check still exit 0. Field names and
log messages were left untouched, and errors/counts/URLs were deliberately not wrapped — confirmed
by grep in both directions (no identifier left raw, nothing non-identifier wrapped).

Also fixed while in the crate: `crates/ipc/tests/register_fix_peer_keys.rs:60`, the third E0509
from the zeroization wave. The ipc test suite compiles again.

### Two-identity rig — what was actually blocked, and what no longer is

`docs/qa/two-identity-p2p-verification.md` §6 is **stale in three places**. I did not edit it —
`docs/**` belongs to the truth lane. Hand-off list:

| §6 item | Status |
|---|---|
| 1. "The drain cannot be driven" | **STALE.** Six verbs now exist (`status`, `host`, `send`, `drain`, `rehydrate`, `reveal-view-once`), each reaching the exact command the renderer calls, addressed per-instance. |
| 4. "Bilateral burn is inert — the drain destroys the notice" | **STALE.** Apply-before-delete landed earlier today; I wired the outbound lane this session. |
| 6. "Orientation is discarded in the renderer" | **STALE.** Fixed this session and carried end-to-end. |

**The real gap was not `main.rs` — it was the harness.** `scripts/qa/osl-p2p-loop.ps1` had never
been updated for the verbs: it still declared "it drives exactly one verb", still hand-printed
operator instructions for the receive side, and its P2/P5/P6 gap texts still cited pre-fix line
numbers and conclusions. Changes made:

- Added `Get-InstanceFileToken` (mirrors `instance_file_token`, and *refuses* >96-char identifiers
  rather than silently disagreeing with the digest branch) and `Invoke-SelftestVerb`, which drives
  one verb on one **named** instance through the addressed rendezvous. The unqualified trigger is a
  global first-consumer-wins rendezvous, so with two instances live a trigger meant for B can be
  eaten by A — addressing is the only safe way to drive B.
- The verb body is written as JSON. A non-JSON body is treated by the app as the **legacy send**,
  so a truncated write would fall through into the one verb with an irreversible side effect; the
  helper never writes a partial body.
- P2 now drives `drain` on B instead of asking a human to click. **This changes the grade
  semantics**: a driven-but-silent drain is now `fail` (real evidence about the receive path)
  rather than `unmeasurable`. `-OperatorDrivesReceiveSide` is retained as an override.
- Parse-checked with the documented `Parser::ParseFile` invocation: **PARSE OK**.

It synthesises no input: it writes a file and reads a file. No `SendInput`, no keyboard injection,
no `PostMessage`, no window touched.

### Machine-safety trap found: do NOT wrap the QA build script in the cargo flock

`scripts/qa/osl-instance-b-build-wsl.sh` **already wraps its own `cargo build` in
`flock /tmp/osl-cargo.lock`**. Wrapping the script in that same lock — which is what the standing
"wrap EVERY cargo command in flock" rule reads like it requires — **self-deadlocks**: `flock` is
not reentrant, so the inner lock waits forever on the outer one held by its own parent. It presents
as a build that produces no output and no cargo process for as long as you let it run, and it
blocks every other lane queued behind it.

Correct usage: `export CARGO_BUILD_JOBS=4` then invoke the script **unwrapped**. The flock rule
applies to cargo commands you issue yourself, not to scripts that already self-lock. Two of my
attempts were lost to this before the process tree showed both `flock` PIDs on the same file.

(Also worth knowing: `/mnt/c/Users/liamw/AppData/Local/Temp/osl-instance-b-build.json` can hold a
**stale** verdict from an earlier tab's run — mine showed a 15:27 keystore failure that had nothing
to do with the current bytes. Check `runStartedAt` before believing it.)

### Round 3 — instance B is built, running, and answering verbs (first real evidence)

**Instance B exists and was driven.** This is the first on-disk evidence from a live OSL process in
this lane.

- Built from current bytes with a freshly built `dist` (so B embeds today's renderer, including the
  orientation fix). Staged to `C:\OSL-QA-B` with `WebView2Loader.dll` — the exe alone hangs before
  `main()` with no window and no trace file.
- **B exe sha256 `cd1ff079ddaa7cfbd53a9a09056e500ac408adb82c913a6892cf8d1a16c573d0`**, distinct
  from A's `3fc0a5ae…`. The build proves the `TAURI_CONFIG` identifier overlay took by grepping the
  identifier out of the artefact before staging.
- Launched as pid 2084, marker class `org.oslprivacy.hubqab-sic`, own `%APPDATA%` root and own
  private temp root, **relocated onto `\\.\DISPLAY5` (non-primary)** so nothing appeared on the
  owner's screen.
- `assert/instance-a-untouched`: **ok** — no A marker appeared during the launch and A's identity
  file is byte-identical by sha256.
- **Drove the read-only `status` verb through the instance-addressed rendezvous and it answered:**
  `status_observed: { pass: true, graded: true }`, advertising all six verbs
  (`status`, `host`, `send`, `drain`, `rehydrate`, `reveal-view-once`) and naming
  `osl-qa-selftest.org.oslprivacy.hubqab.request` / `.json` — exactly the paths
  `Invoke-SelftestVerb` writes and reads. **The harness upgrade is validated against a live
  instance, not just parse-checked.**

Two script defects fixed to get here, both of which produced confidently wrong verdicts:

1. `osl-instance-b-build-wsl.sh` looked for the exe at `$REPO/target/...`, but **`apps/osl-hub` is
   not a workspace member** and cargo writes to `apps/osl-hub/target/...`. A fully successful
   431 MB build was reported as `BLOCKED -- cargo reported success but the exe does not exist`.
   Now checks the hub-local directory first, with the workspace path as fallback.
2. `osl-launch-instance-b.ps1` hard-blocked unless instance A was running, because "A was not
   disturbed" is unprovable without an anchor. Added **`-NoInstanceA`**, which does not weaken that
   guarantee: it is **refused** if an A marker is actually present, and the post-launch assertion
   becomes "no A marker appeared during the launch **and** A's identity file is byte-identical by
   sha256" — the identity comparison is the half that protects the owner's account and it does not
   need A to be running.

### Keyserver 0029 — snowflake lookups: mostly fail closed, one real gap

Verified in source (a codex inventory, with both load-bearing claims re-checked by hand):

**Good — the modern send path already does exactly the right thing.**
`refresh_peer_pubkeys_from_keyserver` (`crates/ipc/src/commands.rs:6088`) refuses a
snowflake-shaped id *before* any HTTP request:
`all ASCII digits && len in 17..=20` → `Err("OSL: Discord identifiers cannot resolve keys")`, and
the caller turns that into a clear "message NOT sent" refusal rather than a silent failure. No
lookup retries internally: every `fetch_pubkeys` is one request, and non-2xx becomes
`Error::HttpStatus`. No negative results are cached, so nothing poisons.

**Gap 1 — the guard exists in only one place.** The legacy receive paths
(`cmd_osl_decrypt_message_with_id`, `resolve_sender_pubkey`) read `osl_user_id` straight out of
`peer_map` and hand it to `fetch_pubkeys` with no shape check. Legacy peer entries genuinely can
hold a snowflake there (`crates/ipc/src/peer_map.rs:242`, `:267` copy a legacy string directly into
`osl_user_id`). Those make one doomed request and return a generic error instead of the clear
"this can never resolve" explanation. **Being fixed** by hoisting the predicate into a shared
`is_discord_snowflake_shaped` helper and applying it at those call sites.

**Gap 2 — RECORD, owner decision. The control-inbox drain has no terminal give-up.** On dispatch
failure the drain logs and **leaves the row in place** (`commands.rs:5420`, "leaving row in place").
That is correct for a transient failure and it is what makes the burn lane recoverable. But a row
whose sender can *never* resolve under 0029 is now retried on **every** drain, forever, with no
bounded attempt count and no user-visible explanation. This is polling, **not** a hang or a
blocking spinner — the user does not freeze — but it is unbounded repetition against a lookup that
is permanently dead, and it consumes the bounded per-pair inbox.

I am **not** fixing Gap 2 unilaterally. Deciding when to discard an authenticated inbox row is the
same class of decision as the burn-notice deletion defect fixed earlier today: delete too eagerly
and a recoverable message is destroyed. The honest fix is a *permanent-refusal* classification —
distinguish "cannot resolve yet" from "can never resolve" (snowflake-shaped sender, or a 0029
refusal) and give only the latter a terminal state with an operator-visible reason.

### Round 4 — contamination, target hardening, duress tri-state

**Instance B was contaminated and has been rebuilt from a clean launch.** Another lane's helper
selected "the first process with a window" and drove six UI steps of this lane's instance B
(pid 2084): skipped the Pro code, continued the privacy page, set cover insertion, skipped
stealth/burn passwords and Mullvad, ticked Brave. Nothing was imported or deleted and no real data
was read, but the instance's state was no longer known, so **it was killed rather than trusted**.
Relaunched clean as pid 25936; the read-only `status` verb re-driven and green.

**Target selection is now unexpressible by name in this harness.** `Invoke-SelftestVerb` requires
`-ExpectPid` and `-ExpectExePath` (both mandatory, and `-ExeB` is now a mandatory parameter of the
loop) and refuses before writing a single byte unless all three of these hold:

1. the named pid is running;
2. that process's image path is exactly this lane's staged build; and
3. the `<identifier>-sic` marker belongs to that same pid.

Verified in both directions on the live machine: B (pid 25936) resolves to
`C:\OSL-QA-B\osl-privacy-hub.exe` and passes; a decoy process (DiscordPTB, pid 3048) resolves to a
different path and is refused. Selecting by window title, process name, or "the one that is
running" is the defect — every OSL build is titled `OSL Privacy` — so the fix is to make the unsafe
selection impossible rather than to be careful.

**Duress wipe on a TPM-less Windows box — fixed as a tri-state, not by lying.** The rejected fix
(`evict_tpm_key() -> Ok(())`) would report successful key destruction when nothing was destroyed.
`evict_tpm_key` now returns `Result<TpmEvictOutcome>` where `NoTpmNothingToEvict` is a success
**only when provably nothing could be evicted**:

| Condition | Result | Why |
|---|---|---|
| Platform crypto provider will not open | `Ok(NoTpmNothingToEvict)` | No provider means no key was ever persisted through it |
| Provider opens, key not found | `Ok(NoTpmNothingToEvict)` | Provably nothing to destroy |
| Provider opens, key deleted | `Ok(Evicted)` | A key existed and is gone |
| Provider opens, key found, **delete fails** | `Err(SealerError::Tpm)` | A key exists and was NOT destroyed |

Mapped in `duress.rs` as `Evicted → Wiped`, `NoTpmNothingToEvict → AlreadyClean`,
`Err → Failed`, so the journal is retained **only** for the last row and a clean QA VM no longer
comes up believing a duress wipe is still in progress.

**A worse defect found while reviewing this:** the previous code did
`let _ = NCryptDeleteKey(key, 0);` — it **discarded the delete result**. A key that existed and
whose deletion *failed* was reported as a successful wipe. That is the same lie the rejected fix
would have introduced, except it was already shipping on machines that do have a TPM. It is now an
`Err` that retains the journal.

### Owner decision recorded — undispatchable control-inbox rows (queued, not yet built)

Per the owner, and matching the burn-notice defect class: **never silently discard an authenticated
row.**

- Snowflake-shaped sender post-0029 → **provably** can never resolve (the shared
  `is_discord_snowflake_shaped` predicate already exists) → retire the row with a recorded reason.
- Everything else → bounded retry with backoff, then **quarantine to a dead-letter state, not
  deletion**.
- **Surface the count.** "3 messages could not be delivered" is honest; an invisible forever-loop
  is not.

Not yet implemented — it needs client-side persistence for attempt counts and the dead-letter set,
which is a design change rather than an edit, and it should not be squeezed in beside the VM work.

### Calibration note on claiming

A8 was **under-claimed** by this lane and the checklist writer awarded +1 after verifying
`ZeroizeOnDrop` and the field-clearing tests in source. The lesson, recorded so it is not
over-corrected in the other direction: a unit test is the *correct and complete* proof for memory
wiping, because the property cannot be observed end to end without reading freed memory. The
"tests are not proof" rule guards against shallow tests standing in for **observable** behaviour —
it does not demote a test that genuinely fits the property being claimed. D6 and C5 stay unclaimed
because they *are* observable and have not been observed.

### Keyserver 0029 — checked in code, not yet live

`0029_authoritative_osl_identity.sql` adds `identity_lookup_enabled` defaulting to `0`, disabling
all existing rows until re-registration; numeric Discord snowflakes are refused and can never
enable. Verified against source:
- `ensure_keyserver_registered` (`commands.rs`) calls `client.register(id)` on **every** launch and
  unlock, not only for new identities — so a quarantined identity re-enables itself on next launch.
  The 0029 recovery path exists.
- `build_register_request` (`keystore/src/client.rs:601`) sends `user_id: identity.user_id` and
  **no snowflake**, so an OSL identity re-registers cleanly. The refusal only bites rows whose
  `user_id` *is* a numeric snowflake.
- **Recorded risk, not verified live:** `Identity` carries a `discord_snowflake` field populated by
  `osl_register_self_snowflake`. Any lookup path keyed on a snowflake rather than an OSL user id is
  now permanently unresolvable. Worth an explicit check by whoever owns peer lookup.

### Crypto-shred — owner decision: NOT NOW

Recorded per the owner's instruction. `crates/store/src/lib.rs`'s `put` never populates
`wrapped_key`, so the burn paths NULL a column that is always already NULL: burn today is row
deletion plus SQLite `secure_delete`, not key destruction.

**Recommended design, for after the deadline:** give each message a random content key `K`, store
the ciphertext under `K`, and store `wrapped_key = seal(store_key, K)` per row. Burning a scope
then overwrites `wrapped_key`, and the message is unrecoverable even from a page-level forensic
copy, because the store key alone no longer opens it. This is a schema migration plus a rewrite of
`put`/`get` and every burn path, and it changes what the product may truthfully claim.

**Not started, deliberately.** The honest short-term position is the one already taken: the threat
model no longer claims it and the claim allowlist bans the phrase. Revisit after 2026-08-02 with
the two-identity rig in place to verify it. **Checklist status: known architectural gap**, not a
defect to be fixed inside a defect lane.

## Record, do not fix

1. **`crates/store/src/lib.rs` — cryptographic burn is not implemented.** `put` (the only normal
   writer) never populates `wrapped_key`; every message is sealed under one store-wide key derived
   from the identity secret (`cipher::derive_key(identity_secret)`). The burn paths
   (`wipe_wrapped_keys_in_scope`, and the `wrapped_key = NULL` sites) therefore destroy a column
   that is always already NULL. Burn today is row deletion plus `secure_delete`, not key
   destruction.
   **Recommended design (owner decision — large blast radius):** give each message its own random
   content key `K`, store the ciphertext under `K`, and store `wrapped_key = seal(store_key, K)`
   per row. Burning a scope then overwrites `wrapped_key` and the message is unrecoverable even
   from a page-level forensic copy of the DB, because the store key alone no longer opens it. This
   is a schema migration plus a rewrite of `put`/`get` and every burn path, and it changes what the
   product may truthfully claim, so it should not be started inside a defect lane.
2. **`broker.rs` — Received and Opened receipts collapse into one `acknowledgmentCount`** and their
   order is lost. The operator cannot distinguish "delivered" from "read", and cannot see which
   happened first.
3. ~~**`crates/ipc` tracing emits plaintext identifiers.**~~ **FIXED in round 2** — this lane was
   given the crate. Original inventory retained for the record: **72
   interpolating tracing statements, 35 of which emit an identifier** — concentrated in
   `commands.rs`, emitting `discord_message_id`, `scope`, `scope_storage_key`, `peer`, `sender`,
   `requester`, `msg_id`, `user_id` (e.g. `commands.rs:189, 292, 301, 488, 1935, 1955, 2043, 2056,
   2076, 2220, 2686, 3977, 3990, 4171, 4356, 4490, 4786, 4800, 4822, 4839, 4964, 4993, 5071, 5127,
   5306, 5352, 5357, 5366, 5394, 5400, 5417, 7694, 10299, 10305`, plus `migration.rs:143`).
   Recommended: opaque truncated-hash formatting in logging paths. Not this lane's crate.
4. ~~**`crates/ipc/tests/register_fix_peer_keys.rs:60` does not compile**~~ — **FIXED in round 2.**
   Third E0509 from the zeroization wave; the ipc test suite is green again.

## Checklist rows — nothing claimed

Rows owned: A1–A8, B1–B7, D6, and the attribution part of C5. **No `earned:` value was changed.**

Reasoning, per the standing rule that closing a code defect does not earn a point until the finding
is genuinely closed: D6 is `open-security-finding` and its control lane is now wired rather than
inert, but there is no runtime or two-identity evidence that a burn request actually crosses
between two devices — and the keyserver revocation lane's deployment state is still a contested
claim in this repository. C5's attribution defect is fixed in code and unit-proven at the
producer, but the renderer half is proven only structurally and nothing has been observed on
screen. Claiming either would be exactly the "code written = point earned" failure the rule exists
to prevent.

## Round 5 — two confirmed defects IN THIS LANE'S OWN WORK

Both were found by an adversarial review this lane commissioned against itself, and both were then
verified in source by hand rather than accepted on the reviewer's word. **Both are cases where
making a path work converted a dormant inaccuracy into a delivered claim.**

### Defect 1 — carrier replay: a peer can make OSL label their own row "You"

**Mechanism.** `prose_token_recv_classified` (`crates/ipc/src/prose_token.rs:268`) derives its MAC
key from `derive_scope_primitives(scope_input)` — **the scope only**. There is no binding to the
Discord row, to the posting account, or to a per-message nonce. So one cover text authenticates
identically in *any* row of that conversation, whoever posted it.

**Trigger.** The operator sends a protected message; its cover is public text visible in Discord.
The peer copies that still-live cover into a new message they post. The eye decodes the peer's row,
the wire verifies as signed by the local identity, `authenticate_oriented_prose_pointer` resolves
`SelfToPeer`, and `overlay.ts` renders the row as outgoing with `author: localIdentity`.

**This lane made it worse.** Before tonight every decoded row was stamped `incoming` / friend —
wrong for the operator's own rows, but it never asserted the operator authored a row a peer posted.
The orientation work introduced that assertion.

**Impact.** No new plaintext is exposed; the ciphertext is the operator's own message. The harm is
transcript integrity: the peer chooses where and when the operator's words appear and OSL certifies
them as the operator's.

**Real fix** binds the token to the posting identity or a per-row nonce — a wire change.
**Immediate mitigation is an owner decision**, because failing closed on `SelfToPeer` rows restores
safety but removes the operator's ability to read their own half of a transcript, which the code
comments at `broker.rs` explicitly argue is required for a transcript to be a transcript.

### Defect 2 — bilateral burn: an authenticated receipt for enforcement that does not happen

**Mechanism.** `apply_peer_revocation` (`apps/osl-hub/src/security.rs`) durably records a burn floor
and returns an ack with `applied: true`. The gate that would refuse peer content below that floor,
`admit_peer_content_seq` (`security.rs:2435`), together with `peer_scope_commitment` (`:2416`) and
`next_peer_send_seq` (`:2387`), has **zero callers anywhere in the repository** — verified by
grepping all `*.rs`; each name resolves only to its own definition, plus one comment at `:2525`
referencing `admit_peer_content_seq`. Protected content also carries no sequence for such a gate to
check (`PeerProtectedPayload`, `broker.rs`).

**This lane made it reachable.** Before tonight the ack was produced and dropped, because nothing
posted the outbox — so no false assurance ever left the device. Wiring the outbound lane means the
sender now receives a cryptographically authenticated receipt attesting to enforcement the receiver
has not implemented.

**Impact.** An operator can be shown that a peer honoured their burn while that peer retains no
enforceable floor over the affected content. Worse than a visible delivery failure, because the
receipt authenticates a claim the implementation has not made true.

**Recommendation.** Until the admission gate is wired and content carries a sequence, the receipt
should assert only what is true today — that the burn floor was *recorded* — rather than implying
enforcement. That wording change is small; wiring the gate is a wire-format decision, not a defect
fix.

### The pattern, stated plainly

Both defects share one shape: **this lane made a path work without re-reading what the working path
would then assert.** Connecting something is not the same as making its claim true, and the moment
a surface becomes reachable is exactly when to re-read what it promises. Recorded here because it
is a failure mode of the author, not of the code.

### Also this round

- **Both hub gates now pass**, and the permanent expected-failure is gone. The
  `native_discord_adapter` header-proof test asserted `header_proof_is_enforced()` unconditionally
  while that function is defined as `!cfg!(feature = "discord-qa-shell")`. It was a *test* defect,
  now cfg-aware. A standing red that everyone is told to ignore trains people to ignore red.
- **Neither gate alone is honest.** `--features core` omits `qa_selftest_request` — the module
  deciding whether a trigger becomes a status read or the irreversible send. `--features
  core,discord-qa-shell` compiles it but turns header-proof enforcement **off** by design. Both runs
  are needed, and a count without its gate means nothing.
- **Error strings were leaking identifiers.** The earlier redaction pass covered `tracing::` macros
  only and missed `format!` error strings, which reach the UI and are logged by callers. Now
  tokenised via named arguments; one site deliberately left because a test asserts its exact text.
- **keystore tracing break: RESOLVED.** The dependency is declared and both gates plus the Windows
  check pass. This lane guessed the cause twice before asking, and the second guess (the sibling
  worktree) was wrong — that tree's keystore contains no `tracing::` calls at all. Recorded because
  a confidently wrong diagnosis costs another lane real time.

## Acceptance rows this earns

**None. This lane claims nothing, and two rows moved backwards.**

| Row | State after tonight | Why not earned |
|---|---|---|
| **D6** Bilateral Burn | **Regressed to a confirmed defect** | The outbound lane is wired and every unit gate passes, but the receipt now delivers an authenticated claim of enforcement that has no implementation (Defect 2). Not merely unproven — actively misleading. Needs the admission gate wired, content sequencing, and a burn observed crossing two identities. |
| **C5** (attribution part) | **Regressed to a confirmed defect** | Orientation is carried end to end and fails closed on unproven rows, but a peer can replay a cover to have their row labelled as the operator's (Defect 1). Needs token-to-poster binding, or the fail-closed mitigation and its product trade-off accepted. |
| **A7** Honest Burn / duress | **Cannot be earned by this lane's work at all** | See below — the duress engine has no production caller, so the wipe cannot fire. The TPM tri-state made an unreachable subsystem honest. |
| **A8** Secret zeroization | Already awarded by the checklist writer | Not claimed here. Recorded only so the +1 is not double-counted. |
| **B-rows** | Unchanged | Nothing in this lane produced two-identity evidence. |

### A7 — the duress engine is unreachable, verified

The checklist's A7 note ("duress/auto-lock/10-attempt burn have no production caller") is **correct
and now verified with a mechanism rather than assumed.** `crates/keystore/src/lib.rs:50` re-exports
the whole duress surface — `DuressEngine, DuressError, DuressHandlers, DuressJournal, DuressPaths,
DuressReport, StepOutcome, WipeFn, WipeStep`. Grepping every `*.rs` under `apps/`, `crates/ipc/`
and `crates/store/`:

```
DuressEngine: 0   DuressPaths: 0   DuressHandlers: 0   DuressReport: 0   WipeStep: 0
```

Zero uses outside `crates/keystore`. Nothing in the application can invoke the wipe.

**This corrects a statement this lane made earlier tonight.** I said A7 needed "a duress wipe on a
TPM-less VM, journal cleared" as its evidence. That is not runnable: you cannot observe a wipe that
nothing can trigger. The honest next step for A7 is **wiring, not testing** — and wiring a
destructive subsystem to a production trigger is an owner decision, not a defect fix.

It also right-sizes tonight's TPM tri-state work: it made an unreachable subsystem *honest*, which
is worth having before it is ever wired, but it moves no acceptance row. The same "zero callers"
check found both of tonight's other defects; it is now three for three in this lane.

`unknown` is the honest state for every B row this lane touches; **A7 is not unknown but
structurally unearnable until the engine has a caller.** Truth should judge and apply; this lane has
deliberately not edited the checklist.

## Gate numbers RE-MEASURED at end of session

Both hub gates were re-run in the tree as it stood at the end of the session, after other lanes had
committed, rather than left as figures measured earlier:

| Gate | Result |
|---|---|
| `--features core` | 685 passed, 0 failed, 1 ignored |
| `--features core,discord-qa-shell` | 742 passed, 0 failed, 1 ignored |

Unchanged from the earlier run, so the numbers quoted above are current and not inherited. This
lane spent the session telling other people that an inherited count is not a measurement; that
applies to its own counts measured an hour earlier in a tree that has since moved.

Both gates are still required. `--features core` omits `qa_selftest_request` — the module deciding
whether a trigger becomes a status read or the irreversible send. `--features
core,discord-qa-shell` compiles it but turns header-proof enforcement off by design
(`header_proof_is_enforced()` is `!cfg!(feature = "discord-qa-shell")`), so a pass under it is not
evidence that enforcement holds.

## In flight at end of session — a systematic unreachable-subsystem sweep

The "zero callers" check found something real **three times tonight**, each time by accident:

| Subsystem | Where | What it appeared to promise |
|---|---|---|
| `post_due_revocations` | `broker.rs` | The outbound burn lane. Fully written, never called. |
| `admit_peer_content_seq`, `peer_scope_commitment`, `next_peer_send_seq` | `security.rs:2435/2416/2387` | Burn enforcement. A recorded floor that nothing consults. |
| Entire `DuressEngine` surface | `keystore/src/duress.rs`, re-exported at `lib.rs:50` | A duress wipe that cannot be triggered. |

Three for three is a pattern, not a coincidence: **these subsystems pass their unit tests precisely
because nothing exercises them in anger.** It is the same family as a double that cannot fail — the
code is correct in isolation and inert in practice — and in two of the three cases something
downstream was claiming the behaviour worked.

A read-only sweep for the rest is running; its output lands at
`scratchpad/codex-deadcode.log` in this session's scratch directory. It covers
`crates/keystore/src/**`, `crates/ipc/src/**` (excluding `wire_rn.rs`, the ratchet lane's) and
`apps/osl-hub/src/**`, discounting `#[cfg(test)]` callers, doc mentions and bare re-exports, and
excluding anything reachable through `generate_handler!`.

**Partially spot-verified — 4 of its claims checked by hand, all 4 confirmed.** It is not fully
reviewed, so re-grep anything before acting on it, but it is calibrated rather than unknown:

| Claim spot-checked | Observed |
|---|---|
| `post_wrapped_key` unused | 0 non-test hits outside `crates/keystore` |
| `fetch_wrapped_key` unused | 0 non-test hits outside `crates/keystore` |
| `BurnAlertPayload` unused | 0 non-test hits outside `crates/keystore` |
| `osl_notes` / `osl_lan` / `osl_assets` not wired | 0 mentions in `apps/osl-hub/src/main.rs`, so not in `generate_handler!` |

**Two of these matter beyond this lane and are routed, not fixed:**

1. **Wrapped-key and prekey-bundle APIs are not in the production path.** `post_wrapped_key` and
   `fetch_wrapped_key` have no callers, so any claim that the app uses the wrapped-key service for
   production message-key exchange is unsupported. Relevant to what the B rows may state; this lane
   is not asserting anything either way about what the live path *does* use, only that it does not
   use these.
2. **`osl_notes`, `osl_assets`, `osl_lan` and siblings have UI-facing command strings but no
   wiring into `generate_handler!`.** The user-visible copy exists; the backend is not reachable.
   That is a claim-surface question and belongs with the truth lane, alongside its in-app copy
   sweep — a feature named in the UI whose backend is inert is exactly the kind of thing an in-app
   claim gate would not catch, because the string is present and truthful-looking.

`BurnAlertPayload` being inert also means any "burn alert is signed and verified" statement is
currently a claim about unreachable code — the same shape as A7's duress engine.

### The full result, captured here because the scratch path is session-scoped

**17 subsystems with zero production callers.** Each column-3 entry is what someone would wrongly
believe if they assumed the code ran — which is the reason this table exists. None is reachable
from `generate_handler!` or `main`.

| Symbol | Where | What it would be claiming if it ran |
|---|---|---|
| `osl_notes::{list,upsert,…}` | `osl_notes.rs:112,:169` | Notes/documents/searches/revisions encrypted at rest and exposed via `list_osl_notes`/`save_osl_note`. **UI command strings exist; no backend command is registered.** |
| `osl_assets::{list,begin,append,finish,read_chunk}` | `osl_assets.rs:62,:87,:153,:200,:224` | Chunked, encrypted, hashed, quota-bound asset vault linked to notes |
| `osl_lan::{host,join,sync_host,sync_guest,stop}` + `osl_collab::{create_invitation,seal_frame,open_frame}` | `osl_lan.rs:108…`, `osl_collab.rs:53,:106,:130` | Local-only encrypted LAN rooms, invitations, frame AEAD, no cloud relay |
| `decode_office_asset` | `osl_formats.rs:103` | Office files locally decoded into encrypted editable notes |
| `osl_plugins::{inspect,run}` | `osl_plugins.rs:45,:58` | Encrypted `.oslmod` plugins run in a deny-by-default, fuel-limited WASM sandbox |
| `DuressEngine` / `DuressHandlers` / `WipeStep` | `keystore/duress.rs:242,:154,:70` | A duress password triggers a resumable wipe of identity, prekeys, TPM/keyring material, caches |
| `BurnAlertPayload` / `sign_burn_alert` / `verify_burn_alert` | `keystore/burn_alert.rs:34,:75,:83` | Recipients get authenticated burn-alert messages proving the sender authored them |
| `KeyServerClient::burn` / `BurnScope` | `keystore/client.rs:919`, `burn.rs:35` | A local burn also deletes keyserver wrapped keys |
| `PrekeyState` / `replenish_prekeys` / `fetch_prekey_bundle` | `keystore/prekeys.rs:101`, `client.rs:831,:758` | Signed SPK/OPK replenishment and one-time prekey fetch are live production flows |
| `WrappedKeyUpload` / `post_wrapped_key` / `fetch_wrapped_key` | `keystore/wrapped_key.rs:15`, `client.rs:805,:782` | Message keys are uploaded to and fetched from the wrapped-key service |
| `next_peer_send_seq` / `peer_scope_commitment` / `admit_peer_content_seq` | `security.rs:2387,:2416,:2435` | Every protected message carries a sequence, and content below a burn floor is refused before rendering |
| `plan_local_burn` / `plan_remote_friend_burn` / `apply_remote_consent_revocation` / `BurnReplayJournal::accept` | `burn_contract.rs:150,:271,:206,:388` | Burns require signed confirmations, Pro gating, revocable consent, replay journals |
| `OfflineControlQueue` / `ReceivedControlJournal` | `control_contract.rs:127,:189` | Burn/expire/receipt controls queued, retried, deduped, replay-protected offline |
| `TimedMessageState::{new,record_open,evaluate}` | `control_contract.rs:291,:315,:339` | First-open timers and absolute expiry drive key destruction |
| `opened_receipt_status` | `control_contract.rs:420` | Opened receipts are Pro-gated, consent-bound and revocable |
| `ComposerOverlayGuard` / `DecryptionOverlayGuard` / `VisiblePlaintextCache` | `external_overlay.rs:155,:241,:366,:484` | Overlays fail closed on verified context/geometry/focus; visible plaintext bounded and zeroized |
| `NativeAttachmentJobRegistry` / `NativeAttachmentSecrets` | `native_attachment_jobs.rs:173,:83` | Attachments staged through authenticated job state; secrets never serialize or clone |

**Read this as a list of candidate false claims, not a list of defects.** Unreachable code harms
nobody by existing; it harms when a checklist row, a threat model or a UI string describes it as
behaviour. Several of these describe exactly the properties this product sells — LAN-only
collaboration, a sandboxed plugin system, expiry that destroys keys, overlays that fail closed.

**Four were verified by hand** (`post_wrapped_key`, `fetch_wrapped_key`, `BurnAlertPayload`, and
the `osl_notes`/`osl_lan`/`osl_assets` wiring). **The other thirteen are Codex's counts and are
unverified** — re-grep before acting on any of them.

### How to triage these — the count is not the interesting number

A subsystem nobody claims is dead code and can wait indefinitely. One that is *sold* is a live false
claim. So the ranking that matters is by **harm surface**, not by caller count:

| Bucket | Meaning | What to do |
|---|---|---|
| **SOLD** | A user or auditor is currently told this works — an in-app string, the threat model, the public claim allowlist, or website copy | Live false claim. Either wire it or withdraw the claim. Urgent. |
| **INTERNAL ONLY** | Only a checklist row or internal design doc mentions it | Correct the row's status. Not user-facing, not urgent. |
| **UNCLAIMED** | Nothing asserts it | Genuinely just dead code. Leave it; deleting is its own risk. |

Two searches are running to fill this in: a re-verification of the thirteen unverified counts
(required to quote the actual command and its raw output per entry, and to distinguish "no callers
anywhere" from "no callers under the cfgs I compiled with" — this tree is heavily `#[cfg]`-gated and
those are different claims), and a search for what asserts each subsystem to a human.

**Boundaries for whoever completes this:** anything touching `crates/store` is the store lane's;
anything touching `docs/` or the website is the truth lane's. This lane reports, it does not reach.
The `osl_notes` case is already routed to truth, since the claim gate is theirs.

**Keep hand-verified entries marked separately from delegated counts.** That distinction is the
only thing making this table safe to act on.

**Why it is worth finishing:** every entry is a candidate for a checklist row or a user-facing claim
that asserts something inert. That is exactly the shape of both defects confirmed above.

## Resume here

- **Current verified state:** all five lane gates green (keystore 176/0/1, store 17/0, hub core
  679/0/1, Windows desktop check exit 0, frontend 740 pass / 2 known fail, tsc 0). The bilateral
  burn outbound lane is wired for the first time. Sender attribution is carried end-to-end and
  fails closed.
- **Exact build/worktree:** `osl-eye-and-features-2026-07-26` @ `fc6b983`, dirty. No binary was
  linked; `cargo check` does not produce one, so there is no sha256 to record.
- **Round 2 additions:** `crates/ipc` identifier logging closed (44 sites, salted per-process
  tokens, 4 new tests); `osl-p2p-loop.ps1` upgraded to drive the receive side through the
  instance-addressed verb rendezvous (parse-checked OK); instance B being rebuilt from current
  bytes with a freshly built `dist`.
- **Next unblocked action, in order:**
  1. Finish the instance-B build, launch B alone, and drive the read-only `status` verb. That
     proves the addressed rendezvous works end-to-end **and** doubles as the live 0029
     re-registration regression test — neither needs a second Discord account nor instance A.
  2. Build a file-backed test harness for `security.rs` (temp `osl_config_dir` + unlock) and land
     the two `remove_friend` tests.
  3. Add a fake control-inbox client so the inbound `0x0A` apply-before-delete test can be
     behavioural instead of source-shape.
  4. Full P1–P6 two-identity run — this is what would actually move D6/B6.
- **Human prerequisites for a full run** (§2 of the QA doc; none of these can be done unattended):
  a second Discord account logged in where B can reach it; instance A running and adopted to a QA
  conversation; `-ConfirmCreatesIdentity` (a real production keyserver write); and
  `-ConfirmDriveLiveConversation` (posts a real message into A's live conversation).
- **Note on B's build identity:** the WSL build script deliberately reuses the `dist` instance A
  embedded, so that a renderer difference cannot be confused with a product difference. That is not
  possible here — `overlay.ts` changed today, so `dist` was rebuilt and **B now embeds today's
  renderer while the existing A binary does not**. Any A-vs-B comparison must account for this;
  proving today's fixes requires rebuilding both sides.
- **Known blockers/risks:** the 0027 deployment claim is still unresolved in-repo (the client is
  built to survive either answer); `overlay.test.ts` (renderer orientation test) belongs to the
  frontend-test owner and was not touched; `docs/qa/two-identity-p2p-verification.md` §6 items 1, 4
  and 6 are stale and belong to the truth lane.
- **Master/checklist rows to update on completion:** none from this run.

## Round 6 — residual proofs and two owned follow-up defects

Starting revision inspected: `5822a8f993ef36b306f0e06532d59eb5167efacb`. HEAD later
advanced through unrelated server/truth commits with no overlap in these source paths. No live
Discord session and no second identity were used. Everything in this section is
`test-proven-only` unless explicitly marked `blocked`.

### Residual 1 — file-backed friend-removal proof was already present

The requested gap no longer existed at current HEAD, so it was not rewritten.
Exact evidence:

- `security.rs:3737-3770` has `FileBackedSecurityHarness`, serialised by
  `GLOBAL_KEYSTORE_TEST_LOCK`, with active-account directory and file-key restoration in `Drop`.
- `security.rs:4367-4421` behaviourally proves terminal burned scopes survive a real
  file-backed `remove_friend`.
- `security.rs:4424-4448` forces the People atomic write to fail and proves the peer map is
  restored on disk.
- `osl-cargo test --features core,discord-qa-shell --lib
  security::tests::remove_friend_preserves_burned_manual_scopes_on_disk -- --exact
  --test-threads=1`: 1 passed, 0 failed.
- The same command for
  `security::tests::remove_friend_rolls_back_peer_map_on_people_write_failure`: 1 passed,
  0 failed.

Verdict: `test-proven-only`; already committed by `c42da82`, not claimed again here.

### Residual 2 — real/fake control-inbox seam

The earlier closure test was behavioural, but the requested fake control-inbox client did
not exist. It now does:

- Production routes the `0x0A`/`0x0B` retirement effects through
  `KeyserverRevocationControlInboxClient` (`broker.rs:4847-4891`), which uses the real
  authenticated post/delete methods.
- `FakeControlInboxClient` (`broker.rs:6268`) observes the same seam. The positive case
  classifies literal v3 `MSG_TYPE_REVOCATION` (`0x0A`), marks the durable apply complete,
  then observes `apply_notice`, `post_ack`, `delete_row` in that order. Its delete method
  asserts the durable apply flag, so moving deletion ahead of apply fails the test.
- Deferred rows produce no fake-client effect and remain counted; terminally
  unappliable rows delete without pretending a durable burn happened.
- `osl-cargo test --features core --lib broker::tests::inbound_revocation --
  --test-threads=1`: 2 passed, 0 failed.
- The same filter under `--features core,discord-qa-shell`: 2 passed, 0 failed.

Verdict: `test-proven-only`. This is not a two-identity burn and does not earn D6/B6.

### Follow-up 1 — marker availability command was implemented-unwired

Re-verification confirmed the renderer invoked `discord_marker_available`, its ACL already
named the command, and `generate_handler!` did not register it. The strengthened wiring
gate was run before the fix and failed exactly on missing
`discord_marker_available,`.

The read-only Tauri command now returns
`NativeDiscordComposerState::marker_available()` (`main.rs:1424-1427`) and is registered
at `main.rs:7270`. The wiring loop now checks permission declaration, exact command
binding, capability grant, handler registration, and the annotated function itself
(`security.test.ts:216-226`); the last assertion is the negative control that prevents
the same-named status DTO field from satisfying the gate.

Verdict: `test-proven-only`. `npx vitest run src/security.test.ts`: 4 passed, 0 failed.
The production Windows desktop check compiled the command and handler successfully.

### Follow-up 2 — transcript watchdog retry accumulation

Re-verification confirmed a never-settling invoke caused its watchdog to clear
`rehydrateBusy` and self-schedule another invoke every timeout. The new gate was run
before the fix and failed because the unbounded watchdog contained
`scheduleTranscriptRehydrate();`.

The watchdog now spends `rehydrateWatchdogReplacementUsed` before scheduling one
automatic replacement (`overlay.ts:821,905-909`). If that replacement also never
settles, it cannot schedule another by itself. A genuine edge received while a read is
stuck is captured separately by `externalRetryPending` (`overlay.ts:891,905`), consumed
before scheduling, and therefore also cannot chain without another external edge. A
settled read resets the allowance (`overlay.ts:920`); cancellation clears the latch.

Verdict: `test-proven-only`. `security.test.ts:244-279` proves the latch precedes the
only watchdog schedule, the scheduler cannot reset it, and an external wheel edge still
reaches the scheduler. `npx tsc --noEmit` exited 0. The focused overlay/security run was
53 passed / 1 failed; the one failure is the pre-existing
`applies QA transcript visibility directly from the trusted header event` geometry
contract and is unrelated to this change.

### Gate boundaries

- `osl-cargo check --features desktop --bin osl-privacy-hub --target
  x86_64-pc-windows-gnu`: exit 0, warnings only.
- The equivalent `desktop,discord-qa-shell` Windows check is `blocked`: four
  pre-existing `E0004` matches in `main.rs` do not cover the newly added
  `ListBrowserProfiles`, `GrantBrowserProfile`, `RevokeBrowserProfile`, and
  `RunBrowserImport` variants from another in-flight owner.
- Both Linux desktop checks are `blocked` before hub code by `rfd` requiring a
  `gtk3` or `xdg-portal` backend.
- `git diff --check` over the changed lane paths: exit 0.

No runtime, verified-live, or two-identity claim is made. No acceptance row is earned by
these source and test proofs.

## Round 7 — QA-shell browser-profile verb policy

Starting revision: `e83a487`. No live process, Discord session, or second identity was used.

The requested pre-fix command was:

`osl-cargo check --features desktop,discord-qa-shell --bin osl-privacy-hub --target
x86_64-pc-windows-gnu`

It exited 101 with exactly four `E0004` non-exhaustive-match errors in `main.rs`: `run_timeout`
(then line 5115), `Observation::ready_for` (then line 5439), `insert_verb_criteria` (then line
5925), and `run_receive_side_verb` (then line 6358). Each error named the same four missing
variants: `ListBrowserProfiles`, `GrantBrowserProfile`, `RevokeBrowserProfile`, and
`RunBrowserImport`.

The QA shell has no production driver for those operations. It now accepts each spelling only
long enough to return a distinct named refusal (`main.rs:5107-5110,6412-6427`), bypasses a
misleading readiness wait (`main.rs:5451-5457`), and inserts a distinct graded-false criterion
(`main.rs:5955-5986`). No wildcard arm was added. Therefore future `Verb` growth remains
compiler-enforced at all four match sites.

Compiler exhaustiveness cannot by itself prove the policy remains refused and graded false.
`security.test.ts:244-298` pins both properties for all four verbs and has negative controls
against wildcard arms in the criteria and driver matches.

Focused evidence:

- `osl-cargo check --features desktop,discord-qa-shell --bin osl-privacy-hub --target
  x86_64-pc-windows-gnu`: exit 0; `Finished dev profile ... in 6.47s`; 14 existing library
  warnings and 8 binary warnings.
- `osl-cargo check --features desktop --bin osl-privacy-hub --target
  x86_64-pc-windows-gnu`: exit 0; `Finished dev profile ... in 3.84s`; 15 existing library
  warnings and 8 binary warnings.
- `npm test -- --run src/security.test.ts`: 1 file passed, 5 tests passed, 0 failed.

Verdict: `test-proven-only` for the compile-time and source-policy gates. The verbs themselves
are explicitly unavailable and fail closed. This earns no runtime or two-identity claim.

### Evidence correction for Round 6

Store's subsequent exact-archive audit found that commit `e83a487` does not contain the IPC and
keystore dependency closure needed to compile the reported broker/security focused tests.
Accordingly, Round 6's control-inbox and file-backed friend-removal behavioural verdicts are
corrected from `test-proven-only` to `blocked`. The passing commands recorded there exercised
later dirty-tree dependencies and are not evidence for the exact committed bytes. They remain
blocked until an explicit minimal prerequisite commit and an exact-archive mutation run pass.

## Round 8 — exact-archive repair and adversarial behavioural proof

Store's correction in Round 7 was confirmed. Exporting exact `e83a487` and running

`osl-cargo test --features core --lib broker::tests::inbound_revocation --
--test-threads=1`

failed before test execution with 20 IPC compiler errors. They reduced to five missing owned
source dependencies. Commit `1d8bfa8bf55a5e83a47d4b69753d89844d65fa69` contains only that
closure:

- sender-bound v3 decryption and its mismatch error (`crates/ipc/src/wire_v2.rs:363,790-834`);
- the complete TOFU `KeyBundle`, comparison, and safety-number API
  (`crates/ipc/src/tofu.rs:24-94`);
- the persisted peer bundle (`crates/ipc/src/peer_map.rs:147-158`);
- the non-rendered pending alert bundle (`crates/ipc/src/state.rs:173`);
- the complete-bundle registration-signature verifier
  (`crates/keystore/src/client.rs:222`).

The other dirty IPC/keystore files were excluded. In particular, only the missing
`verify_peer_bundle` hunk from `keystore/client.rs` was committed; its separate dirty capability
and network-fetch changes remain uncommitted.

### Exact committed baseline

`git archive 1d8bfa8` was extracted to a disposable directory. With one reusable target
directory, these commands ran against only those archived bytes:

- `osl-cargo test --features core --lib broker::tests::inbound_revocation --
  --test-threads=1`: 2 passed, 0 failed, 662 filtered out.
- `osl-cargo test --features core --lib
  security::tests::remove_friend_preserves_burned_manual_scopes_on_disk -- --exact
  --test-threads=1`: 1 passed, 0 failed, 663 filtered out.
- `osl-cargo test --features core --lib
  security::tests::remove_friend_rolls_back_peer_map_on_people_write_failure -- --exact
  --test-threads=1`: 1 passed, 0 failed, 663 filtered out.

### Five independent one-sided mutations

Each mutation started from a separate copy of that exact archive and ran only its owning
focused test:

1. `Applied` changed to non-retiring: failed because `deferred_rows` was 1 instead of 0.
2. `Deferred` changed to retiring: failed with
   `control-inbox row deleted before revocation apply completed`.
3. `delete_row` moved before `apply_row`: failed with the same ordering assertion before the
   apply closure could set its durable flag.
4. `withdraw_person_grants` changed to clear `burned_manual_scopes`: failed because the
   file reloaded after `remove_friend` no longer contained the terminal burned scope.
5. The `persist_friend_removal` People-write rollback was removed: failed with
   `failed People write must restore the peer map on disk`.

Verdict: the Round 6 control-inbox ordering/retirement proof and both file-backed
`remove_friend` behaviours are restored to `test-proven-only`, now on exact committed bytes and
with independent one-sided mutation failures. The dependency closure itself is
`test-proven-only` only to the extent required to compile and run those focused tests; this
report makes no broader claim about the still-dirty IPC/keystore work.

No live process, provider, Discord conversation, runtime rig, or second identity was used. This
does not earn a runtime, verified-live, or two-identity claim.

## Acceptance rows this earns

None.
