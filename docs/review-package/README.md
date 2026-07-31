# OSL independent cryptographic review package

**Plan unit:** `b13` — assemble review package.
**Not in scope of this document:** `b14` (engaging a firm) is a human step and has not been taken.

This package exists to satisfy the machine-checked bundle contract
`crypto_review_package_bundle_definition` in
[`docs/reports/crypto-lane-2026-07-26.md`](../reports/crypto-lane-2026-07-26.md).
That contract's JSON is the authority; this document is the artifact it describes.
The nine `requiredEvidence` items are the nine `##` headings below, named exactly.

**Read this first — three statements that govern everything else:**

1. **The reviewed tree is dirty and was moving while this package was assembled.**
   It is not equal to any commit. See §1 and `evidence/reviewed-file-hashes.txt`.
2. **Every test count in §3 was produced by the author of this package re-running the
   suite in this working tree.** No count is inherited from an earlier report.
   (`inherited_test_count_without_focused_rerun` is an invalid-package condition.)
3. **Where authority, consent, binding, a key, a sealer, or a review gate is absent, the
   correct answer here is "refused", never "probably fine".** Where this package could not
   establish something, it says *not verified* and does not estimate. In particular this
   package makes **no runtime claim about the shipping application** — see §9.

---

## Reader's map

| File | What it is |
|---|---|
| `README.md` (this file) | The package. Nine evidence sections. |
| `evidence/01-test-runs.md` | Exact commands, exact nextest summary lines, environment. |
| `evidence/02-reachability-and-caller-search.md` | Every search run, verbatim, and the call graph it produced. |
| `evidence/03-at-rest-and-sealer.md` | Sealer trait, implementations, selection, store schema, zeroization. |
| `evidence/04-wire-and-api-contracts.md` | OSL-RN wire/state formats, negotiation, legacy v=3, keyserver API. |
| `evidence/nextest-tail-excerpt.txt` | Raw captured nextest output (tail-truncated — see §3). |
| `evidence/reviewed-file-hashes.txt` | SHA-256 of every reviewed file, pinning the dirty tree. |

**Suggested reading order for a reviewer with limited hours:**

1. `crates/osl-ratchet-next/DESIGN.md` and `THREAT-MODEL.md` (the protocol and its own stated limits).
2. `crates/osl-ratchet-next/src/{handshake,kdf,session,pq,skipped,negotiate}.rs`.
3. `crates/ipc/src/wire_rn.rs` (the integration/persistence/negotiation layer).
4. `crates/ipc/src/wire_v2.rs` `encrypt_v3` — **this is what actually ships today**, and it has
   no ratchet. If review hours are scarce, spend them here rather than on OSL-RN.
5. `crates/keystore/src/sealer.rs` + `crates/store/SECURITY.md`.

---

## 1. commit_tree_branch_dirty_state_date

Captured by the author of this package, in the repository working tree.

| Field | Value | Command |
|---|---|---|
| HEAD commit | `1eaa7b5fbddc441228b215dd51a990675b996f4d` | `git rev-parse HEAD` |
| HEAD tree | `c942b1b9d11f2abe1d7b20c4af47bb7021aca20c` | `git rev-parse HEAD^{tree}` |
| Branch | `batch-verify` | `git branch --show-current` |
| HEAD subject | `Merge branch 'hb-broker' into batch-verify` (authored `2026-07-30 22:45:07 -0700`) | `git log -1` |
| Dirty entries at start | **22** | `git status --porcelain \| wc -l` |
| Dirty entries at end | **28** | same command, re-run |
| Assembly window | `2026-07-31T06:52:12Z` → `2026-07-31T06:58:36Z` (local `2026-07-30 23:52–23:58 PDT`) | `date -u` |

### The tree is DIRTY. This is stated, not hidden.

`git status --porcelain` at the end of assembly:

```
 M Cargo.lock
 M apps/osl-hub-ui/src/crypto-review-package.test.ts
 M apps/osl-hub-ui/src/security.test.ts
 M apps/osl-hub-ui/src/whatsapp-qa-entry.test.ts
 M apps/osl-hub-ui/vite.config.ts
 M apps/osl-hub/src/broker.rs
 M apps/osl-hub/src/lib.rs
 M apps/osl-hub/src/main.rs
 M apps/osl-hub/src/native_surface_capture.rs
 M apps/osl-hub/tauri.conf.json
 M apps/osl-hub/tests/native_discord_receive_e2e.rs
 M apps/osl-hub/tests/osl_launch_instance_b_contract.rs
 M apps/osl-hub/tests/osl_p2p_pair_script.rs
 M apps/osl-hub/tests/qa_p2p_loop_b6.rs
 M crates/adapter-profile/src/contract.rs
 M crates/ipc/src/state_reload.rs
 M crates/keystore/src/client.rs
 M crates/osl-ratchet-next/tests/interleaving.rs
 M docs/design/osl-internal-build-checklist.md
 M keyserver-cf/scripts/canonical-rollout-admission-contract.mjs
 M keyserver-cf/scripts/configure-telegram-operators.test.ts
 M scripts/qa/a11ybench/capture.ps1
 M scripts/test_osl_vm_discord_uia_harness.py
 M src-tauri/Cargo.toml
?? apps/osl-hub-typecheck/
?? apps/osl-hub/tauri.whatsapp-qa.conf.json
?? docs/review-package/
?? src-tauri/Cargo.lock
```

Three consequences a reviewer must not be shielded from:

- **Two files inside the reviewed roots are uncommitted edits**: `crates/keystore/src/client.rs`
  and `crates/ipc/src/state_reload.rs`. `crates/osl-ratchet-next/tests/interleaving.rs` is also
  uncommitted, so one of the test suites re-run in §3 is not the committed suite.
  **Checking out `1eaa7b5` will not reproduce what was reviewed here.**
- **The tree changed measurably during the ~6-minute assembly window.** `git status` went
  22 → 28 entries (one of which is this package). Independently measured line counts moved
  during the same window: `apps/osl-hub/src` 144,503 → 144,510 lines; `crates/keystore/src`
  13,159 → 13,167 lines. Other work was landing concurrently.
- **Therefore the only reproducible pin for this review is the file-hash manifest**,
  `evidence/reviewed-file-hashes.txt` (397 files, SHA-256 each). Its own SHA-256 is
  `570ebf67b012d97f90845d58e39be44c7decef2e274566fa8d4694ff643959f9`. A reviewer should verify
  each file they read against that manifest and treat any mismatch as "you are reading a
  different artifact than the one this package describes".

**A dirty tree is not an acceptable state to hand to an outside firm.** Before `b14`, the
reviewed roots should be frozen on a tag or a dedicated branch with a clean `git status`, and
this section regenerated. This package does not claim otherwise.

---

## 2. reviewed_source_roots

Measured in this working tree by the author of this package
(`find <root> -name '*.rs' | wc -l` and `find <root> -name '*.rs' -exec cat {} + | wc -l`).

The bundle contract's `reviewedRootMinimum` is
`["apps/osl-hub/src", "crates/ipc/src", "crates/keystore/src", "crates/crypto/src",
"touched_worker_endpoints_or_migrations"]`. All five are covered, plus three additional roots
that the cryptography cannot honestly be reviewed without.

