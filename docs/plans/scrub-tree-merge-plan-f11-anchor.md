# f11 — integration branch anchor

**Unit:** f11 (rank 840, 0 dependencies). Sibling note to
`docs/plans/scrub-tree-merge-plan.md` (MERGE.T1), which this unit follows without
re-deriving. This unit performed **git-only** work: no code changes, no builds, no
merges. It only creates the branch/worktree the merge units (T7/T8/S6/S7/S10/S12/S13
etc.) will build on.

Behavioral acceptance test:
`integration_branch_anchor_records_osl_newest_integration_403cfa2`.

The test passes only if this anchor record names the integration source line as
`osl-newest-integration`, preserves the pinned input SHA
`403cfa2e090bf76ae4cb2950f3febcc72204fc59`, and records that the branch was created
as the plan's integration anchor without mutating the source checkouts. It fails if
the integration source is absent, if a newer integration HEAD is silently substituted
for the pinned SHA, or if the branch/worktree cannot be identified.

## What was created

| Field | Value |
|---|---|
| Branch | `unit-f11` |
| Worktree path | `/home/<user>/osl-unit-f11` |
| Base SHA (per plan §2 "Base tree") | `16778b297d3ec8d0358b7d3812a95f4f8443e462` (main HEAD at time the plan was written) |
| Tree hash at base | `e85e4bd48bfaab810845e307614ab8af238a33d3` |
| Created via | `git -C /home/<user>/discord-privacy-client worktree add -b unit-f11 /home/<user>/osl-unit-f11 16778b297d3ec8d0358b7d3812a95f4f8443e462` |
| Working tree state | clean (0 dirty files) immediately after creation |

This branch currently contains **only** main's tree at the plan's pinned SHA — no
f1-footprint or osl-newest-integration content has been merged in yet. That is
deliberate: f11's job was only to establish the anchor point. The 0-conflict
f1-footprint merge and the 87-real-conflict osl-newest-integration merge (plan §2, §4)
are later units' work, to be performed **on this branch/worktree**, in the order the
plan specifies (main → f1-footprint → osl-newest-integration).

## Verification performed before branching (2026-07-29)

All three plan SHAs still resolve as valid objects and none have been rewritten:

| Line | Plan SHA | Still resolves | Still ancestor of current line HEAD |
|---|---|---|---|
| main | `16778b2...` | yes | yes — current main HEAD `73dccd58...` is exactly 1 commit ahead |
| f1-footprint | `61933d3...` | yes | yes — `61933d3...` **is** current f1-footprint HEAD (unchanged) |
| osl-newest-integration | `403cfa2...` | yes | yes — current integration HEAD `c08941e9...` is exactly 1 commit ahead |

Merge-bases re-verified and match the plan exactly:
- main ∩ f1-footprint = `4b5b1d30caa45a363382211a020133e64584f6fd`
- main ∩ osl-newest-integration = f1-footprint ∩ osl-newest-integration = `12357c0b9767a4ed69b1025c6d19577ef2d126f1`

**Drift found (minor, does not invalidate the plan):** all three source lines have
advanced since the plan was written — main +1 commit (dirty files 109→114),
f1-footprint dirty-file count 0→1 (HEAD itself unchanged), osl-newest-integration +1
commit (dirty files 47→48). No rebase/force-push on any line — every plan SHA is
still a clean ancestor of its line's current HEAD, so the plan's measurements (87
conflict files, store schema v8/v4/v3, etc.) remain valid against these exact pinned
SHAs. **Recommendation for the merge units:** merge against the pinned SHAs above
(not each line's newer HEAD), since that's what the 87-conflict count and every other
measurement in the plan was taken against; re-running `git merge-tree` against the
newer HEADs was out of scope for f11 and was not done.

**Note on this unit's title vs. the plan:** f11's ticket title read "Create the
integration branch from `osl-newest-integration@403cfa2`." The plan (§2) is explicit
that the *base tree* is **main HEAD** (`16778b2`), with f1-footprint merged in second
and osl-newest-integration — despite being the tree's namesake — merged in **last**,
specifically because it carries the most conflicts and the most material that must be
deliberately excluded (do-not-merge list, plan §6). Branching from `403cfa2` directly
would have contradicted the plan's own stated rationale and thrown away the "central
files stabilize once via f1 before absorbing integration's 87 conflicts" ordering
argument (plan §2). f11 followed the plan's explicit base (main `16778b2`) rather than
the ticket title's literal SHA, per this unit's own instructions ("Create the
integration branch/worktree per that plan's stated base and order" / "if the plan
doc's base SHA no longer resolves or conflicts with reality, STOP and report rather
than improvising a different base" — the plan's base SHA resolved cleanly, so no stop
was needed, but the title/plan mismatch is flagged here for whoever picks up the next
unit).

## Confirmation: no mutation of the three source checkouts

`git worktree add` only registers a new worktree in `discord-privacy-client`'s
`.git/worktrees` metadata and creates a new branch ref (`unit-f11`); it does not touch
any existing checkout's HEAD, index, or working tree. Verified HEADs of all three
source repos immediately after worktree creation match their pre-creation values
exactly:

- `/home/<user>/discord-privacy-client` → `73dccd588a8e07c48d5165bb05b77150539ce0e9` (unchanged)
- `/home/<user>/discord-privacy-client-f1-footprint` → `61933d3a4b50e410e3be1d5e05560d955ee72b4c` (unchanged)
- `/home/<user>/osl-newest-integration` → `c08941e998bf9906e6629b39584c2630b48b542f` (unchanged)

No stash, reset, checkout, clean, merge, or push was run in any of the three source
repos.
