# VM QA harness roles

There are two VM QA directories. They serve different purposes and are not
interchangeable.

| Directory | Role | May gate a release? |
| --- | --- | --- |
| `scripts/vmqa/` in the OSL repository | The authoritative, release-grade harness. It validates its contract, namespaces VM resources, holds a per-VM Azure lease, and rejects vacuous passes. Run `scripts/vmqa/vmqa-run.sh` for release evidence. | **Yes** |
| `/home/liamw/osl-plan/vmqa/` | Scratch drivers and diagnostics for iterating on a VM or investigating a failure. Its output is exploratory evidence only. | **No** |

## Release rule

No command, log, screenshot, or result produced only by the scratch harness may
be reported as a release pass. A result discovered there must be reproduced
through `scripts/vmqa/vmqa-run.sh`; only that runner's validated verdict and
artifacts can satisfy a release gate.

## Keeping the split useful

Use the scratch directory for fast, disposable experiments. When an experiment
becomes repeatable release coverage, move or reimplement the driver and its
assertions in `scripts/vmqa/`. Do not copy a release verdict back into the
scratch directory or add a second release runner there.
