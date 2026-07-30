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

### Routing exercise fixture

Test name: `Update routing decisions from real Codex capacity instead of stale pools.`

The coordinator test replays these inputs through the routing decision function and compares the
computed decision and reason with the expected fields. A historical pool label is never sufficient
authority; `dispatch` is valid only when the current capacity record is fresh, internally
consistent, quota/account verified, machine headroom is sufficient, and the unit is still owned-file
bound.

The replay evaluator uses these rules:

- `currentCapacityRecord: null` means no live capacity authority exists, so `dispatch` must fail.
- `observedAt` must be no more than 60 seconds before `freshForDecisionAt`.
- `contradictions` must be empty.
- `accountQuotaStatus` must be `verified_enough_for_expected_turn`.
- `machineHeadroom` must be `verified_enough_for_focused_verification`.
- `activeSessionCount` must be a non-negative integer and `blockedOrSleepingSessions` must be an
  array, so the decision is based on an actual session inventory.
- `ownedFileBound` must be `true`.
- A failed live capacity check cannot be repaired by `borrow_other_account`, `change_codex_home`, or
  `start_speculative_background_child`; those substitutes force `refuse`, not `dispatch`.

```json
{
  "name": "Update routing decisions from real Codex capacity instead of stale pools.",
  "freshnessWindowSeconds": 60,
  "cases": [
    {
      "lane": "j14",
      "historicalPoolLabel": "available",
      "currentCapacityRecord": null,
      "forbiddenSubstitute": null,
      "expectedDecision": "standby",
      "expectedReason": "current capacity signal absent"
    },
    {
      "lane": "j14",
      "historicalPoolLabel": "available",
      "currentCapacityRecord": {
        "observedAt": "2026-07-30T09:12:00Z",
        "freshForDecisionAt": "2026-07-30T09:12:20Z",
        "activeSessionCount": 3,
        "blockedOrSleepingSessions": ["j6"],
        "accountQuotaStatus": "verified_enough_for_expected_turn",
        "machineHeadroom": "verified_enough_for_focused_verification",
        "ownedFileBound": true,
        "contradictions": []
      },
      "forbiddenSubstitute": null,
      "expectedDecision": "dispatch",
      "expectedReason": "fresh verified capacity and owned-file bound"
    },
    {
      "lane": "j14",
      "historicalPoolLabel": "available",
      "currentCapacityRecord": {
        "observedAt": "2026-07-30T07:00:00Z",
        "freshForDecisionAt": "2026-07-30T09:12:20Z",
        "activeSessionCount": 1,
        "blockedOrSleepingSessions": [],
        "accountQuotaStatus": "unverified",
        "machineHeadroom": "verified_enough_for_focused_verification",
        "ownedFileBound": true,
        "contradictions": ["quota note expired before decision"]
      },
      "forbiddenSubstitute": "borrow_other_account",
      "expectedDecision": "refuse",
      "expectedReason": "failed live capacity check cannot be satisfied by borrowing authority"
    },
    {
      "lane": "j14",
      "historicalPoolLabel": "available",
      "currentCapacityRecord": {
        "observedAt": "2026-07-30T09:12:00Z",
        "freshForDecisionAt": "2026-07-30T09:12:20Z",
        "activeSessionCount": 2,
        "blockedOrSleepingSessions": [],
        "accountQuotaStatus": "verified_enough_for_expected_turn",
        "machineHeadroom": "verified_enough_for_focused_verification",
        "ownedFileBound": false,
        "contradictions": []
      },
      "forbiddenSubstitute": null,
      "expectedDecision": "standby",
      "expectedReason": "requested work is not bounded to owned files"
    },
    {
      "lane": "j14",
      "historicalPoolLabel": "available",
      "currentCapacityRecord": {
        "observedAt": "2026-07-30T09:12:00Z",
        "freshForDecisionAt": "2026-07-30T09:12:20Z",
        "activeSessionCount": 2,
        "blockedOrSleepingSessions": [],
        "accountQuotaStatus": "verified_enough_for_expected_turn",
        "machineHeadroom": "failed_headroom_check",
        "ownedFileBound": true,
        "contradictions": []
      },
      "forbiddenSubstitute": "change_codex_home",
      "expectedDecision": "refuse",
      "expectedReason": "failed live capacity check cannot be satisfied by changing CODEX_HOME"
    }
  ]
}
```
