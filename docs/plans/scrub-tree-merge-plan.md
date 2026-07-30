# Scrub Tree Merge Plan

## Purpose

This plan records the pinned source trees and file ownership for the scrub merge
line. Merge units must use these SHAs as evidence inputs; they must not silently
substitute newer branch heads.

Behavioral acceptance test:
`scrub_tree_merge_plan_records_source_shas_base_tree_and_exclusive_files`.

The test passes only if this plan names all pinned source SHAs, the base tree hash,
and the exclusive files for the scrub merge units. It fails if a merge operator cannot
determine which trees to merge, which tree is the base, or which files are owned by
this lane.

## Source SHAs

| Line | Pinned SHA | Notes |
|---|---|---|
| main | `16778b297d3ec8d0358b7d3812a95f4f8443e462` | Base commit used by f11. |
| f1-footprint | `61933d3a4b50e410e3be1d5e05560d955ee72b4c` | Merge second after main. |
| osl-newest-integration | `403cfa2e090bf76ae4cb2950f3febcc72204fc59` | Merge last; f11 records this line had advanced to `c08941e998bf9906e6629b39584c2630b48b542f` but the pinned plan input remains this SHA. |

## Base Tree

| Field | Value |
|---|---|
| Base commit | `16778b297d3ec8d0358b7d3812a95f4f8443e462` |
| Base tree | `e85e4bd48bfaab810845e307614ab8af238a33d3` |
| Worktree branch created by f11 | `unit-f11` |
| Worktree path recorded by f11 | `/home/<user>/osl-unit-f11` |

Verified merge-bases from f11:

| Pair | Merge-base |
|---|---|
| main / f1-footprint | `4b5b1d30caa45a363382211a020133e64584f6fd` |
| main / osl-newest-integration | `12357c0b9767a4ed69b1025c6d19577ef2d126f1` |
| f1-footprint / osl-newest-integration | `12357c0b9767a4ed69b1025c6d19577ef2d126f1` |

## Merge Order

1. Start from main at `16778b297d3ec8d0358b7d3812a95f4f8443e462`.
2. Merge f1-footprint at `61933d3a4b50e410e3be1d5e05560d955ee72b4c`.
3. Merge osl-newest-integration at `403cfa2e090bf76ae4cb2950f3febcc72204fc59`.
4. Resolve scrub-semantic conflicts only in the exclusive files below.
5. Do not rebase, push, reset, clean, or silently advance to newer branch heads as
   part of this merge plan.

## Exclusive Files

The scrub lane owns these paths for this plan:

| Path | Reason |
|---|---|
| `apps/osl-hub/src/scrub_imap.rs` | Native IMAP scrub inspection, consent ledger, deletion verification, and end-to-end fixture seam. |
| `apps/osl-hub/src/cloud_autoscrub_envelope.rs` | Cloud autoscrub envelope validation for origin/schema/account binding. |
| `apps/osl-hub/src/cloud_autoscrub_consent.rs` | Present local consent module that may receive follow-on wiring if the envelope module remains absent. |
| `docs/plans/scrub-tree-merge-plan.md` | This merge plan and its pinned evidence. |

Current verification-batch exclusive files:

| Path | Unit coverage |
|---|---|
| `apps/osl-hub/src/scrub_imap.rs` | f4, f5, f7, f12, f131 |
| `crates/store/src/anchor.rs` | a48 |
| `crates/ipc/src/secure_local_store.rs` | a140 |
| `docs/plans/scrub-tree-merge-plan.md` | f10 |
| `docs/design/build-order.md` | f83 |
| `apps/osl-hub/src/cloud_autoscrub_envelope.rs` | f149 |

Other files may be read for context, but merge behavior for the scrub lane must not
depend on edits outside these exclusive paths unless a later unit explicitly grants
ownership.
