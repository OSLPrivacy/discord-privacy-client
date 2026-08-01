# Parallel Cargo lanes

Use `scripts/fleet/lane.sh` for local multi-lane Cargo work:

```bash
scripts/fleet/lane.sh t1 test -p crypto
scripts/fleet/lane.sh t2 check -p storage
```

Each lane receives `CARGO_TARGET_DIR=target/fleet/<lane-name>` (or
`$OSL_FLEET_TARGET_ROOT/<lane-name>` when that test-only/temporary root is set), so it never
shares build artefacts with another lane. Lane names are limited to one safe path component.

The launcher executes plain `cargo` with `-j 4` and rejects caller-provided Cargo job options.
Four simultaneous lanes therefore issue no more than sixteen Cargo jobs. Do not use
`osl-cargo`: it remaps worktrees onto a shared target directory and serialises the fleet.

Run the black-box launcher check with:

```bash
scripts/fleet/lane.sh --self-test
```

It starts four concurrent fake Cargo lanes, samples `/proc` for their Cargo-job workers, and
verifies four distinct target directories plus a fleet maximum of sixteen workers. The fake binary
lets this validate launcher behaviour without compiling the workspace.