### Contract-minimum roots

| Root | `.rs` files | Lines | Why it is in scope |
|---|---:|---:|---|
| `apps/osl-hub/src` | 87 | 144,510 | The shipping Tauri application. Owns the send/receive call sites, the Discord adapter, and the Tauri command registration surface. |
| `crates/ipc/src` | 37 | 41,093 | Wire formats (`wire_v2.rs`, `wire_rn.rs`), command layer (`commands.rs`), app state, password/master-key handling, local store adapters. |
| `crates/keystore/src` | 24 | 13,167 | Identity generation and persistence, the `Sealer` trait and all implementations, keyserver client, prekeys, duress/burn. |
| `crates/crypto/src` | 16 | 5,830 | Primitives layer: AEAD (XChaCha20-Poly1305, AES-256-GCM), X25519, Ed25519, ML-KEM-768, HKDF, PQXDH, padding, the legacy ratchet. |
| **Worker endpoints / migrations touched** | — | — | `keyserver-cf/src` (75 `.ts` files, 16,822 lines) and `keyserver-cf/migrations` (**39** `.sql` files). Specifically in scope: `src/index.ts` (routing), `src/lib/signed-request.ts`, `src/endpoints/register.ts`, `src/endpoints/pubkeys.ts`, and migrations `0026_rn_capability_advertisement.sql`, `0027_control_inbox_revocation_lane.sql`. |

### Additional roots reviewed (not optional, in this reviewer's judgement)

| Root | `.rs` files | Lines | Why |
|---|---:|---:|---|
| `crates/osl-ratchet-next` (`src`) | 12 | 5,047 | The protocol this review exists for. Plus `DESIGN.md` (607 lines), `THREAT-MODEL.md` (233), `MIGRATION.md` (452). |
| `crates/store/src` | 5 | 5,861 | At-rest message/attachment database, schema migrations, burn. Named by the contract's at-rest evidence item. |
| Test roots | — | — | `crates/osl-ratchet-next/tests` (8 files / 2,379 lines), `crates/crypto/tests` (13 / 3,659), `crates/keystore/tests` (14 / 5,093), `crates/ipc/tests` (62 / 14,876), `crates/store/tests` (9 / 9,005), `apps/osl-hub/tests` (27 / 11,392). |

### Roots deliberately NOT reviewed

`crates/{stego,transport,selectors,message-lifecycle,runtime,adapter-profile,cover-draft}`,
`apps/osl-hub-ui`, `src-tauri`, `services/`, `keyserver/` (the legacy Node/Fastify server),
`cipher-store-cf`, `scripts/`, `infra/`, `tools/`. See §9.

---

## 3. owning_unit_behavior_tests

**Every number in this section was produced by re-running the suite in this tree during the
assembly window. Nothing here is copied from a prior report.**

Environment: `OSL_CARGO_DISK_CLASS=warm`, `OSL_CARGO_JOBS=3`, `rustc 1.88.0 (6b00bc388 2025-06-23)`,
`cargo-nextest 0.9.140 (a9fef2964 2026-07-05)`, `x86_64-unknown-linux-gnu`.
Runner: `osl-cargo` (a wrapper holding a repo-global build lock — see `evidence/01-test-runs.md`).
Suites were run **sequentially**, `--test-threads=1`.

| # | Command | Verbatim nextest summary line | Exit |
|---|---|---|---:|
| 1 | `osl-cargo -C . nextest run -p osl-ratchet-next --test-threads=1` | `Summary [  76.376s] 108 tests run: 108 passed, 0 skipped` | 0 |
| 2 | `osl-cargo -C . nextest run -p crypto --test-threads=1` | `Summary [  33.905s] 201 tests run: 201 passed, 0 skipped` | 0 |
| 3 | `osl-cargo -C . nextest run -p keystore --test-threads=1` | `Summary [  11.319s] 289 tests run: 289 passed, 2 skipped` | 0 |

**Total: 598 tests executed, 598 passed, 2 skipped, 0 failed.**

### The 2 skipped keystore tests — identified, not glossed

A second, confirming run of suite 3 was made specifically to read the skip banner:

```
    Starting 289 tests across 15 binaries (2 tests skipped)
     Summary [   6.982s] 289 tests run: 289 passed, 2 skipped
```

The `#[ignore]` attributes in `crates/keystore/tests/` that account for skips:

- `crates/keystore/tests/live_keyserver_smoke.rs:14` — `#[ignore = "mutates the explicitly
  configured live test keyserver"]`.
- `crates/keystore/tests/recovery_reseal_test.rs:157` — `#[ignore = "needs a real, persistent OS
  keyring backend (Windows Credential …"]`.
- `crates/keystore/tests/recovery_reseal_test.rs:190` — `#[ignore = "needs a real Windows TPM
  behind the Microsoft Platform Crypto …"]` (also `#[cfg(windows)]`, so it does not count as a
  skip on this Linux host).

**Reviewer consequence:** the TPM and OS-keyring sealer paths — the two sealers
`select_best_sealer()` actually prefers in production — **were not exercised by any test run in
this package.** Their correctness here is *source-reviewed only*. This is called out again in §9.

### What the passing suites do and do not cover

- `osl-ratchet-next` (108) covers: known-answer vectors and determinism (`kat.rs`), negative /
  tamper behaviour (`negative.rs`), handshake binding and downgrade (`negotiation.rs`), PQ epoch
  healing (`pq_healing.rs`), reordering/loss/interleaving (`interleaving.rs`), bounds
  (`bounds.rs`), carrier budget (`carrier_budget.rs`), healing latency (`healing_latency.rs`).
- `crypto` (201) covers the primitive wrappers: `aead`, `x25519`, `ed25519`, `hkdf`,
  `ml_kem_768`, `pqxdh`, `padding`, `wire`, plus the legacy ratchet and sender-keys.
- `keystore` (289) covers identity, storage, password, prekeys, duress, burn, license cache,
  sealer, zeroization, and username client vectors.

**Not run, and therefore not claimed:** `-p ipc`, `-p store`, and the `apps/osl-hub` test binaries.
Those packages hold `wire_rn.rs`, `wire_v2.rs`, `commands.rs` and the store schema — i.e. a large
part of the code this package asks a reviewer to look at. Their test counts are **not verified
here**, and this package quotes none. Running them was outside the time budget of unit `b13` and
the build lock was contended by concurrent work. This is a gap, and it is listed in §9.

Full detail, including the raw captured output, is in `evidence/01-test-runs.md` and
`evidence/nextest-tail-excerpt.txt`.

---

## 4. negative_control_statement

The contract requires an explicit statement of *what inversion would fail these tests* — i.e. how
a reader knows the suites can turn red rather than being decorative.

### 4.1 What was actually demonstrated (a real, if narrow, control)

A selection control was run to prove the reported counts are produced by live test selection and
are not a constant echoed from a report:

