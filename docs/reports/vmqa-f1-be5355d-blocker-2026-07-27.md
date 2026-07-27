# VMQA F1 exact-build blocker — 2026-07-27

## Decision

**Do not execute the F1 walkthrough. This task earns F1 +0 and makes no checklist movement.**

The requested Scrub harness landed within the ten-minute watch window:

- commit: `be5355d79e558ad6abf8f3cc5ee0a228829e7def`
- parent: `0e6aed68537b49a79316a7ae4b86c2a68b91693b`
- tree: `56427995071bf2224627ada8da447545b19ba55e`
- subject: `QA: bind F1 walkthrough to persisted revoke reread`

The audit read exact Git objects and a clean archive, not the dirty shared worktree.

## Product-side blockers

The product command boundary exists: the renderer invokes
`list_browser_profile_choices`, `set_browser_profile_consent`, and
`begin_protected_browser_import`; the Rust commands require an unlocked owner, are registered, and
have explicit Tauri permissions. The committed walkthrough cannot retain authoritative evidence
from that boundary:

1. It records UI Automation checkbox/text state, not every native IPC request and response. A
   renderer click probably reaches Tauri, but the receipt cannot distinguish that from visible
   state produced without the command.
2. Its post-revoke “fresh reread” leaves the browser route through `Not now` and returns with
   `Back` in the same process. It does not stop, relaunch, re-attest, and reread in a fresh process.
3. It accepts any visible non-Firefox source and serializes raw browser/profile names. It does not
   bind the selected tuple to the credential-free seeded Chromium/Brave manifest or use a
   privacy-safe profile identity.
4. Its capture sidecar does not retain the exact trusted target window class, raw and DWM
   pre/post rectangles, or a verifier-checked PNG decode/dimension/color contract.
5. Its cleanup verifier accepts only `stopped` or `already-exited`; it does not require retained
   PID absence and zero processes at the exact executable path.
6. Its F1 tests mutate caller-authored JSON and accept arbitrary text bytes named `.png`. They
   prove the JSON guard can reject empty import, missing revoke, and stale claimed identity, not
   that those negatives fail through a live native boundary.
7. It has no recursively closed V2 request/verdict schema and no independently recomputable
   build-identity object. It cannot supply the stronger VMQA run contract without a Scrub-owned
   follow-up commit.

These are product-harness omissions. The VM grader must not infer or synthesize the missing facts.

## VM-owned hardening

The VMQA contract now requires:

- schema version 2 exactly, with unknown fields rejected recursively;
- a clean-source build identity binding commit, tree, empty dirty-state digest, UI dist digest,
  exact Windows release target/features/build argv, toolchain identity, and executable/loader
  size and SHA-256;
- independent host measurement of retained executable bytes, plus a Git source archive,
  deterministic dist archive/manifest, raw npm/Cargo output, and a closed build log whose Cargo
  compiler-artifact path and digest bind those exact bytes to the named source and argv; the
  verifier requires an independently supplied expected commit/tree and recomputes the Git tree ID
  from the retained archive instead of trusting its label;
- request, verdict, and build-identity digest agreement;
- normal and blocked live verdict producers using the exact V2 fields;
- recursive scalar types, timestamp ordering, and exact request/result step correspondence;
- retained raw Azure instance view, detailed census, followed subscription REST pages, projections,
  and cleanup JSON whose hashes bind the request, verdict, executable, build identity, target VM
  and resource group, authoritative `PowerState/deallocated`, complete target membership, and zero
  running VMs.

Focused local evidence:

- `scripts/vmqa/test-selftest-grading.sh`: 87 passed, 0 failed.
  - Includes V999, unknown top/nested field, nested object-for-scalar substitutions, stale embedded
    identity, and coherent executable digest substitution through both selftest and ordinary-run
    verification.
- `python3 scripts/vmqa/test-vmqa-contract.py -v`: 19 passed.
  - Includes clean unrelated source plus arbitrary executable, Azure V999/unknown fields, raw-byte
    and projection drift, cross-run swaps, prose-only deallocation, empty/partial/incomplete
    censuses, pagination, timestamp drift, and nested scalar substitutions.
- Bash syntax, Python compilation, PowerShell parser, and `git diff --check`: passed.

## Acceptance rows this earns

