# Evidence 02 — reachability classification and caller search

Supports README §5 (`reachability_classification`) and §6
(`production_registration_or_caller_search`).

All searches were run by the author of this package in the repository working tree during the
assembly window. Line numbers refer to the working tree pinned by
`reviewed-file-hashes.txt`, **not** to commit `1eaa7b5`.

---

## Method, and its limits

Reachability was established by:

1. Text search (`grep -rn`) for every OSL-RN symbol across `apps/`, `crates/`, `services/`,
   `src-tauri/`, `keyserver/`, `keyserver-cf/`.
2. For each hit, resolving whether it is production or test by comparing its line number against
   the `#[cfg(test)]` module boundaries in the same file (`grep -n '#\[cfg(test)\]' <file>`).
3. Checking the Tauri command registration surface and the renderer-side invoke names.

**This is not a compiler-verified dead-code proof.** Two stronger checks a reviewer can run and
this package did not:

- Delete `crates/osl-ratchet-next` and confirm the shipping binary still builds.
- Build with `RN_WIRE_IN_ENABLED = true` and diff the resulting call graph.

---

## Search 1 — the crate name

```
grep -rn --include='*.rs' --include='*.toml' -E 'osl[-_]ratchet[-_]next' \
     apps crates services src-tauri keyserver keyserver-cf
```

Result grouped by location:

| Location | Hits | Kind |
|---|---|---|
| `crates/osl-ratchet-next/**` | many | the crate itself + its own `tests/` |
| `crates/ipc/Cargo.toml:73` | 1 | the dependency declaration (comment above it at `:72`: *"Consumed only by `wire_rn`; the v=2..v=5 paths do not reference it."*) |
| `crates/ipc/src/wire_rn.rs` | many | the adapter module |
| `crates/ipc/src/commands.rs` | many | RN seam + `#[cfg(test)]` modules |
| `crates/ipc/src/secure_local_store.rs:324-326` | 3 | inside `#[cfg(test)]` |
| `crates/ipc/src/sender_attribution_proof.rs:218` | 1 | a version-table entry (`("wire rn", osl_ratchet_next::WIRE_VERSION_RN)`) |
| `crates/keystore/src/client.rs:409`, `crates/keystore/tests/client_test.rs:913` | 2 | doc comments only |
| **`apps/osl-hub/src/**`** | **0** | — |
| `services/`, `src-tauri/`, `keyserver/`, `keyserver-cf/` | 0 | — |

**Zero hits under `apps/osl-hub/src`.**

## Search 2 — the adapter module (and the positive control)

```
grep -rn --include='*.rs' 'wire_rn' apps crates services src-tauri
```

Under `apps/osl-hub/`:

```
apps/osl-hub/src/broker.rs:87:/// `ipc::wire_rn::{send_rn,receive_rn}`.
apps/osl-hub/tests/ratchet_lane_signoff_b36.rs:99
apps/osl-hub/tests/ratchet_lane_signoff_b36.rs:100
apps/osl-hub/tests/ratchet_lane_signoff_b36.rs:146
```

`broker.rs:87` is a doc comment. The three `ratchet_lane_signoff_b36.rs` hits read
`crates/ipc/src/wire_rn.rs` **as a string** (`fs::read_to_string`) — see the source-text-test
caveat in README §4.4.

Under `crates/ipc/src/`: 100+ hits (`wire_rn.rs`, `commands.rs`, `state.rs`, `lib.rs:86`,
`secure_local_store.rs`, `sender_attribution_proof.rs`).

**This search is the positive control for the detector.** The same grep that returns one
doc-comment hit under `apps/osl-hub/src` returns 100+ hits one directory over. The detector is
not silently broken.

Second positive control, Search 8 below: the identical grep style finds the *live* v=3 call site.

## Search 3 — callers of every RN entry point