```
$ osl-cargo -C . nextest run -p osl-ratchet-next --test-threads=1 \
      -E 'test(this_test_name_does_not_exist_control)'
    Starting 0 tests across 9 binaries (108 tests skipped)
     Summary [   0.000s] 0 tests run: 0 passed, 108 skipped
error: no tests to run
```

The same harness that reported `108 tests run: 108 passed` reports `0 tests run` when the filter
matches nothing, and exits non-zero. **This proves the counter is live and the runner reports
failure states. It does NOT prove any individual assertion is strong.** It is offered as exactly
what it is.

### 4.2 What was NOT demonstrated, and why

**No mutation experiment was performed.** The honest and complete control would be to invert a
primitive — e.g. make `aead_open` ignore the Poly1305 tag, make `chain_step` return a constant,
or make `RnPeerPin::raise_to_rn` a no-op — rebuild, and show the specific tests that turn red.
That was not done, because the author of this package is scoped to `docs/**` and is explicitly
forbidden from editing `crates/**`, `apps/osl-hub/src/**`, or any test, and other work was landing
in the same tree concurrently. **A reviewer should not accept §4.3 below as equivalent to a
mutation run.** Performing that mutation matrix is a recommended first action for the reviewing
firm, and it is cheap: each of the suites above rebuilds and runs in well under two minutes.

### 4.3 The falsifiability mechanism, per suite, stated precisely

These are the specific inversions that would fail specific named tests. Each is derived from
reading the test source; each is a concrete, checkable prediction the reviewer can run.

**`osl-ratchet-next` — the suite is structured as an inversion battery already.**

| Inversion | Test that must fail (`crates/osl-ratchet-next/tests/…`) |
|---|---|
| AEAD tag ignored on body open | `negative.rs::every_single_bit_flip_is_rejected_and_the_session_survives`; `negative.rs::random_garbage_never_panics_and_never_opens` |
| Header AD no longer chains version/flags/preamble | `negative.rs::a_header_from_one_message_on_the_body_of_another_is_rejected` |
| Length framing not validated | `negative.rs::truncation_at_every_length_is_rejected`; `negative.rs::malformed_framing_is_rejected_by_category` |
| Skipped-key store allows reuse of a consumed message key | `negative.rs::a_restored_session_cannot_reuse_a_consumed_message_key`; `negative.rs::skipped_message_replay_stays_rejected_after_restart` |
| Session id / binding not mixed into `SK` | `negative.rs::a_message_from_a_different_session_is_rejected`; `negotiation.rs::bound_and_unbound_sessions_are_not_interchangeable` |
| Negotiation digest stops covering the initiator identity or the responder KEM key | `negotiation.rs::a_rewritten_initiator_identity_fails_closed`; `negotiation.rs::substituting_the_responder_kem_key_fails_closed` |
| Version floor no longer enforced (downgrade permitted) | `negotiation.rs::a_floor_disagreement_fails_closed` |
| Any change to the key schedule, wire layout, or label constants | `kat.rs::known_answer_vectors`; `kat.rs::a_frozen_wire_blob_still_decrypts`; `kat.rs::protocol_is_fully_deterministic_given_its_randomness` — these are frozen vectors, so *any* schedule drift is caught |
| Bootstrap forgery accepted | `negative.rs::a_forged_bootstrap_cannot_establish_a_session`; `negative.rs::a_non_bootstrap_message_cannot_start_a_session` |
| PQ epoch inferred rather than announced, or advanced unilaterally | `pq_healing.rs::unidirectional_traffic_cannot_complete_a_pq_epoch`; `pq_healing.rs::pq_epochs_advance_and_both_sides_agree` |
| Corrupt persisted state silently mis-parsed instead of refused | `negative.rs::corrupt_session_state_is_refused_not_mis_parsed` |

Note in particular `negative.rs::sender_rollback_replays_send_state_but_receiver_rejects_and_recovers`.
This is a test that *encodes a known weakness rather than a guarantee* — see §9.

**`crypto` (201)** — `hkdf_test.rs` and `ml_kem_768_test.rs` contain published test vectors
(HKDF RFC 5869 A.1 in `crates/osl-ratchet-next/src/primitives.rs:334`, FIPS 203 for ML-KEM), so a
wrong key schedule or a wrong parameter set fails immediately. `aead_test.rs` inversion: accepting
a modified ciphertext or a modified AAD. `x25519_test.rs`: accepting an all-zero (non-contributory)
shared secret — the check is `crates/osl-ratchet-next/src/primitives.rs:15-20`, constant-time via
`subtle`. `ed25519_test.rs`: verifying a signature over different bytes.

**`keystore` (289)** — `zeroization_test.rs` is a real inversion battery: removing
`ZeroizeOnDrop` from `Identity` fails `recovery_entropy_is_zeroized_when_identity_drops` and
`cloned_identity_also_zeroizes_its_recovery_entropy`; changing `Sealer::unseal` to return a bare
`Vec<u8>` fails `unseal_returns_a_zeroizing_buffer` and `unseal_through_trait_object_is_also_zeroizing`.
`storage_test.rs` pairs `sealed_blob_is_opaque_on_disk_for_memory_sealer` against
`user_id_visible_on_disk_for_noop_sealer` — the second is itself the positive control proving the
first can fail. `sealer_test.rs` carries two deliberately-broken sealers, `SealFailure`
(`crates/keystore/tests/sealer_test.rs:8`) and `WrongRoundTrip` (`:28`), which exist purely to prove
the round-trip verifier can reject.

### 4.4 A negative control that is NOT sufficient — flagged for the reviewer

`apps/osl-hub/tests/ratchet_lane_signoff_b36.rs:146` guards the review gate like this:

```rust
assert!(
    wire_rn.contains("pub const RN_WIRE_IN_ENABLED: bool = false;"),
    "re-review must not enable RN"
);
```

That is a **source-text assertion** — it reads `crates/ipc/src/wire_rn.rs` as a string. The bundle
contract lists `source_text_only_test` as an invalid package condition. It is disclosed here rather
than counted as evidence. The load-bearing gate is instead `crates/ipc/tests/b3_ipc_integration_proof_status.rs:44`,
which imports the constant (`use ipc::wire_rn::RN_WIRE_IN_ENABLED;`) and asserts on the *value*, so
it cannot be satisfied by a comment or a renamed constant. A reviewer should treat the b36
assertion as documentation, not as a control.

Similarly, `crates/ipc/src/wire_rn.rs` gate #2, `the_sealed_file_contains_no_recognisable_state`
(`:3353`), is a **substring blacklist** of four encodings of the plaintext export plus the peer
identity key. It cannot detect a weak cipher, a constant key, or compression. It is a real test
but it is not a second independent gate on the at-rest guarantee — see §7.

---

## 5. reachability_classification

Three categories. The classification is derived from the searches recorded in §6 and in
`evidence/02-reachability-and-caller-search.md`.

### A. Reachable in the shipped product

