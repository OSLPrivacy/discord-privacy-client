# OSL dual-demo plan -- 2026-07-29

Authority: local release-owner reconciliation only. This file starts the
dual-demo plan by naming the dirty-worktree root that later inventory and merge
steps must preserve before they consume any unmerged source as evidence.

## 1. Ground truth {#section-1-ground-truth}

95 worktrees hold unmerged local state. They are not interchangeable build
inputs, and none of their dirty changes are release evidence until an owner
packet names the exact source, purpose, changed paths, and acceptance boundary.

The root rule is preserve first, reconcile second, and merge only after semantic
inventory. A clean integration line may consume a worktree only after the owner
has explicitly authorized that worktree's packet for the exact row being
advanced.

Absence of owner consent, binding to an accepted row, or reconciliation
authority is refusal. It never implies permission to clean up, merge, discard,
or treat dirty bytes as a completed demo dependency.

Acceptance check for this unit:

```text
grep -F '95 worktrees hold' docs/plans/osl-dual-demo-plan-2026-07-29.md
```
