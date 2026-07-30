# OSL parallel build plan - 2026-07-29

## risk-codex-quota

Codex quota may run out during a parallel implementation wave, so routing must use current,
observable Codex capacity instead of stale lane pools or cached availability notes.

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

Machine-checkable acceptance contract:

```json
{
  "schemaVersion": 1,
  "tests": [
    {
      "name": "Update routing decisions from real Codex capacity instead of stale pools.",
      "evaluator": "coordinator-routing-capacity-v1",
      "cases": [
        {
          "name": "stale available pool refuses",
          "historicalPoolLabel": "available",
          "currentCapacityRecord": {
            "status": "absent"
          },
          "attemptedSubstitutes": [],
          "expectedDecisions": ["refuse", "standby"],
          "requiredReason": "missing_current_capacity_signal"
        },
        {
          "name": "fresh verified capacity permits dispatch",
          "historicalPoolLabel": "available",
          "currentCapacityRecord": {
            "status": "fresh",
            "activeSessionCount": 1,
            "blockedOrSleepingSessionsRecorded": true,
            "accountQuotaStatus": "verified_enough_for_expected_turn",
            "machineHeadroom": "enough_for_focused_verification",
            "ownedFileBound": true
          },
          "attemptedSubstitutes": [],
          "expectedDecisions": ["dispatch"],
          "requiredReason": "live_capacity_verified"
        },
        {
          "name": "failed live capacity cannot be bypassed",
          "historicalPoolLabel": "available",
          "currentCapacityRecord": {
            "status": "fresh",
            "activeSessionCount": 1,
            "blockedOrSleepingSessionsRecorded": true,
            "accountQuotaStatus": "verified_insufficient_for_expected_turn",
            "machineHeadroom": "insufficient_for_focused_verification",
            "ownedFileBound": true
          },
          "attemptedSubstitutes": [
            "borrow_account",
            "change_codex_home",
            "start_speculative_background_work"
          ],
          "expectedDecisions": ["refuse", "standby"],
          "requiredReason": "capacity_check_failed"
        }
      ],
      "refusedSubstitutes": [
        "borrow_account",
        "change_codex_home",
        "start_speculative_background_work"
      ]
    }
  ]
}
```