| Area | Evidence of reachability |
|---|---|
| `crates/ipc/src/wire_v2.rs` `encrypt_v3` / `decrypt_v3` (wire `0x03`, PQ-hybrid wrap, **no ratchet**) | `apps/osl-hub/src/broker.rs:1446` calls `ipc::commands::cmd_osl_encrypt_message_v2`; `broker.rs:1815` calls `ipc::wire_v2::encrypt_v3` directly for the manual-peer path. **This is the cryptography that actually protects user messages today.** |
| `crates/crypto/src/**` primitives | Consumed by `wire_v2`, `keystore`, `store`. Transitively reachable from the send path above. |
| `crates/keystore/src/**` identity, sealers, password, prekeys, keyserver client | `apps/osl-hub/src/{startup_gate,password_lifecycle,core_bridge}.rs` call the IPC commands that use them; `crates/ipc/src/state.rs:406` and `state_reload.rs:369` call `select_best_sealer()`. |
| `crates/store/src/**` sealed message/attachment DB, burn | `crates/ipc/src/commands.rs:14843/14852/14883` open the production store anchored to the identity X25519 secret. |
| Keyserver `POST /v1/register`, `GET /v1/pubkeys/:user_id`, `GET /v1/prekey-bundle/:user_id` | Called by `crates/keystore/src/client.rs`; wired from `apps/osl-hub/src/core_bridge.rs:351`. |
| **RN capability advertisement (send side)** | `build_register_request` (`crates/keystore/src/client.rs:875-902`) sets `rn_capabilities = CLIENT_RN_CAPABILITY_FLOOR` and signs the extended REG_MSG. **This ships.** See the staleness warning below. |
| **The RN *selection* seam** | `crates/ipc/src/commands.rs:4457` and `:4481` sit on the production `cmd_osl_encrypt_message_v2_wire` path. They can, in production, return a **hard refusal** (`select_rn_wire_path`, `commands.rs:4315-4335`) rather than downgrade. See "reachable failure mode" below. |

### B. Built, tested, and DELIBERATELY NOT WIRED

**`crates/osl-ratchet-next` (OSL-RN, wire `0x10`) does not carry any traffic in the shipped
product, on purpose.** This is the single most important reachability fact in this package, and it
is verified two independent ways, not asserted.

There are **two** independent gates, both closed:

1. **Compile-time fuse.** `crates/ipc/src/wire_rn.rs:83` — `pub const RN_WIRE_IN_ENABLED: bool = false;`
   The non-test `wire_in_enabled()` (`wire_rn.rs:216-218`) returns the constant with no runtime path;
   a thread-local override exists only under `#[cfg(test)]` (`wire_rn.rs:226-229`).
2. **Runtime flag.** `AppState::rn_wire_in_enabled: AtomicBool` (`crates/ipc/src/state.rs:295`),
   initialised `false` (`state.rs:427`), in-memory only, never persisted. Its only mutator,
   `set_rn_wire_in_enabled` (`state.rs:600`), has **zero non-test callers** — every call site is
   inside a `#[cfg(test)]` module (verified in §6).

Consequences that follow from those two facts:

- `try_encrypt_rn_first_contact_with_bundle` returns `Ok(None)` immediately when the flag is
  false (`crates/ipc/src/commands.rs`, first statement of the function), so no OSL-RN handshake is
  ever initiated by the shipping build.
- The inbound dispatcher does recognise wire `0x10` (`commands.rs:7157`) but passes
  `RN_WIRE_IN_ENABLED` into `accept_rn_bootstrap_inbound_unknown`, so an inbound OSL-RN message
  cannot open a session either.
- Zero OSL-RN Tauri commands are registered: the `generate_handler!` block in
  `apps/osl-hub/src/main.rs:9264` (≈159 commands) contains no `rn`/`ratchet` entry, and
  `apps/osl-hub-ui/src` contains no `osl_rn`/`ratchet` invoke name.
- The workspace manifest itself documents the status (`Cargo.toml`, members list): *"Isolated
  research crate, UNWIRED at the product level … There is still no production call path, so the
  status remains implemented-unwired."*
- `crates/osl-ratchet-next/src/lib.rs:3` carries the banner **"UNREVIEWED. NOT WIRED IN. DO NOT
  CARRY REAL TRAFFIC."**

**A reachable failure mode that IS in the shipped build.** Because the selection seam is live but
the fuse is blown, a peer whose *signed* capability bitmap advertises OSL-RN causes the send to
**refuse**, not to downgrade:

```
"OSL: peer requires OSL-RN but wire-in is disabled in this build;
 refusing to send v=3 (no downgrade)"
```
(`crates/ipc/src/commands.rs:4322-4331`.) This is the intended fail-closed behaviour and is
consistent with `authorityAbsenceRule: "refuse"`. But note it composes with the fact that the
**client already advertises** `RN_CAP_WIRE_RN` at registration (§A above). A reviewer should
specifically assess whether two current-build peers can advertise OSL-RN to each other and
thereby refuse to send at all. **This package has not run that scenario and makes no claim about
whether it occurs in practice — it is a source-level concern, flagged, not a finding.**

### C. Not wired / not reachable at all

| Area | Status |
|---|---|
| `crates/ipc/src/wire_rn.rs` `send_rn` / `receive_rn` / `send_rn_for_state` / `receive_rn_for_state` | Zero non-test callers (§6). |
| `crates/ipc/src/wire_rn.rs` `RnSessionStore` sealed-session persistence | Constructed on the production path (`commands.rs:4469`, `:4590`, `state.rs:923`) but only ever *written to* behind the closed fuse. In a shipping build no `.session` file is created. **Not verified at runtime** — no shipping build was launched. |
| Keyserver migrations `0026_rn_capability_advertisement.sql`, `0027_control_inbox_revocation_lane.sql` | Both files self-declare **"NOT DEPLOYED"** in their headers. Whether they are applied to the production D1 instance **cannot be determined from this repository**. See §9. |
| The legacy Node/Fastify keyserver in `keyserver/` | Has no `rn_capabilities` support at all. Its deployment status is **not verified**. |

### Correction to prior internal notes — do not carry these forward

An internal note dated 2026-07-26 states that *"No Rust client advertises"*, that
`build_register_request` *"signs the legacy `reg_msg`"*, that `RegisterRequest` *"has no
`rn_capabilities` field"*, and that `verify_peer_capabilities` *"has zero production callers"*.
`crates/osl-ratchet-next/MIGRATION.md:136-152` repeats the same four claims and cites
`client.rs:601`, `:63-87`, `:301`.

**All four are false in this tree.** Verified directly:
`crates/keystore/src/client.rs:96` (the field exists), `:875-902` (`build_register_request`
advertises `CLIENT_RN_CAPABILITY_FLOOR` and signs the extended REG_MSG), `:934-969` (rotation
does too), and `crates/ipc/src/commands.rs:4662` (a production caller of
`verify_peer_capabilities`). The MIGRATION.md line citations no longer resolve to those symbols.

**`crates/osl-ratchet-next/MIGRATION.md` §1 is stale and must be corrected before it is handed to
an outside firm**, or the firm will review a downgrade story that does not match the code. Note
also that `crates/keystore/src/client.rs` is one of the *uncommitted* files (§1), so this
correction is itself pinned to the hash manifest, not to a commit.

---

## 6. production_registration_or_caller_search

