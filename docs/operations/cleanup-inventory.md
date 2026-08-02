# Cleanup inventory

Run this inventory as scheduled maintenance before proposing any cleanup. It is
read-only: it emits a timestamped report to standard output and exits with
`cleanup_blocked` (exit 3) whenever it finds edits, stashes, a held build lock,
or a crash/WSL uncertainty marker. It does not remove, stage, or rewrite files.

```bash
scripts/fleet/cleanup-inventory.sh --repo /path/to/worktree \
  --lock-path /tmp/osl-cargo.lock --crash-marker /path/to/worktree/.osl-crash-marker \
  > cleanup-inventory-$(date -u +%Y%m%dT%H%M%SZ).txt
```

The report records disk bytes; worktrees, branches, heads and upstreams; dirty
and untracked content fingerprints; stash count; lock owners/PIDs; artifact age
references; crash markers; a recovery plan; and an expected-reclaim estimate.
The estimate intentionally remains unavailable until a later approved reclaim
operation identifies a reproducible target.

`cleanup_ready` is only a preflight result. Preserve the report, resolve every
blocker, compare each material worktree and stash, obtain the owner approval
required by the reclaim procedure, and use the recoverable reclaim tool. Never
turn an inventory result into a low-disk `rm -rf` action.