| Acceptance condition | Evidence tier | Result |
|---|---|---|
| Exact V2 schemas; V999 and unknown fields refused | mutation-tested | accepted |
| Normal and invalid/interrupted blocked producers carry the required V2 identity fields | source-shape mutation plus retained-contract test | accepted |
| Independently selected commit/tree bind source archive, dist, argv, logs, and executable | archive tree recomputation plus product-shaped unrelated-source mutation | accepted |
| Ordinary run independently hashes local executable bytes and retains build evidence | ordinary-run coherent-substitution mutation | accepted |
| Nested schema scalars retain exact types | request, verdict, build, and cleanup object-for-scalar mutations | accepted |
| Cleanup binds retained request, verdict, build identity, and coherent timestamps | cross-run and timestamp mutations | accepted |
| Raw Azure state authoritatively proves `PowerState/deallocated` | raw/projection and prose-only deallocation mutations | accepted |
| Detailed census equals a fully followed paginated subscription census and contains the target | target-absence, partial-census, and incomplete-`nextLink` mutations | accepted |

This earns **8/8 VMQA contract acceptance conditions at test-proven tier**. It earns **zero
checklist rows and F1 +0**: no Windows walkthrough was executed, and the product-side blockers above
remain outside VM ownership.

## Cloud state

No VM was started and no Windows build was shipped because the pre-spend contract audit failed.
Read-only checks found `OSL-Azure-Client-1` already `VM deallocated` and the subscription-wide
leak check reported nothing running. A run-scoped Azure cleanup receipt was deliberately not
fabricated: without an admissible run and retained build identity there is no honest executable or
build digest to bind it to.

## Latest VM lane checkpoint — minimal warm reset/retry receipt

The VM-owned successor is:

- commit: `ff117dd12422444b0e41aa45e20dd2c3397226d3`
- parent: `893251f5c71f82c9c0edf6f92663ed5cd11ab0ee`
- tree: `45142200f7deed12ea1424c4e1a17793bc851a17`
- exact scope: `scripts/vmqa/vmqa_fast_cycle_receipt.py` and
  `scripts/vmqa/test-vmqa-fast-cycle.py`
- exact two-file archive SHA-256:
  `480fd55bb406da7ffac643c18fba7003f47113ca5d6ed0a484ac8ac17fef8b7a`

The exact archived focused suite ran 9 tests and passed. The validator is deliberately limited to
the existing fast path:

1. exact `OSL-Azure-Client-1` WARM-agent snapshot and restored-disk lineage;
2. fresh interactive session-1 agent heartbeat;
3. pinned source/tree, build identity and staged executable hash;
4. one complete non-pass five-step `stage,launch,ping,shot,kill` selftest pair;
5. reset proving the old PID and exact executable are absent;
6. a fresh agent heartbeat and distinct passing five-step selftest pair.

Simulation requires the explicit internal fixture switch and remains labelled `simulation` with
`runtimeProvenByThisValidator: false`. Normal validation rejects simulation. It also rejects every
caller-authored `live` receipt because
`scripts/vmqa/vmqa_fast_cycle_receipt.py:34` leaves
`PINNED_LIVE_RECEIPT_SHA256` unset; the fail-closed check is at lines 491–496.

### Precise live blocker

The only recorded usable VM snapshot is
`OSL-Azure-Client-1-WARM-bootstrap-202607270252`. The retained lane report records at lines 240–247
that this snapshot has not earned the WARM-agent name and that Discord is not installed. The
fast-cycle validator requires `OSL-Azure-Client-1-WARM-agent-<timestamp>` at
`scripts/vmqa/vmqa_fast_cycle_receipt.py:50–52` and carries a mutation proving WARM-bootstrap
cannot impersonate WARM-agent.

Therefore the next step is external and currently blocked: the VMQA setup owner must install and
sign into disposable Discord on `OSL-Azure-Client-1`, deallocate it, take an honest WARM-agent
snapshot, then explicitly authorize restore/start and the interactive two-attempt cycle. Only
after the raw receipt is independently audited may a successor pin its exact hash. No Azure
command was run for `ff117dd`.

## Acceptance rows this earns

| Acceptance condition | Evidence tier | Result |
|---|---|---|
| Minimal warm-reset retry receipt has exact target, lineage, build, agent and five-step selftest bindings | source/mutation-tested | accepted |
| WARM-bootstrap, stale executable, empty/skipped run, ineffective reset and replayed retry refuse | source/mutation-tested | accepted |
| Simulation cannot cross the normal live boundary | exact-archive focused test | accepted |
| Authorized WARM-agent restore, interactive run and exact live receipt | live/runtime | blocked |

This earns **zero checklist rows and F1 +0**. It is retained-receipt validation, not a VM run.