Every search below was run by the author of this package in this tree. Verbatim commands and full
output are in `evidence/02-reachability-and-caller-search.md`; the material results are here.

| # | Search | Result |
|---|---|---|
| 1 | `grep -rn --include='*.rs' --include='*.toml' -E 'osl[-_]ratchet[-_]next' apps crates services src-tauri keyserver keyserver-cf` | **Zero hits under `apps/osl-hub/src`.** All hits are in `crates/osl-ratchet-next/**` (self), `crates/ipc/src/{wire_rn.rs,commands.rs,secure_local_store.rs,sender_attribution_proof.rs}`, `crates/ipc/Cargo.toml:73`, and comments in `crates/keystore/src/client.rs:409` / `crates/keystore/tests/client_test.rs:913`. |
| 2 | `grep -rn --include='*.rs' 'wire_rn' apps crates services src-tauri` | Under `apps/osl-hub/src`: exactly **one** hit, a doc comment at `broker.rs:87`. The only other `apps/` hits are in `apps/osl-hub/tests/ratchet_lane_signoff_b36.rs`, which reads the file as text. |
| 3 | Caller search for each RN entry point: `send_rn_for_state`, `receive_rn_for_state`, `accept_and_persist_with_sealer`, `initiate_and_persist_with_sealer`, `encrypt_rn_content_send` | Every call site is inside a `#[cfg(test)]` module or a `tests/` file, **except** `commands.rs:4497` (`encrypt_rn_content_send`) and `commands.rs:7346/7387`, which sit behind the closed fuse. |
| 4 | `grep -rn 'fn set_rn_wire_in_enabled\|set_rn_wire_in_enabled(' crates apps` | Definition at `state.rs:600`. Call sites: `state.rs:755`, `:757` (after `#[cfg(test)]` at `:712`); `wire_rn.rs:2755`, `:2791`, `:2834`, `:2842` (after `#[cfg(test)]` at `:1517`); `commands.rs:7794`, `:7821` (after `#[cfg(test)]` at `:5869`); `broker.rs:8406` (after `#[cfg(test)]` at `:8126`). **Zero production callers.** |
| 5 | Tauri command registration: `awk '/generate_handler!/,/\]\)/' apps/osl-hub/src/main.rs \| grep -i 'rn\b\|ratchet'` | **No matches.** The handler block (`main.rs:9264`) registers ≈159 commands, none OSL-RN. |
| 6 | `grep -rn 'osl_rn\|ratchet' apps/osl-hub-ui/src --include='*.ts' \| grep -v test` | **No matches.** No renderer-side invoke path exists. |
| 7 | `grep -rn 'verify_peer_capabilities' crates apps --include='*.rs'` | One production caller: `crates/ipc/src/commands.rs:4662`. Definition `crates/keystore/src/client.rs:432`. Remaining hits are tests/doc comments. |
| 8 | `grep -n 'encrypt_v3\|wire_v2::' apps/osl-hub/src/broker.rs` | `broker.rs:1815` calls `ipc::wire_v2::encrypt_v3`. Confirms the shipping path is v=3. |

**Positive control for the detector.** A reachability search that only ever returns "no hits" is
worthless. Search #2 is the control: the same grep that returns zero production hits for
`wire_rn` under `apps/osl-hub/src` returns 100+ hits for the same symbol under `crates/ipc/src`,
and search #8 returns the live `encrypt_v3` call site with identical syntax. The detector
demonstrably turns red on code that *is* wired.

**Method limitation, stated.** These are textual searches plus manual `#[cfg(test)]` boundary
resolution (comparing each call-site line number against the `#[cfg(test)]` line numbers in the
same file). They are **not** a compiler-verified dead-code proof. A reviewer wanting a stronger
guarantee should build with `RN_WIRE_IN_ENABLED` and confirm via `cargo build --release` +
dead-code analysis, or simply delete `crates/osl-ratchet-next` and confirm the shipping binary
still builds. **That stronger check was not performed here.**

---

## 7. at_rest_and_sealer_posture

Full detail in `evidence/03-at-rest-and-sealer.md`. Summary and the parts that matter for review:

### The `Sealer` boundary

`crates/keystore/src/sealer.rs:75`. Contract: implementations MUST authenticate, so tampered
ciphertext is rejected on `unseal`; `unseal` returns `Zeroizing<Vec<u8>>` by signature (`:88`).

| Implementation | Definition | Key source | AEAD |
|---|---|---|---|
| `TpmSealer` (Windows) | `sealer.rs:344` | Random 32-byte data key wrapped by a **TPM-resident RSA key** via Microsoft Platform Crypto Provider / NCrypt, **PKCS#1 v1.5** padding | XChaCha20-Poly1305 |
| `KeyringSealer` | `sealer.rs:227` | Random 32-byte key in the OS credential store (`keyring` crate). **No KDF — not password-derived.** | XChaCha20-Poly1305 |
| `EphemeralProcessSealer` | `sealer.rs:165` | Process-lifetime `OnceLock` random key, never persisted | XChaCha20-Poly1305 |
| `MemorySealer` | `sealer.rs:144` | Random per-instance key (test) | XChaCha20-Poly1305 |
| `NoOpSealer` | `sealer.rs:115` | **None — writes plaintext** | **None** |

`select_best_sealer()` (`sealer.rs:655`): Windows TPM → keyring → ephemeral, each gated on a live
round-trip probe (`verify_sealer_round_trip`, `:98`). **It has no path that returns `NoOpSealer`
or `MemorySealer`.** Shared helper `seal_with_aead_key` (`:682`) uses a fresh 24-byte random nonce
and fixed AAD `b"discord-privacy-client/keystore/seal/v1"` (`:676`); output is `nonce ‖ ct`.

**Three things a reviewer should press on:**

1. `NoOpSealer` and `MemorySealer` are **not `#[cfg(test)]`-gated** — both are publicly exported
   from the production library (`crates/keystore/src/lib.rs:99-103`). Only the *selection function*
   refuses them; any caller can construct `NoOpSealer` directly.
2. The non-Windows `TpmSealer` stub (`sealer.rs:607`) always errors on seal/unseal but still
   reports `is_tpm_backed() == true` (`:623-625`).
3. `requires_insecure_banner()` is a **self-report**. `crates/ipc/src/wire_rn.rs:660-662` refuses
   to persist ratchet state when it is true — but it trusts the sealer to tell the truth. The
   companion gate (§4.4) is only a substring blacklist. **Genuine two-gating would require a
   positive assertion that the bytes decrypt back under the real sealer's key and under no other
   key. That assertion does not exist.** This is a recorded gap, not a finding of exploitability.

### Where absence becomes REFUSAL (not permission)

Consistent with `authorityAbsenceRule: "refuse"`, these paths refuse rather than degrade:

- Plaintext sealer → `RnError::PlaintextSealerRefused` (`wire_rn.rs:660-662`, `:164-167`); the
  test at `wire_rn.rs:3262` also asserts **nothing was written**.
