# Task 5205 catalogue gate

`task-5205` loads the exact catalogue embedded in both shipping production
entry points, compares it with an independently written key inventory, and
executes the missing/duplicate/interpolation/change-count acceptance cases.

TASK 5205b adds a compiled semantic oracle that is independent of the
production JSON. It checks every loaded key through both real production entry
functions, including the focused TASK 5209–5213 contracts and the exact 5212
accessibility limit.

`tests/task_5205b_break_it.sh` creates nine attacks only below `mktemp`, checks
all nine through both production callers, requires all 18 refusals to exit 1,
and explicitly removes every external catalogue before reporting success.
`tests/task_5205b_starvation.sh` proves every attack is required without
rebuilding between skip checks.

Run the focused gate from the repository root:

```text
cargo test --manifest-path tools/task-5205-catalogue/Cargo.toml \
  -p task-5205-catalogue --test task_5205_acceptance -- --test-threads=1
tools/task-5205-catalogue/tests/task_5205b_break_it.sh
tools/task-5205-catalogue/tests/task_5205b_starvation.sh
```