```
for f in select_rn_wire_path_for_send encrypt_rn_content_send \
         rn_session_store_from_config_dir rn_session_store \
         send_rn_for_state receive_rn_for_state \
         accept_and_persist_with_sealer initiate_and_persist_with_sealer; do
  grep -rn --include='*.rs' "\b$f\b" apps crates | grep -v "fn $f"
done
```

`#[cfg(test)]` module start lines used to classify hits:

- `crates/ipc/src/commands.rs`: 38, 43, 48, 53, 281, 989, 2010, 2505, 3055, **4105**, **4858**,
  **5370**, **5869**, **8481**, 9013, … (36 total)
- `crates/ipc/src/wire_rn.rs`: 220, 226, **1517**
- `crates/ipc/src/state.rs`: **712**, 946
- `apps/osl-hub/src/broker.rs`: **8126**

Classification:

| Symbol | Production call sites | Test-only call sites |
|---|---|---|
| `send_rn_for_state` | **none** | `wire_rn.rs:2773,2804,2830,2835,2844`; `commands.rs:7531,7761,7810,7836,7839,7841,7888,7897,7906` (all > 5869) |
| `receive_rn_for_state` | **none** | `wire_rn.rs:2777`; `commands.rs:7770,7853,7865,7873,7916,7926,7934` |
| `initiate_and_persist_with_sealer` | `wire_rn.rs:917` (from `initiate_and_persist`), `wire_rn.rs:1187`; `commands.rs:4836` (behind the fuse) | the rest (all > 1517 in `wire_rn.rs`, > 5869 in `commands.rs`) |
| `accept_and_persist_with_sealer` | `wire_rn.rs:991`; `commands.rs:7387` (behind the fuse) | the rest |
| `select_rn_wire_path_for_send` | **`commands.rs:4481`** — live on the production send path | `commands.rs:3868,3886,3937,3946,4004,4017,4123,4135` |
| `encrypt_rn_content_send` | **`commands.rs:4497`** — live, but only reachable if the fuse is on | `commands.rs:4037,4089` |
| `rn_session_store_from_config_dir` | **`commands.rs:4469`** — live | — |
| `rn_session_store` | `commands.rs:7345,7386`; `state.rs:233,405,923` | `state.rs:1086,1092,1097` |

## Search 4 — the runtime flag's mutator

```
grep -rn --include='*.rs' 'fn rn_wire_in_enabled\|fn set_rn_wire_in_enabled\|set_rn_wire_in_enabled(' crates apps
```

```
crates/ipc/src/state.rs:596:    pub fn rn_wire_in_enabled(&self) -> bool {
crates/ipc/src/state.rs:600:    pub fn set_rn_wire_in_enabled(&self, enabled: bool) {
crates/ipc/src/state.rs:755:        state.set_rn_wire_in_enabled(true);
crates/ipc/src/state.rs:757:        state.set_rn_wire_in_enabled(false);
crates/ipc/src/wire_rn.rs:2755:        state.set_rn_wire_in_enabled(true);
crates/ipc/src/wire_rn.rs:2791:        state.set_rn_wire_in_enabled(true);
crates/ipc/src/wire_rn.rs:2834:        state.set_rn_wire_in_enabled(true);
crates/ipc/src/wire_rn.rs:2842:        state.set_rn_wire_in_enabled(false);
crates/ipc/src/commands.rs:7794:        gate.set_rn_wire_in_enabled(true);
crates/ipc/src/commands.rs:7821:        bob_state.set_rn_wire_in_enabled(true);
apps/osl-hub/src/broker.rs:8406:        core.osl.set_rn_wire_in_enabled(true);
```

Against the `#[cfg(test)]` boundaries in Search 3: `state.rs:755/757` > 712; `wire_rn.rs:2755…2842`
> 1517; `commands.rs:7794/7821` > 5869; `broker.rs:8406` > 8126.

**Every single mutator call site is test-only. Zero production callers.**

Declaration and default (`crates/ipc/src/state.rs`):

