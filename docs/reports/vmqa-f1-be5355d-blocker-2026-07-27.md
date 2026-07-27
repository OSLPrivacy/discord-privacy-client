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

## Bounded live-readiness operator successor

The VM-owned operator successor is:

- commit: `fae029b16e12c7468412497ac06016aed1a97d70`
- parent: `abeec4163d7ea22f3a3f6e4b4d90ff67971a5e30`
- tree: `d32bbf1e250f7c0f920e1d2a55a89c1c0830ef4c`
- exact scope: `scripts/vmqa/vmqa-fast-cycle-operator.py` and
  `scripts/vmqa/test-vmqa-fast-cycle-operator.py`
- exact two-file archive SHA-256:
  `429fec908f45b929d3f2b179ca8235331ccad3de7ada9cd0c3d114b89317b2ea`

The operator is one read-only post-run admission command. It has no VM selector, fixture option,
Azure lifecycle verb, build, push, or product-run action. Its only arguments are absolute,
non-symlink paths to an existing production bundle and retained receipt:

```bash
scripts/vmqa/vmqa-fast-cycle-operator.py \
  --receipt /absolute/path/to/live-fast-cycle-receipt.json \
  --bundle /absolute/path/to/producer-build-bundle
```

It first requires the source-pinned `live` fast-cycle receipt, so simulation refuses before any
bundle or Azure access. It then runs the existing production bundle and build-identity validators,
rehashes the exact executable and identity bytes against the receipt, verifies the fixed Azure
subscription plus `OSL-Azure-Client-1` resource and VM ID with read-only queries, fetches only the
fixed live heartbeat blob, and requires those fresh session-1 heartbeat bytes to equal the retained
post-reset observation. The source-pinned receipt validator supplies the exact five-step pair,
old-PID/executable absence, distinct retry nonce/verdict hashes, and warm snapshot/disk lineage
checks.

Focused evidence:

- `python3 scripts/vmqa/test-vmqa-fast-cycle-operator.py -v`: 7/7 passed.
- `python3 scripts/vmqa/test-vmqa-fast-cycle.py -v`: 9/9 passed.
- Python compilation and `git diff --check` passed.
- Mutations cover a coherent wrong subscription lineage, wrong VM resource identity, changed
  executable bytes, changed post-reset heartbeat bytes, stale heartbeat, and normal-command
  simulation refusal.

Read-only Azure checks observed subscription
`a7d5d97b-3bf4-460a-8a7c-6bd11b5810b1` enabled under tenant
`79981b01-1944-4da0-aa9a-fb9f63bddb5e`, and the exact
`OSL-TWO-CLIENT-LAB/OSL-Azure-Client-1` resource reported provisioning state `Succeeded` with VM ID
`a36c21c3-c563-4320-a8eb-70bc588c7caf`. No start, restore, deallocate, snapshot, build, push,
product run, or other Azure mutation was issued.

### Remaining live mechanism

This command cannot pass today: normal receipt validation still stops at
`PINNED_LIVE_RECEIPT_SHA256 = None` before touching Azure. The recorded WARM-bootstrap snapshot is
also not a WARM-agent snapshot. A human must install/sign into disposable Discord, create and
authorize the honest WARM-agent snapshot, run the bounded two-attempt cycle, retain all raw
receipts, and obtain independent audit of the exact live receipt before a source successor pins its
hash.

## Acceptance rows this earns

| Acceptance condition | Evidence tier | Result |
|---|---|---|
| Exact account, subscription, resource group, VM name/ID and session are fixed | source/mutation-tested plus read-only observation | accepted |
| Production bundle identity and executable bytes equal the source-pinned stage claim | source/mutation-tested | accepted |
| Current interactive heartbeat equals the retained fresh post-reset bytes | source/mutation-tested | accepted |
| Normal operator command refuses simulation | subprocess mutation-tested | accepted |
| Authorized WARM-agent cycle and independently pinned live receipt | live/runtime | blocked |

This earns **zero checklist rows and F1 +0**. Simulation is explicitly separate from live proof.
