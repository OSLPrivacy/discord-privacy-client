# TASK 5131 Messenger composer contract

This is an isolated test/development workspace. It holds the non-empty visual
contract for Messenger's protected-composer catalogue entry and a fail-closed
source inventory. It is not a member or dependency of any shipping OSL
workspace, binary, installer, painter, action registry, or provider registry.

The contract intentionally describes only geometry, typography, and visual
style. It grants no runtime authority and its `shippingEligible` value must
remain `false`. The checker independently scans six shipping scopes: production
imports, installer files, release-manifest rows, runtime providers, painters,
and installed actions. Every Messenger compositor count must stay at zero.

Run the unchanged gate from the repository root:

```text
CARGO_TARGET_DIR=/mnt/d/osl-lane-targets/x cargo run \
  --manifest-path tools/task-5131-messenger-contract/Cargo.toml \
  -p task-5131-messenger-contract \
  --bin task-5131-messenger-contract-check -- .
```

The test target exercises empty-contract, per-key starvation, per-inventory
starvation, and per-component promotion attacks against that same binary.