- On-disk schema newer than the binary → *"refusing to open"* (`crates/store/src/schema.rs:222-228`).
- Session/pin file over its cap → `RnError::StateTooLarge` / bounded read (`wire_rn.rs:675-680`).
- Peer record cap reached → `RnError::StoreFull`; **refuses the new record, never evicts a live one**
  (`wire_rn.rs:682-694`).
- Peer pinned to OSL-RN but capability unverified → `RnError::PinnedToRn`; `select_wire_version`
  is *structurally incapable* of returning `LegacyV3` for a pinned peer (`wire_rn.rs:472-495`),
  and `RnError` has no "fall back to v=3" variant at all (`:143-204`).
- Unsigned/unverifiable capability bitmap → `PeerCapabilities::{Absent,Unverified}`; there is
  **no** "unknown, assume capable" variant (`crates/keystore/src/client.rs:241-256`).

### `crates/store` — correcting the "schema v4" framing

**The current on-disk schema is v8, not v4** (`crates/store/src/schema.rs:68`,
`SCHEMA_VERSION: u32 = 8`). v4 is a named intermediate (`PRIVACY_SCHEMA_VERSION = 4`, `:69`).
Any review text describing "the v4 store" as current is wrong as of this tree.

What v4 did (`schema.rs:49-53`, verbatim): *"Removes every plaintext identifier from disk. Ids and
timestamps move inside a sealed per-row blob; lookups run against keyed blind indexes; ordering runs
against an opaque seq."* Later: v5/v6 per-record content keys, v7 sealed attachment manifest,
v8 removes the plaintext `burned_at`.

Still visible in the clear on disk (`crates/store/SECURITY.md:130-147`): the opaque `seq`, the
`burned` bit (deliberately queryable so re-put fails closed), ciphertext lengths, row counts, and
the **deterministic keyed blind indexes** `mid_bi`/`chan_bi`/`sender_bi`/`ck_bi` — which leak
equality and frequency. This is documented by the project, not hidden, and is a good target for
review.

Store master key: `HKDF-SHA256(salt="", ikm=identity_secret, info="osl-message-store-v1")`
(`crates/store/src/cipher.rs:35`, `:71`), where `identity_secret` is the identity **X25519 secret
key bytes** (`crates/ipc/src/commands.rs:4903-4908`).

Burn is terminal, enforced in SQL by `WHERE messages.burned = 0` on the upsert
(`crates/store/src/lib.rs:923`) and `WHERE attachments.burned = 0` (`:1469`), with
`secure_delete=ON` (`:631`) and `PRAGMA wal_checkpoint(TRUNCATE)` (`:405`).
**Burn is local destruction only — it is not revocation** (`SECURITY.md:291-319`).

### Identity key derivation — a correction worth making explicitly

The identity is **not** derived from the user's password. It is derived deterministically from
16 bytes of BIP39 entropy via `HKDF-SHA256(salt=b"OSL-identity-v1", ikm=entropy, info=<label>)`
seeding a ChaCha20 RNG (`crates/keystore/src/identity.rs:228-233`, `:246`). The password protects
the *recovery phrase*: Argon2id (m=65536 KiB, t=3, p=1, 64-byte output) split into a 32-byte
verification hash and a 32-byte **AES-256-GCM** key (`crates/ipc/src/main_password.rs:66-75`,
`:241-252`). The `crates/keystore/src/password.rs` module is explicitly **legacy** and its
Argon2 usage is password *hashing*, not key derivation (`password.rs:19-23`).

Zeroization is systematic: `Identity` is `ZeroizeOnDrop` with a compile-time assertion
(`crates/keystore/src/storage.rs:345-347`), `unseal` returns `Zeroizing`, RN state exports are
zeroized after sealing (`wire_rn.rs:665`) and after import (`:776`), and
`crates/keystore/src/sensitive_memory.rs` adds `mlock`/`VirtualLock` page locking.

**Not verified at runtime:** the TPM and keyring sealers. Both of their dedicated tests are
`#[ignore]`d (§3), and this package ran on Linux. Their behaviour here is source-review only.

---

## 8. wire_api_contract_evidence

Full detail in `evidence/04-wire-and-api-contracts.md`.

### 8.1 What actually ships: `wire_v2` v=3

`crates/ipc/src/wire_v2.rs:685-770`, `encrypt_v3`. **PQ-hybrid multi-recipient key wrap. It is not
a ratchet: no forward secrecy, no post-compromise security.** (`wire_rn.rs:394` says so in-tree.)

- Fresh random 32-byte body key `K`; body sealed once with **AES-256-GCM** (12-byte nonce,
  16-byte tag), AD `b"OSL/A1/body/v3"`.
- Per recipient: `pqxdh::initiate` (X25519 + ML-KEM-768 → HKDF combiner) → `wrap_key =
  HKDF([], session_key, b"OSL/A1/wrap-key/v3")`; `K` sealed under `wrap_key`, AD `b"OSL/A1/wrap/v3"`.
- Slot: `pubkey_hash_prefix(8) ‖ ek_x25519_pub(32) ‖ ct_len(u16 LE) ‖ mlkem_ct ‖ wrap_nonce ‖ wrap_ct`.
  Header: `version(0x03) ‖ msg_type ‖ sender_ik_x25519_pub(32) ‖ N`. Max 255 recipients.
- Output: `"DPC0::" + base64`.
- Version namespace: `0x02`/`0x03`/`0x04` (legacy ratchet)/`0x05` (sender keys), message types
  `0x00`–`0x0B` (`wire_v2.rs:85-214`).

**This is the highest-value review target in the package**, because it is the only one carrying
real user data.

### 8.2 OSL-RN wire `0x10` (unwired — see §5)

`crates/osl-ratchet-next/src/session.rs`.

- `WIRE_VERSION_RN = 0x10` (`:148`); `RN_FLAG_BOOTSTRAP = 0x01` (`:151`); reserved-bit mask
  `0xFE` (`:154`); prefix `"DPC0::"` (`:157`); `MAX_WIRE_BYTES = 64 KiB` (`:160`);
  `MAX_HEADER_CT_BYTES = 1024` (`:162`).
- Handshake: PQXDH-shaped. `dh1=DH(IK_a,SPK_b)`, `dh2=DH(EK_a,IK_b)`, `dh3=DH(EK_a,SPK_b)`,
  optional `dh4=DH(EK_a,OPK_b)`, plus `ss = ML-KEM-768.Encaps(PQ_prekey_b)`;
  `SK = HKDF(salt=[], ikm = dh1‖dh2‖dh3‖dh4?‖ss, info = handshake_info(binding))`
  (`src/handshake.rs:207-226`).
- Primitives: X25519 (`x25519-dalek` 2.0, with a constant-time all-zero rejection at
  `src/primitives.rs:15-20`), ML-KEM-768 (`ml-kem` 0.2; `EK=1184`, `DK=2400`, `CT=1088`),
  XChaCha20-Poly1305, HKDF-SHA256. `#![forbid(unsafe_code)]` and
  `deny(clippy::{indexing_slicing,unwrap_used,panic})` (`src/lib.rs:48-51`).
