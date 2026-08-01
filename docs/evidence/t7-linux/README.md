# T7-01 Linux audit status

No verdict is recorded for defects 12, 13, 14, or 17. The required re-shoot must run a native
`osl-hub` binary freshly built after `apps/osl-hub-ui/dist` has been regenerated, under
`Xvfb :77` at 1440x900x24 with XTEST input. This task's author rule forbids running Cargo, and
there was no pre-existing binary or Linux evidence bundle to audit. Producing any CONFIRMED or
STALE verdict from source inspection would be inadmissible.

`audit.json` is deliberately machine-checked. It may only change to `complete` when the captured
build identity, status-tag screenshot, and run log exist and all verdicts are assessed.
