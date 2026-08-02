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

## Space negative proofs

`space-removal.ps1` is the T21-K2 three-VM proof. It invokes the installed
`C:\OSL\space.ps1` UIA bridge on each VM; it is deliberately not a health or
screenshot check. Run `python3 scripts/vmqa/test-space-removal.py` before a
leased Windows run. The bridge must be provisioned on all three VMs or the run
fails closed.

`space-join-history.ps1` is the T21-K3 proof. It crosses the five-minute key
re-emit interval, requires pre-join corpus, and requires exactly one rendered
post-join message. Validate its sabotage guard with
`python3 scripts/vmqa/test-space-join-history.py` before the VM run.

`space-offline.ps1` is the T21-K5 proof. It disables the adapter before
checking local history, queued compose, and immediate local burn; reconnecting
then must refuse the queued send after a roster change. Validate it with
`python3 scripts/vmqa/test-space-offline.py` before the VM run.

Use the scratch directory for fast, disposable experiments. When an experiment
becomes repeatable release coverage, move or reimplement the driver and its
assertions in `scripts/vmqa/`. Do not copy a release verdict back into the
scratch directory or add a second release runner there.
