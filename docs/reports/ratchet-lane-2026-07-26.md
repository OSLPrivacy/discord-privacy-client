# Ratchet lane — 2026-07-26

Lane owner: Claude Opus 5. Exclusive ownership: `crates/osl-ratchet-next/**`,
`crates/ipc/src/wire_rn.rs`. Everything else is read-only to this lane; changes needed
elsewhere are recorded below as exact diffs to be routed, not applied.

Standing constraint honoured: **nothing was enabled.** OSL-RN carries no real traffic. Master
§7.11 requires external cryptographic review before uncontrolled traffic, and
`crates/osl-ratchet-next/DESIGN.md` carries the same standing instruction. No feature was
flipped on, no gate was defaulted to enabled.

---

## Five plain lines

1. The ratchet reuses its AEAD nonce deterministically if sender state ever rolls back, because
   the nonce is derived from the message key alone — this is the finding that matters most.
2. B4 was not unstarted — capability negotiation is already built across three layers, the server
   half is deployed live, and the real defect was a `bool` parameter that let any future caller
   void the whole downgrade proof.
3. No OSL client advertises its capability at all, so negotiation is inert end to end and would
   select v=3 for every peer, permanently.
4. The advertisement must be switched on in the same commit as the wire-in and never before,
   because advertising a capability the build cannot honour pins peers into an unrecoverable
   refusal state.
5. Nothing was enabled, nothing outside this lane's two paths was edited, and every test claimed
   as proof here was watched failing first.

---

## 1. Status correction: B4 is `implemented-unwired`, not unstarted

The lane brief said B4 (capability negotiation and the monotone downgrade pin) was UNSTARTED.
Source evidence contradicts that, and per master §0.1 rule 5 current source evidence settles
implementation status. B4 exists across three layers:

| Layer | Where | State |
| --- | --- | --- |
| Keyserver | migration `0026_rn_capability_advertisement.sql`; `RN_CAP_WIRE_RN = 1` at `keyserver-cf/src/lib/signed-request.ts:46` | **Deployed live.** Master §9 records the escalation closed: keyserver `3f92f0f5`. |
| Client verification | `crates/keystore/src/client.rs:301` `verify_peer_capabilities`, `PeerCapabilities{Absent,Unverified,Verified}` | Implemented, with 10+ negative tests at `crates/keystore/tests/client_test.rs:744-850`. |
| Version selection | `crates/ipc/src/wire_rn.rs:281` `select_wire_version`, `RnPeerPin` at `:232` | Implemented, monotone, with downgrade tests. |

**The capability bitmap IS covered by the identity signature** — verified directly, not assumed.
`reg_msg_with_capabilities` (`crates/keystore/src/client.rs:151`) appends the bitmap to REG_MSG,
and `verify_peer_bundle` (`:220`) reconstructs and verifies that exact byte string. A non-zero
bitmap cannot be laundered through the legacy encoding: `:258` returns false for any non-zero
bitmap before the legacy fallback is attempted. The brief's downgrade-oracle concern is
therefore already closed on this axis.

---

## 2. Finding: the advertisement half does not exist (routed, not fixed)

This is the most consequential finding of the session and it is outside this lane.

`build_register_request` (`crates/keystore/src/client.rs:601`) signs the **legacy** `reg_msg(...)`
at `:609`, not `reg_msg_with_capabilities`. `RegisterRequest` (`:63-87`) has **no
`rn_capabilities` field at all** — the client cannot advertise even if it wanted to.

Confirmed by grep: `verify_peer_capabilities` has **zero production callers**, and
`reg_msg_with_capabilities` is called only from within `client.rs` itself and from tests.

Consequence: every OSL client registers as a legacy peer. Every peer therefore resolves to
`PeerCapabilities::Absent`, so `select_wire_version` under the default `RnPolicy::Opportunistic`
returns `LegacyV3` for **every peer, permanently**. Capability negotiation is inert end-to-end.
Wiring the ratchet without fixing this would produce a system that appears to negotiate and in
fact sends v=3 one hundred percent of the time.

### Why this is NOT being routed as a standalone fix