- **Header encryption**: per-chain header keys `hks`/`hkr` from the root step
  (`src/kdf.rs:87-121`, HKDF → 96 bytes split `root ‖ chain ‖ header`); 12 random nonce bytes on
  the wire expanded with the constant prefix `b"OSL-RN/hdr\x00\x01"`; AD chains
  `b"OSL-RN/v1/header" ‖ version ‖ flags ‖ preamble?`, body AD `b"OSL-RN/v1/body" ‖ <everything so far>`.
  Trial header decryption is bounded at `2 + max_chains` = 7 AEAD opens.
- **Body nonce is deterministic**: `HKDF(salt=MK, ikm="nonce", info=b"OSL-RN/v1/body-nonce")[0..24]`
  (`src/kdf.rs:130-133`). This makes any message-key reuse a nonce reuse — see §9.
- Bounds (all with `validated()` constructors): `SkipParams` default
  `max_skip_per_message=512, max_keys_per_chain=512, max_total_keys=2048, max_chains=5, max_age=100_000`
  (`src/skipped.rs:66-99`); `PqParams` default `fragment_bytes=128, rekey_interval=64`
  (`src/pq.rs:115-143`); PQ `MAX_FRAGMENTS=64`, `MAX_FRAGMENT_BYTES=512`, `MAX_EPOCH_LOOKAHEAD=2`,
  `MAX_RETAINED_EPOCHS=8`. Whole-session memory bound ≈300 KiB (DESIGN.md §8).

### 8.3 Session state export / import contract

- `Session::export_state` (`src/session.rs:781`) / `import_state` (`:825`).
  **Hand-rolled binary codec (`src/codec.rs`, varints) — there is no serde on session state.**
  A reviewer should note this cuts both ways: no serde attack surface, but also no derived
  round-trip guarantees.
- `STATE_FORMAT_VERSION: u8 = 1` (`:888`); any other version → `Error::BadStateFormat`
  (`:827-829`); trailing bytes rejected by `r.finish()` (`:854`).
- Field order (`:782-822`): version, role, `session_id[16]`, `root[32]`, `dhs[32]`, opt `dhr[32]`,
  opt `cks`, opt `hks`, `nhks[32]`, opt `ckr`, opt `hkr`, `nhkr[32]`, varints `ns,nr,pn,send_mix_epoch`,
  `pq.export`, `skipped.export`, opt bootstrap preamble.
- The crate documents plainly (`src/session.rs:779-780`): **"The output contains every secret the
  session holds."** Sealing is the caller's job — done in `crates/ipc/src/wire_rn.rs:645-672`.
- On-disk envelope (`wire_rn.rs:587-592`): `SealedBlob { version: u32, method: String, sealed_b64: String }`,
  `SESSION_BLOB_VERSION = 1`. Method label compared **constant-time** on load (`:759-769`).
  Caps: session file 1 MiB, pin file 4 KiB, 512 peer records, 4096 skipped keys.
  File naming: first 16 bytes of `SHA-256(b"OSL-RN/v1/session-file/" ‖ peer_x25519)`, hex.
- **The pin file is plain JSON — it is NOT sealed** (`wire_rn.rs:834-838`), deliberately stored in
  a separate file from session state so that deleting the session cannot silently drop the
  downgrade pin (`:41-55`, `:791-802`).

### 8.4 Negotiation and downgrade protection

- `Negotiation::digest()` (`crates/osl-ratchet-next/src/negotiate.rs:160-192`) = SHA-256 over
  length-prefixed (`u32_be(len)‖bytes`) domain `b"OSL-RN/v1/negotiation/v1"`, selected version,
  floor, responder identity, responder ML-KEM ek, initiator identity, context. Refuses
  (`Error::PolicyBound`) when selected ≠ `0x10`, floor < `0x10`, floor > selected, wrong ek length,
  or empty/oversize context.
- `RnPeerPin { min_wire_version: u8 }` (`wire_rn.rs:422-462`). `raise_to_rn` is the **only**
  mutator and raises only. `validated()` accepts only `0x03` or `0x10`.
- `select_wire_version` (`wire_rn.rs:472-495`) checks the pin **first**, then capability.
- **Known defect, disclosed, not fixed:** `initiate_and_persist_with_sealer` raises the peer's pin
  *before* the peer has completed anything — `store.raise_pin_to_rn(&peer_id)?` at
  **`crates/ipc/src/wire_rn.rs:970`**, immediately after the local `save_session_with_sealer`.
  A local initiation the peer never completes therefore pins them permanently, producing
  `PinnedToRn` on every later send with no recovery short of a new identity key. No attacker is
  required. It was identified internally on 2026-07-26 and deliberately **not** fixed, because
  changing it changes downgrade-protection behaviour.
  **Stated accurately, in fairness to the code:** the raise is guarded by `if caps.supports_rn()`,
  so it only fires for a peer whose *signature-verified* bitmap advertises OSL-RN
  (`PeerCapabilities::Verified`), not for an arbitrary peer. That narrows the trigger but does not
  remove it — the peer still never confirmed *this* session. The same unguarded-by-confirmation
  raise also appears at `wire_rn.rs:1026` and `:1102`.
  **This is a live item for the reviewing firm.**

### 8.5 Keyserver API

Cloudflare Worker (`keyserver-cf/src/index.ts`): 4 GET route groups + ~11 GET paths, ~33 POST
paths, 4 DELETE paths, CORS allowlist at `:308-327`, anything else `405` (`:476`). Crypto-relevant:
`POST /v1/register`, `GET /v1/pubkeys/:user_id`, `GET /v1/prekey-bundle/:user_id`,
`POST /v1/prekey-bundle/replenish`, `POST /v1/wrapped-keys` + `GET/DELETE`,
`POST/GET/DELETE /v1/control-inbox`, `POST /v1/proof-challenge`,
`POST /v1/account-ownership/challenge`, `POST /v1/usernames/claim`.

RN capability advertisement (the only defence against first-contact downgrade):

- Client signs an extended REG_MSG = legacy `reg_msg` + `\n` + decimal bitmap
  (`crates/keystore/src/client.rs:212-231`); `RN_CAP_WIRE_RN = 1`, `RN_CAP_MAX = 0xffff`.
- Server parses and enforces monotonicity — a rotation is refused if
  `caps.value < priorRecord.rn_capabilities` (*"capability advertisements never lower"*,
  `keyserver-cf/src/endpoints/register.ts:384-388`); combining a key change with a capability
  change requires an authenticated rotation (`:292`).
- `GET /v1/pubkeys/:user_id` returns `rn_capabilities` **and** `registration_sig` so the reader can
  verify (`keyserver-cf/src/endpoints/pubkeys.ts:51-52`), defaulting to 0 — *"no encoding of
  'unknown, assume capable'"* (`:16-19`).
- Client verification `verify_peer_capabilities` (`client.rs:432-471`) degrades to `Unverified`
  on every failure and only returns `Verified(bits)` after `ed25519::verify` succeeds.

