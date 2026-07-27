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

## Cloud state

No VM was started and no Windows build was shipped because the pre-spend contract audit failed.
Read-only checks found `OSL-Azure-Client-1` already `VM deallocated` and the subscription-wide
leak check reported nothing running. A run-scoped Azure cleanup receipt was deliberately not
fabricated: without an admissible run and retained build identity there is no honest executable or
build digest to bind it to.
