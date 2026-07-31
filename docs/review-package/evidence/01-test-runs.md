# Evidence 01 — test runs

Supports README §3 (`owning_unit_behavior_tests`) and §4 (`negative_control_statement`).

Everything here was executed by the author of this package in the repository working tree during the
assembly window `2026-07-31T06:52:12Z` → `2026-07-31T06:58:36Z`. **No count on this page is
inherited from an earlier report.**

## Environment

```
rustc 1.88.0 (6b00bc388 2025-06-23)
cargo-nextest 0.9.140 (a9fef2964 2026-07-05)
  commit-hash: a9fef2964e34f64ed4fceeee7c0c3559ce560920
  commit-date: 2026-07-05
  host:        x86_64-unknown-linux-gnu
```

Environment variables set for every run:

```
export OSL_CARGO_DISK_CLASS=warm
export OSL_CARGO_JOBS=3
```

## Why `osl-cargo` and not `cargo`

`osl-cargo` (`osl-cargo` (on PATH)) is a repository wrapper that holds a global build
lock and redirects `CARGO_TARGET_DIR` to a shared cache (`~/.cache/osl-cargo-target`). Other work
was building in the same tree concurrently; using bare `cargo` would have raced the shared target
directory. All three suites were run **sequentially**, one cargo invocation at a time, with
`--test-threads=1`.

**Reviewer note:** because the target directory is shared with concurrent builds, the *build* is
not hermetic. The *test execution* is — nextest reports per-test outcomes — but a reviewer
reproducing this should use a clean `CARGO_TARGET_DIR` and a quiescent tree.

## The three runs

Driver (single background shell, sequential loop):

```bash
export OSL_CARGO_DISK_CLASS=warm OSL_CARGO_JOBS=3
for p in osl-ratchet-next crypto keystore; do
  echo "########## PKG=$p ##########"
  osl-cargo -C . nextest run -p "$p" --test-threads=1 2>&1 | tail -60
  echo "########## EXIT=$? ##########"
done
```

### Run 1 — `osl-ratchet-next`

```
osl-cargo -C . nextest run -p osl-ratchet-next --test-threads=1
```
```
     Summary [  76.376s] 108 tests run: 108 passed, 0 skipped
```
Exit code `0`.

Test binaries observed in the output: `bounds`, `carrier_budget`, `healing_latency`,
`interleaving`, `kat`, `negative`, `negotiation`, `pq_healing`, plus in-crate unit tests
(e.g. `osl-ratchet-next skipped::tests::…`). 9 binaries total (confirmed by the control run below,
which reports "across 9 binaries").

Note: `crates/osl-ratchet-next/tests/interleaving.rs` is an **uncommitted** file in this tree
(see README §1), so this run is not reproducible from commit `1eaa7b5` alone.

### Run 2 — `crypto`

```
osl-cargo -C . nextest run -p crypto --test-threads=1
```
```
     Summary [  33.905s] 201 tests run: 201 passed, 0 skipped
```
Exit code `0`.

Integration test files in `crates/crypto/tests/`: `aead_test.rs`, `attachment_test.rs`,
`ed25519_test.rs`, `hkdf_test.rs`, `ml_kem_768_test.rs`, `padding_test.rs`, `pqxdh_test.rs`,
`ratchet_persist_test.rs`, `ratchet_test.rs`, `sender_keys_persist_test.rs`, `sender_keys_test.rs`,
`wire_test.rs`, `x25519_test.rs` (13 files, 3,659 lines).

### Run 3 — `keystore`

```
osl-cargo -C . nextest run -p keystore --test-threads=1
```
```
     Summary [  11.319s] 289 tests run: 289 passed, 2 skipped
```
Exit code `0`.

### Run 3b — confirming run, to read the skip banner

The first run was captured with `tail -60`, which truncated the header line that names the skip
count. The suite was re-run specifically to capture it:

```
osl-cargo -C . nextest run -p keystore --test-threads=1   # grepped for skip/Summary
```
```
    Starting 289 tests across 15 binaries (2 tests skipped)
        PASS [   0.012s] ( 52/289) keystore duress::tests::production_handlers_absence_keeps_remaining_callbacks_skipped
        PASS [   0.013s] (175/289) keystore::duress_test prekey_file_step_skipped_when_path_not_supplied
     Summary [   6.982s] 289 tests run: 289 passed, 2 skipped
```

(The two `PASS` lines above merely matched the grep on the word "skip" in their *names*; they are
passing tests, not the skipped ones.)

`#[ignore]` attributes in `crates/keystore/tests/` that account for the skips:

| File:line | Reason string |
|---|---|
| `crates/keystore/tests/live_keyserver_smoke.rs:14` | `"mutates the explicitly configured live test keyserver"` |
| `crates/keystore/tests/recovery_reseal_test.rs:157` | `"needs a real, persistent OS keyring backend (Windows Credential …"` |
| `crates/keystore/tests/recovery_reseal_test.rs:190` | `"needs a real Windows TPM behind the Microsoft Platform Crypto …"` (also `#[cfg(windows)]`, so not compiled on this host) |

**Consequence, stated plainly:** the two sealers that `select_best_sealer()` actually prefers in
production — TPM and OS keyring — have **zero runtime coverage in this package**.

## Totals

| Suite | Run | Passed | Skipped | Failed | Wall |
|---|---:|---:|---:|---:|---:|
| `osl-ratchet-next` | 108 | 108 | 0 | 0 | 76.376s |
| `crypto` | 201 | 201 | 0 | 0 | 33.905s |
| `keystore` | 289 | 289 | 2 | 0 | 11.319s |
| **Total** | **598** | **598** | **2** | **0** | — |

## Suites NOT run (and therefore not quoted anywhere)

- `-p ipc` — contains `wire_rn.rs` (~4,000 lines incl. tests) and `commands.rs` (~18,000 lines),
  i.e. the OSL-RN integration layer and the send/receive command surface.
- `-p store` — the sealed on-disk database and its schema migrations.
- `apps/osl-hub` test binaries (27 files, 11,392 lines) — including
  `ratchet_lane_signoff_b36.rs`.

These were outside unit `b13`'s time budget with a contended build lock. **No count for them
appears in this package.** A reviewing firm should run them first.

## Negative / selection control

To show that the reported counts come from live test selection rather than being echoed:

```
osl-cargo -C . nextest run -p osl-ratchet-next --test-threads=1 \
    -E 'test(this_test_name_does_not_exist_control)'
```
```
 Nextest run ID e05986fd-7fc8-49b5-9217-ec7a53146755 with nextest profile: default
    Starting 0 tests across 9 binaries (108 tests skipped)
────────────
     Summary [   0.000s] 0 tests run: 0 passed, 108 skipped
error: no tests to run
(hint: use `--no-tests` to customize)
```

Same harness, same package, different filter → `0 tests run` and a non-zero exit.

**What this proves:** the counter is live; the `108` in Run 1 was produced by selecting and
executing 108 tests, and the runner surfaces failure states.

**What this does NOT prove:** that any individual assertion inside those 108 tests is strong. See
README §4.2 — no mutation experiment was performed, and this control is not a substitute for one.

## Raw capture

`nextest-tail-excerpt.txt` in this directory is the raw captured driver output.

**It is truncated.** The driver piped each suite through `tail -60`, so the file contains the last
~60 lines of each run (the final PASS lines and the Summary), not the complete per-test listing.
The Summary lines quoted above are verbatim from it. The `EXIT=` markers in that file are the exit
status of the pipeline as written and should be read alongside the Summary lines, not instead of
them.