Advertising `RN_CAP_WIRE_RN` means "I speak OSL-RN". That is false for every build shipping
today, because the ratchet is unwired. If it were switched on early:

- peers would read `Verified(RN_CAP_WIRE_RN)`, select `SelectedVersion::Rn`, and raise the
  sticky pin;
- the local build cannot actually produce an OSL-RN message, so the send fails;
- the pin has **no lowering operation at all** (`crates/ipc/src/wire_rn.rs:31-33`), and
  `RnError::PinnedToRn` (`:147`) has no recovery path.

**Recommendation (this lane's judgement):** the advertisement bit must be derived from the same
constant as the wire-in feature gate and flipped in the same commit. It must never ship ahead of
the code that honours it. Routing this as an isolated "fix the manifest of capabilities" task
would be actively harmful.

### Operational hazard this creates, for the owner to note

Once advertisement is live, a user who **downgrades OSL to a pre-ratchet build while keeping the
same identity key** becomes permanently unreachable from every peer that pinned them: the peer's
pin stays raised, the downgraded client stops advertising, and `select_wire_version` returns
`Err(PinnedToRn)` forever. Losing the identity key resets it (the pin is keyed by peer identity
X25519, `wire_rn.rs:390`), but a plain version rollback does not. This is fail-closed by design
and is the correct security tradeoff, but it needs a documented recovery procedure before the
advertisement ships. There is none today.

---

## 3. Finding: a doc claim in keystore is stronger than the code (routed, not fixed)

`verify_peer_capabilities` doc comment (`crates/keystore/src/client.rs:289-293`) claims a
stripped or lowered bitmap "lands in `Unverified` ... so tampering cannot be laundered into
'not capable, send v=3 instead' without also being visible."

That is true for a *lowered* bitmap, but **not** for a *fully stripped* field. The function
returns `Absent` at `:302-304` before any signature check runs, and again at `:305` for an
explicit zero. Neither path verifies anything.

Calibration, stated honestly: **this is not a live vulnerability.** `verify_peer_bundle` is
called in production at `crates/ipc/src/commands.rs:5872`, and a record whose owner signed a
non-zero bitmap fails that check once the field is stripped, so the record is rejected wholesale
before use. The guarantee is real; it is just split across two functions and one call site
rather than held by the function whose doc claims it.

Routed recommendation: correct the doc comment, and consider fusing the two calls so a future
caller cannot get the weaker half alone. Not applied — `crates/keystore/**` is another lane's.

---

## 4. Recorded, deliberately not fixed: the root manifest is false

Root `Cargo.toml:25-28` states:

```toml
    # Isolated, UNWIRED research crate. Reachable only from its own
    # tests; no other crate or app depends on it. See
    # crates/osl-ratchet-next/DESIGN.md — unreviewed, must not carry
    # real traffic before external cryptographic review.
    "crates/osl-ratchet-next",
```

"no other crate or app depends on it" is false. `crates/ipc/Cargo.toml:73` declares:

```toml
osl-ratchet-next = { path = "../osl-ratchet-next" }
```

Corrected wording would be: *"Reachable only from `crates/ipc/src/wire_rn.rs`, which is itself
unwired — no send path calls it."* That is both true and preserves the warning's intent.

**Not applied on purpose** — but the stated reason needs correcting, because this lane's first
rationale was wrong. The original argument was that editing a root manifest forces a full
workspace rebuild while other lanes build. That cost has **already been paid**: other lanes have
themselves modified root `Cargo.toml` tonight, adding `crates/adapter-profile` and
`crates/exposure-warning` as members. So "avoid triggering a rebuild" is no longer a live reason.

The reasons that do still hold:

1. The lane instruction to record and not fix this is explicit, and other lanes editing the file
   does not grant this lane permission to.
2. `Cargo.toml` is under active concurrent edit, so an edit here risks a genuine collision with
   whichever lane is mid-change.

This is a comment-only correction with no functional effect. It should be batched by whichever
lane next legitimately touches the root manifest.

---

## 5. Work dispatched

Per the lane's dispatcher mandate, implementation was delegated to Codex (`gpt-5.5`), reviewed
in both directions on return. Evidence sections below.

| Job | Scope | Files it was permitted to change |
| --- | --- | --- |
| A | B4 — replace the `bool` seam with `PeerCapabilities` | `crates/ipc/src/wire_rn.rs` only |
| B | B3 — mutation-prove the recovery suite, then fill holes | `crates/osl-ratchet-next/tests/**` only |
| C | B2 — read-only wire-in costing | one new doc, no source |

### Job C evidence — wire-in costing (complete)

Output: `docs/reports/ratchet-wire-in-costing-2026-07-26.md` (685 lines). Verified it stayed in
bounds: it created that one file and ran no cargo, as instructed.

Reviewed in both directions. What it got right, and what this lane checked independently:

- **It independently confirms the §2 finding.** It grepped and found no production call to
  `verify_peer_capabilities` — eleven call sites, all in `crates/keystore/tests/client_test.rs`.
  It further found that `KeyServerClient::fetch_pubkeys` (`crates/keystore/src/client.rs:739`)
  calls only `verify_peer_bundle`, never `verify_peer_capabilities`. Two independent derivations
  of the same conclusion.
- **The re-decryption blocker is still true and it did not soften it.** It cited four separate
  paths that re-open the same stored ciphertext: history rehydration
  (`apps/osl-hub/src/broker.rs:1745-1822`, which explicitly consumes nothing at `:1737`), the
  control-inbox text drain (`:2666-2829`, deleting only after success at `:2806`, so a failed
  delete guarantees a re-decrypt), attachment listing (`:3622-3680`), and the generic
  control-inbox drain (`crates/ipc/src/commands.rs:5306-5420`). It listed four options and
  refused to pick one, as instructed. Its verdict: "There is no cheap local workaround."
- **It found a real defect in this lane that this lane had missed** — see §6 below.
- **There is nowhere to put a verified capability today.** `PeerEntry`
  (`crates/ipc/src/peer_map.rs:67-144`) has no capability field, and `ManualPeerBinding`
  (`apps/osl-hub/src/security.rs:165`) carries only ids and keys. So the plumbing is a real
  schema change, not a parameter pass.
- **One good catch this lane would not have made:** if the TOFU outcome is `Changed`, the
  existing code skips live key writes at `crates/ipc/src/commands.rs:5922`, and a capability
  write must skip too — otherwise a newly signed capability gets attached to an old trusted key.

### Job A evidence — capability seam closed (complete, accepted)

Scope honoured: `crates/ipc/src/wire_rn.rs` only, 172 insertions / 25 deletions. Verified by this
lane that A did **not** touch the root manifest — the +10 lines there are other lanes adding
`crates/adapter-profile` and `crates/exposure-warning`.

`select_wire_version` now takes `keystore::client::PeerCapabilities` instead of a `bool`. No
`From<bool>`, no deprecated shim, no test-only bool constructor. The truth table is unchanged and
the pin is still consulted before capability, so `SelectedVersion::LegacyV3` stays structurally
unreachable for a pinned peer.

All four negative controls were observed failing before they passed:

| Test | Failure text when the source was deliberately broken |
| --- | --- |
| `unverified_capabilities_do_not_enable_rn_or_downgrade_pinned_peer` | `Unverified capabilities on a pinned peer must fail closed` |
| `unverified_raised_bitmap_cannot_promote_unpinned_peer_to_rn` | `an unverified raised bitmap must not select OSL-RN` — `left: Rn, right: LegacyV3` |
| `verified_zero_bitmap_does_not_read_as_capable` | `Verified(0) must not select OSL-RN opportunistically` — `left: Rn, right: LegacyV3` |
| `selection_is_exhaustive_over_pin_capabilities_and_policy` | `expected LegacyV3 for pin=UNKNOWN caps=Unverified policy=Opportunistic, got Ok(Rn)` |

**Verification was re-run by this lane, not accepted as reported.** A's pasted tail showed only
integration binaries with `0 tests ... filtered out`, which looks like evidence and is not. This
lane ran `flock /tmp/osl-cargo.lock -c "cargo test -p ipc --lib wire_rn"` directly:
`test result: ok. 27 passed; 0 failed`, with all four new tests present by name.

### Job B evidence — recovery suite mutation-proven (complete, accepted)

Scope honoured: `crates/osl-ratchet-next/tests/negative.rs` only, +71 lines. This lane verified
independently that `git diff --stat -- crates/osl-ratchet-next/src` was **empty** on completion,
so every mutation was restored.

Part 1 mutation audit — every required row was broken deliberately and observed failing:

| Test | Mutation | Failed? | Observed failure |
| --- | --- | --- | --- |
| `duplicates_are_rejected_and_leave_the_session_usable` | accept a replayed message number | Yes | `immediate replay must be rejected` |
| `evicted_keys_fail_closed_and_do_not_corrupt_the_session` | disable per-chain + global eviction (`skipped.rs:208,219`) | Yes | `assertion failed: bob.skipped_key_count() <= 16` |
| `skipped_storage_stays_bounded_over_a_long_lossy_run` | disable global eviction (`skipped.rs:219`) | Yes | `global key cap breached at step 18: 411` |
| `a_single_absurd_gap_is_refused_without_deriving_anything` | make `check_skip` always accept (`skipped.rs:174`) | Yes | `must refuse: Opened { .. }` |
| `random_interleavings_deliver_correctly` / `permanent_loss_never_stalls_the_session` / `reverse_order_delivery_within_a_chain` / `state_export_import_survives_an_interleaved_run` | targeted per-property breaks | Yes | incl. `deliver after restore: AuthFailed` |

**Partial decoration finding, reported rather than buried:**
`an_entire_chain_lost_forever_does_not_wedge_the_ratchet` covers a lost prefix of a *newly
observed* DH chain, not a previous-chain-drain failure. A stricter DH-drain mutation **stayed
green** — so that test does not protect the property its name implies. This is the single most
useful line in B's report and it is an honest negative result.

New tests added, each shown failing first:

- `sender_rollback_replays_send_state_but_receiver_rejects_and_recovers` — broken by not
  advancing the receiver's chain key after decrypt; failed with `receiver silently accepted a
  sender-rollback duplicate`. **This test is what surfaced §6.**
- `skipped_message_replay_stays_rejected_after_restart` — broken by making `SkippedKeys::take`
  read without removing; failed with `accepted skipped-message replay after restart`.

Torn/truncated state blobs were **not** duplicated: `negative.rs::corrupt_session_state_is_refused_not_mis_parsed`
already covers that, and B correctly declined to add a second copy.

Final verification: `flock /tmp/osl-cargo.lock -c "cargo test -p osl-ratchet-next"` — full crate
green, including the 6 `pq_healing` tests in 21.07s.

**Deviation recorded, not hidden:** B ran its individual mutation tests *outside* the flock,
using bare `cargo test -p osl-ratchet-next --test <name>`. Cargo's own build-directory lock still
serialised them, so there was no corruption risk and the queuing behaviour was identical, but it
is not what the prompt specified. This lane let the audit finish rather than kill it mid-run. The
final verification command was run under flock as instructed.

**Staleness to note:** C's §4 step 4 proposes passing `binding.peer_capabilities.supports_rn()`
into `select_wire_version`. That is the *old* `bool` signature. Job A ran concurrently and
removed that parameter in favour of `PeerCapabilities` itself. Anyone acting on C's document must
apply job A's signature, not C's. This is a consequence of parallel dispatch, not an error by C.

**Concurrency note for whoever routes §2 and §3:** `crates/keystore/src/client.rs` is under
active edit by another lane right now (`verify_peer_bundle` is being strengthened with an
`RN_CAP_MAX` range check and a dual-encoding fallback). This lane verified that those edits do
**not** touch `build_register_request` — it signs the legacy `reg_msg` at both HEAD and working
tree — so the §2 finding stands. But the route target is moving; re-check before applying.


---

## 6. SECURITY FINDING — deterministic nonce reuse on sender state rollback

This is the most consequential technical result of the session. It was surfaced by job B's
crash-recovery test and then **verified independently at source by this lane** rather than
relayed on trust.

### First, what was ALREADY known — this is not a discovery

`crates/osl-ratchet-next/THREAT-MODEL.md` already carried a **Nonce-misuse** entry stating that
the body nonce is derived from the message key, that safety depends on each message key being
used exactly once, that "a state-restore bug that resurrected a consumed key would produce nonce
reuse under a reused key, which is catastrophic for XChaCha20-Poly1305," and — pointedly —
"There is a test for the restore case; there is no proof."

The mechanism was therefore documented before tonight. Presenting this as a newly discovered
vulnerability would be an overclaim of exactly the kind this lane spent the session policing in
delegated work. What is new is narrower, and it is these four things:

1. **Status change from hypothetical to reachable.** The existing entry frames the trigger as "a
   state-restore *bug*." Job B demonstrated it is reachable through *ordinary* sender rollback —
   a crash between emitting the wire and persisting the advanced state — with no bug required.
2. **The proof the entry said was missing.** "There is no proof" is now false. Job D's
   `send_rn_reload_after_crash_does_not_reuse_the_previous_wire`, broken deliberately, failed
   with `left: "reused-wire", right: "reused-wire"` — the identical wire emitted twice.
3. **The operational trigger was not connected.** Routine VM snapshot restore is this exact case.
4. **A mitigation now exists** at the integration layer where none did.

`osl_ratchet_next::kdf::body_nonce` (`crates/osl-ratchet-next/src/kdf.rs:130-132`) derives the
AEAD body nonce from the message key **and nothing else**:

```rust
pub fn body_nonce(message_key: &Secret32) -> Result<[u8; AEAD_NONCE]> {
    hkdf::<AEAD_NONCE>(message_key.as_bytes(), IKM_NONCE, LABEL_BODY_NONCE)
}
```

There is no per-message randomness in it. Therefore **any reuse of a message key is a
deterministic reuse of the nonce**. Two different plaintexts sealed under one key and one nonce
is a two-time pad: an attacker holding both ciphertexts recovers the XOR of the plaintexts, and
for XChaCha20-Poly1305 the authentication guarantee degrades as well.

Job B confirmed the reachable path by test: after a sender state rollback the code reuses the
same message key and body nonce. Its exact words — "sender rollback is not prevented ... even
without RNG rollback, the code reuses the same message key and body nonce after sender state
rollback."

### Why the existing test does not catch it

`crates/osl-ratchet-next/src/kdf.rs:217 body_nonce_is_key_bound` asserts only that *different*
keys produce different nonces. It cannot detect reuse under the *same* key, which is precisely
the rollback case. It is a reassuring test that does not cover the hazard — the pattern this
project has been burned by repeatedly.

### The mitigating factor, stated honestly

The receiver rejects the replayed counter, so this does **not** cause silent desynchronisation,
and job B proved that rejection is real by mutation. But receiver-side rejection does not undo
the emission: if the sender rolls back and then sends *different* content, the second ciphertext
is already on the wire under the reused key and nonce. The receiver refusing it is irrelevant to
the attacker who recorded both.

### What this lane did about it, and what it deliberately did not

**Applied — the ordering fix, in job D's `send_rn` wrapper:** the wrapper persists the advanced
session state to disk *before* returning the wire string to its caller. A crash in that window
then costs a lost message instead of a key reuse. A lost message is recoverable; a two-time pad
is not. This is a call-ordering property in this lane's own file and needs no protocol change.

**NOT applied — job B's proposed nonce-binding change.** B proposed binding the body nonce to
per-message wire randomness at `crates/osl-ratchet-next/src/session.rs:560` and `:632`. That is a
**wire-format change to unreviewed cryptography**. Master §0.2 rule 6 forbids this lane from
autonomously choosing between materially different wire and persistence behaviours, and §7.11
plus `DESIGN.md` require external cryptographic review. It is recorded here as a finding **for
that review**, with B's proposed shape preserved, and is not applied.

### Residual that the ordering fix does not close

Persist-before-emit defeats the crash window. It does **not** defeat a sender whose sealed state
is restored wholesale from an older backup, a filesystem snapshot rollback, or a VM restore.
That residual needs either the nonce-binding change above or an explicit anti-rollback measure,
and it is a genuine input to the external review — not something to resolve here.

**This residual is operational, not theoretical.** The store lane reported on 2026-07-26 that
schema v4 bumps `SCHEMA_VERSION`, and instructed the vm and scrub lanes that their WARM snapshots
predate v4 and will be restored and re-pointed at live profiles. Restoring a snapshot is exactly
the sender-state-rollback case above. OSL-RN is not carrying traffic, so nothing is affected
today — but any wire-in plan must treat "the team routinely restores machine snapshots" as a
standing property of the deployment environment, not an edge case. An anti-rollback measure is
therefore not optional hardening; it is a precondition for this environment.

Note the scope difference so this is not over-read: the v4 bump affects the **store crate's
SQLite database**. `RnSessionStore` writes separate sealed per-peer files and is not gated by
`SCHEMA_VERSION`. The connection is the *snapshot-restore practice*, not the schema change.

---

## 7. In-lane defect found by job C: the wire version in `session.rs` doc header is wrong

`crates/osl-ratchet-next/src/session.rs:4` documents the protocol as:

```
//! # Wire format, version `0x06`
//!     version(1) = 0x06
```

The authoritative constant in the same file is `pub const WIRE_VERSION_RN: u8 = 0x10;`
(`crates/osl-ratchet-next/src/session.rs:148`). Verified directly, not taken on trust.

This is not cosmetic. `MIGRATION.md` explains that `0x10` was chosen **specifically because**
`0x06` is already `wire_v2::MSG_TYPE_SKDM_REQUEST` in the message-type namespace, and that this
product classifies opaque inbox bundles by reading raw bytes at fixed offsets. A module header
telling an integrator the wire version is `0x06` invites exactly the mis-dispatch that choosing
`0x10` was meant to make impossible.

**How it was found is worth recording.** This defect was surfaced by job C — a *read-only
documentation and costing* task that was forbidden from running cargo or touching source. Two
code-focused jobs and this lane's own reading of the same file had all missed it. It corroborates
the store lane's 2026-07-26 observation that a delegated documentation draft caught the v3/v4
attachment AAD migration bug: accurate prose disagrees with inaccurate code, and that
disagreement is the signal. A second independent instance of the same effect in one night is a
reason to keep commissioning documentation as an audit instrument, not only as output.

**Status: FIXED.** Lines 4 and 8 now read `0x10`. The `0x06` references at `:73-103` were left
untouched on purpose — those are the deliberate explanation of *why not* `0x06`, and they are
correct.

The fix was deferred until job B finished, because B mutates and restores this crate's `src/` and
ends by asserting `git diff --stat -- crates/osl-ratchet-next/src` is empty. Editing during that
window would have made B report a false restore failure. Note for anyone reading the diff: `src/`
is no longer empty against HEAD, and that is this lane's later doc fix, **not** a failure by job B
to restore its mutations. B's restore was verified clean before this edit was made.

---

## Acceptance rows this earns

Truth applies points; this lane never edits the checklist. Stated as claims with their evidence,
for the checklist owner to accept or reject.

| Row | Claim | Status | Evidence |
| --- | --- | --- | --- |
| B4 capability negotiation | Peer ratchet support is discoverable, authenticated, and pinned; bitmap covered by the identity signature | **Partial — do not award full credit** | Signature coverage verified (§1). Pin monotone and structurally enforced. **But no client advertises (§2), so the mechanism is inert end-to-end.** |
| B4 monotone downgrade pin | A pinned peer cannot be sent a legacy v=3 message | see §5 evidence | Enforced by control flow, not caller discipline; `wire_rn.rs:286-295`. Seam closed by job A. |
| B3 persistence and recovery | Sealed state with replay/reorder/skipped-key/crash tests | see §5 evidence | Claim rests on job B's mutation audit, not on the tests passing. |
| B2 wire-in | Prepared behind a gate, not enabled | see §5 evidence | Nothing enabled. Costing in job C's document. |

**Explicitly earns nothing:** no two-identity proof was run, no VM rig was used, and no
encryption row that depends on that proof is claimed here.