```rust
// :291-295
/// Runtime gate for OSL-RN wire-in.
///
/// Defaults false, is in-memory only, and is separate from
/// `wire_rn::RN_WIRE_IN_ENABLED`, which remains the compile-time review
/// fuse for builds that still must not wire OSL-RN into production flows.
pub rn_wire_in_enabled: AtomicBool,

// :425-427
// RN starts unwired. wire_rn::RN_WIRE_IN_ENABLED is the compile-time
// fuse; this runtime flag must never default to a more permissive value.
rn_wire_in_enabled: AtomicBool::new(false),
```

## Search 5 — Tauri command registration

```
awk '/generate_handler!/,/\]\)/' apps/osl-hub/src/main.rs | grep -i 'rn\b\|ratchet'
```
→ **no matches.**

```
awk '/generate_handler!/,/\]\)/' apps/osl-hub/src/main.rs | grep -c ','
```
→ `159` (approximate count of registered commands).

Registration sites: `apps/osl-hub/src/main.rs:9264`
(`builder.invoke_handler(tauri::generate_handler![…])`) and `:9633`
(`builder.invoke_handler(hub_tauri_commands!(hub_tauri_generate_handler))`, macro defined at
`main.rs:9065`).

## Search 6 — renderer-side invoke names

```
grep -rn 'osl_rn\|ratchet' apps/osl-hub-ui/src --include='*.ts' | grep -v test
```
→ **no matches.** No UI path can reach OSL-RN.

## Search 7 — capability verification callers

```
grep -rn 'verify_peer_capabilities' crates apps --include='*.rs'
```

```
crates/keystore/src/client.rs:432:pub fn verify_peer_capabilities(resp: &PubkeysResponse) -> PeerCapabilities {
crates/keystore/src/client.rs:541   (doc comment)
crates/keystore/src/client.rs:551   (doc comment)
crates/ipc/src/commands.rs:3901     (test fn name)
crates/ipc/src/commands.rs:3934     (test)
crates/ipc/src/commands.rs:3964     (test fn name)
crates/ipc/src/commands.rs:3998     (test)
crates/ipc/src/commands.rs:4662     <-- PRODUCTION
crates/ipc/src/commands.rs:5016     (test fn name)
crates/ipc/tests/register_fix_peer_keys.rs:77 (comment)
```

`commands.rs:4662` is the tail of `verified_rn_capabilities_for_live_peer`, a production helper
consumed by the send path.

## Search 8 — the shipping crypto path (second positive control)

```
grep -n 'encrypt_v3\|wire_v2::' apps/osl-hub/src/broker.rs
```

```
apps/osl-hub/src/broker.rs:1815:    ipc::wire_v2::encrypt_v3(
```
plus ~20 other `ipc::wire_v2::` message-type and bundle-predicate references.

And the command entry point:

```
apps/osl-hub/src/broker.rs:1446:    let encrypted = ipc::commands::cmd_osl_encrypt_message_v2(
```

**This is the live cryptography.** The detector finds it with the same technique that finds
nothing for OSL-RN.

---

## The production send call graph, as established

```
apps/osl-hub/src/broker.rs:1446
  └── ipc::commands::cmd_osl_encrypt_message_v2            (commands.rs:4189)
        └── cmd_osl_encrypt_message_v2_wire                (call at commands.rs:4211,
                                                            defined at commands.rs:4340)
              ├── try_encrypt_rn_first_contact_from_state(state, RN_WIRE_IN_ENABLED, …)
              │     │                                       call at commands.rs:4457
              │     └── try_encrypt_rn_first_contact_with_bundle(rn_wire_in_enabled, …)
              │           └── if !rn_wire_in_enabled { return Ok(None); }   <-- GATE 1, always taken
              ├── rn_session_store_from_config_dir()        (commands.rs:4469)
              ├── select_rn_wire_path_for_send(…)           (commands.rs:4481)
              │     └── select_rn_wire_path                 (commands.rs:4315)
              │           └── wire_rn::select_wire_version  (wire_rn.rs:472)
              │                 ├── SelectedVersion::LegacyV3 -> RnWirePath::LegacyV3   (normal)
              │                 └── SelectedVersion::Rn ->
              │                       if RN_WIRE_IN_ENABLED { RnWirePath::Rn }          <-- GATE 2
              │                       else { Err("… refusing to send v=3 (no downgrade)") }
              ├── (RnWirePath::Rn) encrypt_rn_content_send  (commands.rs:4497)  UNREACHABLE
              └── falls through to wire_v2 v=3 / v=5        <-- what actually ships
```

