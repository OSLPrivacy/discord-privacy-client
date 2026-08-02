# Cloud AutoScrub — deferred for v1

Status: **DEFERRED**  
Decision date: 2026-08-01  
Applies to: cloud-hosted AutoScrub only. This does not defer local Pro AutoScrub: the owner reviews and explicitly launches its bounded plan, which may then continue without someone watching under its locked Stop/Revoke contract.

## Decision record

```json
{
  "decision": "DEFERRED",
  "scope": "cloud-autoscrub",
  "release": "v1",
  "reconsideration_requires": [
    "high-sensitivity warning",
    "separate explicit consent",
    "per-run isolation",
    "limited retention",
    "wiping",
    "deletion receipt"
  ]
}
```

Cloud AutoScrub is less private because it would process user data outside the device. It is therefore out of scope for v1 and must not be presented as available, active, or planned for automatic cloud execution.

## Current implementation evidence

The repository retains the consent and isolation modelling for a future, separately approved effort; it is not deleted. The five native modules are registered by the library but have no runtime invocation site:

- `apps/osl-hub/src/cloud_autoscrub_authority.rs`
- `apps/osl-hub/src/cloud_autoscrub_consent.rs`
- `apps/osl-hub/src/cloud_autoscrub_envelope.rs`
- `apps/osl-hub/src/cloud_autoscrub_execution.rs`
- `apps/osl-hub/src/cloud_autoscrub_run.rs`

At this record's verification, those modules total 2,067 lines. The earlier approximately-1,900-line estimate is stale; it does not change the deferral.

## Preconditions to revisit

Any proposal to remove this deferral needs a new owner decision and a separately reviewed implementation that supplies every `reconsideration_requires` control above. The proposal must also preserve the open-source app's complete operation without any closed or cloud module installed.

This record implements Scrub contract Rule 9.4 and closes OQ-S3 as **DEFERRED**.