**Migrations.** `keyserver-cf/migrations/` holds 39 `.sql` files.
`0026_rn_capability_advertisement.sql` exists and is a single DDL
(`ALTER TABLE users ADD COLUMN rn_capabilities INTEGER NOT NULL DEFAULT 0;`); integrity comes from
the Ed25519 REG_MSG/ROT_MSG signatures, not the table.
`0027_control_inbox_revocation_lane.sql` exists and adds a non-evictable, collapsible priority lane
for burn-revocation notices (507 rather than silent drop when full).
**Both files self-declare "NOT DEPLOYED".** Whether either is applied to the production D1 instance
**is not determinable from this repository and is not claimed here.** See §9.

**Structural hazard found, not assessed:** the migrations directory contains duplicate numeric
prefixes — `0025_payment_alert_outbox.sql` / `0025_username_directory.sql`,
`0026_osl_mail.sql` / `0026_rn_capability_advertisement.sql`, and
`0036_account_ownership_challenges.sql` / `0036_account_ownership_proof_required.sql`.
Whether `wrangler d1 migrations apply` orders these deterministically **was not verified**.

---

## 9. explicit_exclusions

Per the bundle contract: *"no runtime/two-identity/deployment/user-visible claim is earned unless
that exact evidence is attached."* None of that evidence is attached. This section is deliberately
generous.

### 9.1 Claims this package does NOT make

- **No runtime claim about the shipping application.** The OSL hub was never launched. No message
  was encrypted, sent, received, or decrypted through the product. No screenshot, no log, no
  two-identity pairing. The only processes executed were three `cargo nextest` runs.
- **No two-identity / interop claim.** No Alice↔Bob exchange outside the in-process test harness.
- **No deployment claim.** Whether keyserver migrations `0026`/`0027` are applied to production
  D1, which Worker version is live, and whether the legacy `keyserver/` is still serving are all
  **not verified**. Both migration files say "NOT DEPLOYED"; that is a claim in a file, not an
  observation of production.
- **No user-visible / UX claim.** Nothing about banners, warnings, or what a user sees when a send
  refuses.
- **No claim that OSL-RN is safe to enable.** The opposite: it is unreviewed by design, and this
  package exists to get it reviewed.
- **No performance or side-channel claim.** No constant-time verification beyond noting where
  `subtle` is used. No timing, cache, power, or fault analysis. No fuzzing beyond the in-suite
  `random_garbage_never_panics_and_never_opens`.
- **No supply-chain claim.** No `cargo audit`, no dependency review, no lockfile analysis, no
  reproducible-build check. `Cargo.lock` is itself uncommitted (§1).
- **No claim about `stego`, `transport`, `selectors`, `message-lifecycle`, `runtime`,
  `adapter-profile`, `cover-draft`, `apps/osl-hub-ui`, `src-tauri`, `services/`, `cipher-store-cf`,
  `scripts/`, or `infra/`.** Notably, the steganographic carrier (`crates/stego`) is excluded, so
  nothing here speaks to whether OSL traffic is distinguishable on the wire.

### 9.2 Coverage gaps inside the reviewed roots

- **`-p ipc`, `-p store`, and `apps/osl-hub` test binaries were not run.** That is where
  `wire_rn.rs`, `wire_v2.rs`, `commands.rs`, and the store schema live. Their test counts are
  unverified and none is quoted. This is the largest gap in §3.
- **The TPM and OS-keyring sealers were not exercised.** Their tests are `#[ignore]`d and this run
  was on Linux. `select_best_sealer()` prefers exactly those two in production.
- **No mutation experiment** (§4.2). Falsifiability is argued from test source, not demonstrated
  by making the suites red.
- **Reachability was established textually**, not by compiler dead-code proof (§6).
- **`apps/osl-hub/src` was reviewed for call sites only.** 144,510 lines were not read. It was
  searched for RN/`wire_rn`/`encrypt_v3` reachability and Tauri registration, nothing more.
- **`crates/ipc/src/commands.rs` is ~18,000 lines** and was read only around the RN and store
  seams.
- **No serde / call-site audit was performed for the uncommitted edits** to
  `crates/keystore/src/client.rs` and `crates/ipc/src/state_reload.rs`. The bundle contract lists
  *"missing serde or call-site audit after a public type change"* as an invalid condition; this
  package cannot certify those two files were not mid-change during review. **This is a reason to
  regenerate the package from a frozen tree before `b14`.**

### 9.3 Known weaknesses carried forward, so the firm does not have to rediscover them

These are disclosed by the project itself and confirmed to still be present in this tree:

1. **The shipping path has no forward secrecy.** `encrypt_v3` is a PQ-hybrid wrap with no ratchet.
   Compromise of an identity key retroactively opens captured traffic. OSL-RN exists to fix this
   and is not wired.
2. **OSL-RN body nonces are deterministic** (`HKDF(MK)`, `src/kdf.rs:130-133`), so any message-key
   reuse is a nonce reuse. `send_rn` persists advanced state *before* returning the wire to
   mitigate the crash case; the **restore-from-backup / VM-snapshot case is not closed**. The
   in-suite test `negative.rs::sender_rollback_replays_send_state_but_receiver_rejects_and_recovers`
   encodes the residual, not a guarantee.
3. **`initiate_and_persist` pins a peer before the peer confirms** (§8.4) — a self-inflicted
   permanent `PinnedToRn`.
4. **The at-rest guarantee for RN session state has one load-bearing gate, not two** (§7, §4.4).
5. **`crates/store` blind indexes are deterministic**, leaking equality and frequency over
   identifiers (§7).
6. **Recovery messages deliberately stay off the ratchet.** `SKDM_REQUEST` / `SESSION_RESET` /
   revocations remain on `encrypt_v3` by design — recovery cannot depend on the thing it repairs —
   so that channel has no forward secrecy and a compromised identity key retroactively forges those
   messages.
7. **`crates/osl-ratchet-next/MIGRATION.md` §1 is stale** and contradicts the code on four points
   (§5). Fix it before handing the package over.
8. **The tree is dirty** (§1). Freeze it before `b14`.

### 9.4 Where this package REFUSES rather than estimates

Consistent with `authorityAbsenceRule: "refuse"`, the following are recorded as *unknown*, and no
inference is offered in either direction:

- Production deployment state of keyserver migrations `0026` / `0027`, and of either keyserver.
- Whether duplicate migration prefixes are ordered deterministically by `wrangler`.
- Actual TPM modulus size (`sealer.rs:367` comments "2048-bit RSA" but
  `NCryptCreatePersistedKey` is called without an explicit length, so the real value is the PCP
  default).
- Whether `select_best_sealer()` is the *only* sealer factory reached in shipping code, or whether
  some path constructs `KeyringSealer::new()` / `NoOpSealer::new()` directly.
- Whether two current-build peers, both advertising `RN_CAP_WIRE_RN`, can drive each other into
  the "refusing to send v=3 (no downgrade)" state in practice (§5).
- Whether an old v3 binary is *proven* refused end-to-end by a running test (the pinned fixture
  `crates/store/tests/fixtures/adff4e45_schema_reader.rs` exists; the test exercising it was not run).

---

*Assembled for plan unit `b13` on 2026-07-31 (UTC) against the working tree described in §1.
`b14` — engaging a reviewing firm — has not been performed and is not authorised by this document.*
