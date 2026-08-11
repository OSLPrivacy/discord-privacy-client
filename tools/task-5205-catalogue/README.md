# Task 5205 catalogue gate

`task-5205` loads the exact catalogue embedded in both shipping production
entry points, compares it with an independently written key inventory, and
executes the missing/duplicate/interpolation/change-count acceptance cases.

`tests/task_5205b_break_it.sh` creates only temporary external catalogue
copies, runs the literal-fallback, duplicate, malformed-interpolation and
permissive-entry-point attacks, verifies exit 1 and removes every copy.

Run the focused gate from the repository root:

```text
cargo test --manifest-path tools/task-5205-catalogue/Cargo.toml \
  -p task-5205-catalogue --test task_5205_acceptance -- --test-threads=1
tools/task-5205-catalogue/tests/task_5205b_break_it.sh
```