Receive side (`commands.rs:7157`):

```rust
Some(osl_ratchet_next::WIRE_VERSION_RN) => {
    tracing::debug!(wire_version = "rn", "OSL-RN bootstrap decode dispatched");
    accept_rn_bootstrap_inbound_unknown(
        state, &content, config_dir.as_deref(),
        crate::wire_rn::RN_WIRE_IN_ENABLED,   // <-- GATE, false
    )?
}
```

The two gates, verbatim:

```rust
// crates/ipc/src/wire_rn.rs:78-83
/// Single switch that turns OSL-RN send/receive wire-in on.
///
/// This must stay off until flipping it has passed the external
/// cryptographic review required by `crates/osl-ratchet-next/DESIGN.md`
/// and master 7.11.
pub const RN_WIRE_IN_ENABLED: bool = false;

// crates/ipc/src/wire_rn.rs:216-218 (non-test)
fn wire_in_enabled() -> bool {
    RN_WIRE_IN_ENABLED
}
```

```rust
// crates/ipc/src/commands.rs:4322-4331
Ok(crate::wire_rn::SelectedVersion::Rn) => {
    if crate::wire_rn::RN_WIRE_IN_ENABLED {
        Ok(RnWirePath::Rn)
    } else {
        Err(
            "OSL: peer requires OSL-RN but wire-in is disabled in this build; \
             refusing to send v=3 (no downgrade)"
                .to_string(),
        )
    }
}
```

## Corroborating in-tree statements

Root `Cargo.toml`, workspace members list:

> *Isolated research crate, UNWIRED at the product level. `crates/ipc` declares the dependency
> (crates/ipc/Cargo.toml:73) and the adapter `crates/ipc/src/wire_rn.rs` genuinely uses it - but
> nothing uses `wire_rn`: its only non-test reference is the module declaration
> `pub mod wire_rn;` at crates/ipc/src/lib.rs:76. There is still no production call path, so the
> status remains implemented-unwired. See `crates/osl-ratchet-next/DESIGN.md` - unreviewed, must
> not carry real traffic before external cryptographic review.*

`crates/osl-ratchet-next/src/lib.rs:3`: **"UNREVIEWED. NOT WIRED IN. DO NOT CARRY REAL TRAFFIC."**

**Note one discrepancy for accuracy:** the workspace comment says `wire_rn`'s *only* non-test
reference is the `pub mod` declaration. That is **no longer exactly true** in this tree — Search 3
found live production references at `commands.rs:4469`, `:4481`, `:4497`, `:4590`, `:4836`,
`:7345`, `:7386`, and `crates/ipc/src/state.rs:233/405/923`. The *conclusion* (no production path
reaches OSL-RN encryption) still holds, because both gates are closed — but the stated *reason*
in that comment is out of date. A reviewer should rely on the gates, not on the comment.

## A reachable failure mode, flagged

Because the selection seam is live while the fuse is blown, and because
`build_register_request` (`crates/keystore/src/client.rs:875-902`) **does** advertise
`RN_CAP_WIRE_RN` in this tree, a peer whose signed bitmap says "I speak OSL-RN" causes the local
send to return the *"refusing to send v=3 (no downgrade)"* error rather than sending anything.

This is the intended fail-closed design. **Whether it actually fires between two current builds
was not tested and is not claimed here** — it requires a two-identity runtime scenario, which this
package explicitly excludes (README §9). It is recorded as a question for the reviewing firm.
