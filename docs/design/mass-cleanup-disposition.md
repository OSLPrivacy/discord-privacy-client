---
id: MASS-CLEANUP-VERDICT
status: accepted
decision: retire-duplicate-seam
authority: scrub-adapter-engine
transition: retain-manifest-and-entitlement-until-engine-parity
safety: remain-fail-closed-until-parity
---

# Mass Cleanup disposition

## Decision

Retire `mass_cleanup.rs` as an independent capability, consent, and execution
boundary. The `ScrubAdapter` engine is the only execution authority for every
Scrub deletion, including the actions previously described as Mass Cleanup.

## Transition

Keep the service catalogue and the native Pro entitlement at the product
boundary only until the engine can represent the same service/action catalogue
and enforce its attended review, one-shot consent, dry-run, verification, and
tri-state receipt contract. At that point remove the Mass Cleanup discovery and
execution DTOs, commands, permission entries, and UI loading seam; callers use
the engine route instead.

The catalogue is planning metadata, not execution authority. It may advertise
only actions that a reviewed adapter can perform and verify through the engine.

## Current safety posture

The transition is not permission to activate the existing seam. It stays
fail-closed: no unreviewed adapter, no discovery or mutation, and no successful
deletion result. The existing UI invokes the IMAP engine command names, but the
native implementations required by T12-I6 are not present yet; removal begins
only after that engine route has reached parity.

## Rationale

`mass_cleanup.rs` currently carries a separate manifest, request DTOs, typed
confirmation, entitlement check, and rejecting discovery/execution commands.
Running that beside `ScrubAdapter` would create two destructive-policy systems
whose consent and receipt rules can drift. One engine preserves the D40 safety
spine: attended operation, explicit review, dry runs, verified outcomes, and
`Unknown` never presented as success.

## Follow-through

When parity is reached, the implementing task must remove the duplicate native
and UI seams together, update their allowlist entries, and demonstrate that all
Mass Cleanup entry points route through `ScrubAdapter`. This record does not
authorize a direct deletion of the module before that migration.
