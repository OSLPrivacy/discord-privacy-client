# TASK 3338b — prove the shared delete approval can fail

TASK 3338 (`let Scrub and timers share one delete action`) says a confirmed
Scrub mark **or** a due timed-delete record approves one exact owned target, and
both then go through the app's **one** delete action. This directory is the
proof that the check behind that claim can go red — that it is a check and not
decoration.

```
apps/osl-hub/task_3338b_shared_delete_approval_can_fail/run-3338b.sh
```

The script needs `CARGO_TARGET_DIR` set to this lane's target directory and
nothing else. Exit 0 means 3338b holds.

## What it does

TASK 3338 was built on branch `lane/f`, commit `159c09ece`, and is not on this
lane's branch. `run-3338b.sh` therefore materialises 3338's own eight files
straight out of that commit's git blobs into a **working copy** under
`/tmp/task-3338b/working-copy` — the three hub sources its check crate compiles
through `#[path]` (`attachment_scan.rs`, `privacy_scan.rs`,
`shared_delete_action.rs`) and the check crate itself. Nothing outside
`/tmp/task-3338b` and `$CARGO_TARGET_DIR/task-3338b` is written, and no sibling
lane's directory is read or written.

Then, in order:

1. run 3338 in the working copy — expected green;
2. copy the working copy to a **throwaway** copy and apply `breaks.patch` to the
   copy only — expected red, naming both breaks;
3. check the working copy is byte-for-byte what it was (sha256) and that this
   lane's `git status` is what it was;
4. run 3338 in the working copy again — expected green;
5. delete the throwaway copy and its target directory.

"Run 3338" means 3338's whole finish line: the
`task_3338_shared_delete_action` test target, plus its four command cases and
`--action-count`, each checked against the strings 3338's evidence claims.
`run_3338` returns 0 when every item holds and **1** when any does not, printing
one `TASK3338B_3338_FAIL item=…` line per item that failed.

## The two breaks

`breaks.patch` is exactly the two the task names, and nothing else:

* **Break 1 — timed delete requires a Scrub mark.** In `approve_delete`, a due
  timed-delete record only approves when a confirmed Scrub mark names the same
  message. The record loop is moved ahead of the mark branch so a record that
  *is* let through still reports itself (`timed_delete_due`) as the reason,
  which keeps the timer path reachable and break 2 observable.
* **Break 2 — a timer-only Discord delete action.** `PerAppDeleteActions` keeps
  a second action for an app as that app's `timer_only` action instead of
  refusing it; `delete_one_owned_target` routes a `DueTimedDelete` approval to
  it; and the fixture registers `discord.timer-only-delete` as Discord's second
  action. The throwaway command grows a `--timer-only-route` mode that shows
  the second action really removes a message, so it is a live second delete path
  and not just a name in a count.

## Why the breaks are checked one at a time as well

`break1-only.patch` and `break2-only.patch` are the same two breaks separately.
They exist so the "names both" gate is not decoration either: with only break 1
the run reports `refused_valid_timer=1 second_discord_delete_action=0`, with
only break 2 it reports `0`/`1`, and both single-break runs end `RESULT=FAIL`.
Only both breaks together give `RESULT=PASS`.

```bash
BREAKS_PATCH=$PWD/apps/osl-hub/task_3338b_shared_delete_approval_can_fail/break1-only.patch \
  apps/osl-hub/task_3338b_shared_delete_approval_can_fail/run-3338b.sh
```

## Host note

`RUSTC_WRAPPER` is cleared inside the script. `sccache` fails intermittently
against this host's `/mnt/d` target directory (`failed to set permissions for
file …: No such file or directory`), and a build that dies in the compiler cache
would be indistinguishable from a red 3338.
