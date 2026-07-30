# OSL parallel build plan - 2026-07-29

## risk-codex-quota

Codex quota may run out during a parallel implementation wave, so routing must use current,
observable Codex capacity instead of stale lane pools or cached availability notes.

Behavioral acceptance test:
`docs/plans/osl-parallel-build-plan-2026-07-29.md`

The test passes only if the coordinator decision can be made from fresh capacity facts, not from
historical pool labels. It fails if an `available` label without a current capacity record permits
dispatch, if a failed quota or headroom check can be bypassed by account or `CODEX_HOME`
substitution, or if a dispatch omits the owned-file bound.

Before dispatching or resuming a Codex child, the coordinator records the real capacity signal used
for that decision: active session count, blocked or sleeping sessions, current account/quota status,
and whether the requested work is still bounded to the recipient's owned files. If the capacity
signal is absent, stale, contradictory, or tied to an unverified account state, the only permitted
routing decision is refusal or standby.

Stale pool labels such as "available", "green", or "idle" are advisory history, not authority. A
lane can receive new work only when a fresh check shows enough Codex quota for the expected turn and
enough machine headroom for the requested focused verification. If either check fails, the
coordinator leaves the lane idle and records the reason rather than borrowing from another account,
changing `CODEX_HOME`, or starting speculative background work.

Routing decisions are derived only from the current capacity record:

| Historical pool label | Current capacity record | Forbidden substitute | Decision |
| --- | --- | --- | --- |
| `available` | absent, stale, contradictory, or unverified account/quota state | none | `refuse` or `standby` |
| `available` | fresh active-session count, blocked/sleeping-session list, verified quota/account status, enough machine headroom, and owned-file bound | none | `dispatch` allowed |
| any value | failed quota/headroom check | borrowed account, changed `CODEX_HOME`, speculative background child | `refuse` or `standby` |

Acceptance for this risk is a coordinator routing exercise, not a prose grep:

1. Present a lane whose historical pool label says `available`, but whose current capacity signal is
   absent, stale, contradictory, or tied to an unverified account/quota state. The routing decision
   must be `refuse` or `standby`, with the reason recorded.
2. Present the same lane with a fresh active-session count, blocked/sleeping-session list, verified
   current quota/account status, machine headroom, and an owned-file bound for the requested unit. The
   routing decision may be `dispatch` only if those live facts show enough capacity for the expected
   turn.
3. Attempt to satisfy a failed capacity check by borrowing another account, changing `CODEX_HOME`, or
   starting speculative background work. The routing decision must remain `refuse` or `standby`.

## Acceptance Fixture

Machine-checkable acceptance contract:

```json
{
  "schemaVersion": 1,
  "testName": "docs/plans/osl-parallel-build-plan-2026-07-29.md",
  "routingInputs": [
    {
      "case": "stale_available_pool_refuses_without_live_capacity",
      "historicalPoolLabel": "available",
      "currentCapacityRecord": {
        "present": false,
        "fresh": false,
        "contradictory": false,
        "verifiedAccountQuota": false,
        "activeSessionCountRecorded": false,
        "blockedOrSleepingSessionsRecorded": false,
        "machineHeadroomEnough": null,
        "ownedFileBound": true
      },
      "forbiddenSubstituteAttempted": null,
      "allowedDecisions": ["refuse", "standby"],
      "forbiddenDecisions": ["dispatch"],
      "reasonRequired": true
    },
    {
      "case": "fresh_capacity_and_owned_files_allow_bounded_dispatch",
      "historicalPoolLabel": "available",
      "currentCapacityRecord": {
        "present": true,
        "fresh": true,
        "contradictory": false,
        "verifiedAccountQuota": true,
        "activeSessionCountRecorded": true,
        "blockedOrSleepingSessionsRecorded": true,
        "machineHeadroomEnough": true,
        "ownedFileBound": true
      },
      "forbiddenSubstituteAttempted": null,
      "allowedDecisions": ["dispatch", "refuse", "standby"],
      "dispatchPreconditions": [
        "present",
        "fresh",
        "verifiedAccountQuota",
        "activeSessionCountRecorded",
        "blockedOrSleepingSessionsRecorded",
        "machineHeadroomEnough",
        "ownedFileBound"
      ],
      "reasonRequiredForNonDispatch": true
    },
    {
      "case": "failed_quota_cannot_be_patched_by_substitution",
      "historicalPoolLabel": "available",
      "currentCapacityRecord": {
        "present": true,
        "fresh": true,
        "contradictory": false,
        "verifiedAccountQuota": false,
        "activeSessionCountRecorded": true,
        "blockedOrSleepingSessionsRecorded": true,
        "machineHeadroomEnough": true,
        "ownedFileBound": true
      },
      "forbiddenSubstituteAttempted": [
        "borrowed_account",
        "changed_CODEX_HOME",
        "speculative_background_child"
      ],
      "allowedDecisions": ["refuse", "standby"],
      "forbiddenDecisions": ["dispatch"],
      "reasonRequired": true
    }
  ],
  "inversionsThatMustFail": [
    "dispatch_from_available_label_without_current_capacity_record",
    "dispatch_after_failed_quota_check",
    "dispatch_after_failed_headroom_check",
    "dispatch_without_owned_file_bound",
    "dispatch_by_borrowing_an_account",
    "dispatch_by_changing_CODEX_HOME",
    "dispatch_by_starting_speculative_background_work"
  ]
}
```
