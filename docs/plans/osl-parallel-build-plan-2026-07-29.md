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

### Routing exercise fixture

Test name: `Update routing decisions from real Codex capacity instead of stale pools.`

The coordinator test replays these inputs through the routing decision function. A historical pool
label is never sufficient authority; `dispatch` is valid only when the current capacity record is
fresh, internally consistent, quota/account verified, machine headroom is sufficient, and the unit
is still owned-file bound.

Freshness is deliberately narrow for dispatch: `observedAt` must be no more than five minutes before
`freshForDecisionAt`, and it must not be in the future relative to the decision timestamp.

```json
{
  "name": "Update routing decisions from real Codex capacity instead of stale pools.",
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
        "observedAt": "2026-07-30T09:07:20Z",
        "freshForDecisionAt": "2026-07-30T09:12:20Z",
        "activeSessionCount": 2,
        "blockedOrSleepingSessions": [],
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
        "observedAt": "2026-07-30T09:12:05Z",
        "freshForDecisionAt": "2026-07-30T09:12:20Z",
        "activeSessionCount": 3,
        "blockedOrSleepingSessions": ["j6"],
        "accountQuotaStatus": "verified_enough_for_expected_turn",
        "machineHeadroom": "insufficient_for_focused_verification",
        "ownedFileBound": true,
        "contradictions": []
      },
      "forbiddenSubstitute": "change_CODEX_HOME",
      "expectedDecision": "refuse",
      "expectedReason": "failed machine headroom check cannot be satisfied by changing CODEX_HOME"
    },
    {
      "lane": "j14",
      "historicalPoolLabel": "available",
      "currentCapacityRecord": {
        "observedAt": "2026-07-30T09:12:05Z",
        "freshForDecisionAt": "2026-07-30T09:12:20Z",
        "activeSessionCount": 3,
        "blockedOrSleepingSessions": ["j6"],
        "accountQuotaStatus": "verified_enough_for_expected_turn",
        "machineHeadroom": "verified_enough_for_focused_verification",
        "ownedFileBound": false,
        "contradictions": []
      },
      "forbiddenSubstitute": "speculative_background_child",
      "expectedDecision": "standby",
      "expectedReason": "unbounded ownership cannot be satisfied by speculative background work"
    }
  ]
}
```

## routing-fixtures

These fixtures are the acceptance test named `docs/plans/osl-parallel-build-plan-2026-07-29.md`.
They intentionally separate the historical pool label from the current capacity record, so a router
that dispatches from stale pool state or forbidden substitutes fails the exercise.

```json
{
  "test_name": "docs/plans/osl-parallel-build-plan-2026-07-29.md",
  "cases": [
    {
      "name": "stale_available_pool_refuses",
      "historical_pool_label": "available",
      "current_capacity_record": {
        "observed_at_unix_seconds": null,
        "active_session_count": null,
        "blocked_or_sleeping_sessions": null,
        "quota_account_status": "unverified",
        "machine_headroom": "unknown",
        "owned_file_bound": true
      },
      "forbidden_substitute_attempted": null,
      "expected_decisions": ["refuse", "standby"]
    },
    {
      "name": "fresh_owned_capacity_may_dispatch",
      "historical_pool_label": "available",
      "current_capacity_record": {
        "observed_at_unix_seconds": 1785369600,
        "active_session_count": 2,
        "blocked_or_sleeping_sessions": [],
        "quota_account_status": "verified_enough_for_expected_turn",
        "machine_headroom": "enough_for_focused_verification",
        "owned_file_bound": true
      },
      "forbidden_substitute_attempted": null,
      "expected_decisions": ["dispatch"]
    },
    {
      "name": "failed_capacity_cannot_be_repaired_by_substitute",
      "historical_pool_label": "available",
      "current_capacity_record": {
        "observed_at_unix_seconds": 1785369600,
        "active_session_count": 9,
        "blocked_or_sleeping_sessions": ["lane-b"],
        "quota_account_status": "verified_insufficient_for_expected_turn",
        "machine_headroom": "insufficient",
        "owned_file_bound": true
      },
      "forbidden_substitute_attempted": "changed_CODEX_HOME",
      "expected_decisions": ["refuse", "standby"]
    }
  ]
}
```

Executable oracle for the fixture:

```js
import assert from "node:assert/strict";

const TEST_NAME = "Update routing decisions from real Codex capacity instead of stale pools.";
const FRESHNESS_WINDOW_MS = 5 * 60 * 1000;

function routeCodexChild({ currentCapacityRecord, forbiddenSubstitute }) {
  if (currentCapacityRecord === null) {
    return {
      decision: "standby",
      reason: "current capacity signal absent",
    };
  }

  const record = currentCapacityRecord;
  const observedAt = Date.parse(record.observedAt);
  const decisionAt = Date.parse(record.freshForDecisionAt);
  const stale =
    !Number.isFinite(observedAt) ||
    !Number.isFinite(decisionAt) ||
    observedAt > decisionAt ||
    decisionAt - observedAt > FRESHNESS_WINDOW_MS;
  const contradictions = Array.isArray(record.contradictions)
    ? record.contradictions
    : ["capacity record contradictions missing"];
  const hasFreshLiveCapacity =
    !stale &&
    contradictions.length === 0 &&
    Number.isInteger(record.activeSessionCount) &&
    Array.isArray(record.blockedOrSleepingSessions) &&
    record.accountQuotaStatus === "verified_enough_for_expected_turn";
  const hasMachineHeadroom =
    record.machineHeadroom === "verified_enough_for_focused_verification";
  const ownedFileBound = record.ownedFileBound === true;

  if (!hasFreshLiveCapacity && forbiddenSubstitute === "borrow_other_account") {
    return {
      decision: "refuse",
      reason: "failed live capacity check cannot be satisfied by borrowing authority",
    };
  }

  if (!hasMachineHeadroom && forbiddenSubstitute === "change_CODEX_HOME") {
    return {
      decision: "refuse",
      reason: "failed machine headroom check cannot be satisfied by changing CODEX_HOME",
    };
  }

  if (!ownedFileBound && forbiddenSubstitute === "speculative_background_child") {
    return {
      decision: "standby",
      reason: "unbounded ownership cannot be satisfied by speculative background work",
    };
  }

  if (!hasFreshLiveCapacity) {
    return {
      decision: "standby",
      reason: stale ? "current capacity signal stale" : "current capacity signal contradictory or unverified",
    };
  }

  if (!hasMachineHeadroom) {
    return {
      decision: "standby",
      reason: "machine headroom unavailable",
    };
  }

  if (!ownedFileBound) {
    return {
      decision: "standby",
      reason: "owned-file bound absent",
    };
  }

  return {
    decision: "dispatch",
    reason: "fresh verified capacity and owned-file bound",
  };
}

const cases = [
  {
    lane: "j14",
    historicalPoolLabel: "available",
    currentCapacityRecord: null,
    forbiddenSubstitute: null,
    expectedDecision: "standby",
    expectedReason: "current capacity signal absent",
  },
  {
    lane: "j14",
    historicalPoolLabel: "available",
    currentCapacityRecord: {
      observedAt: "2026-07-30T09:12:00Z",
      freshForDecisionAt: "2026-07-30T09:12:20Z",
      activeSessionCount: 3,
      blockedOrSleepingSessions: ["j6"],
      accountQuotaStatus: "verified_enough_for_expected_turn",
      machineHeadroom: "verified_enough_for_focused_verification",
      ownedFileBound: true,
      contradictions: [],
    },
    forbiddenSubstitute: null,
    expectedDecision: "dispatch",
    expectedReason: "fresh verified capacity and owned-file bound",
  },
  {
    lane: "j14",
    historicalPoolLabel: "available",
    currentCapacityRecord: {
      observedAt: "2026-07-30T09:07:20Z",
      freshForDecisionAt: "2026-07-30T09:12:20Z",
      activeSessionCount: 2,
      blockedOrSleepingSessions: [],
      accountQuotaStatus: "verified_enough_for_expected_turn",
      machineHeadroom: "verified_enough_for_focused_verification",
      ownedFileBound: true,
      contradictions: [],
    },
    forbiddenSubstitute: null,
    expectedDecision: "dispatch",
    expectedReason: "fresh verified capacity and owned-file bound",
  },
  {
    lane: "j14",
    historicalPoolLabel: "available",
    currentCapacityRecord: {
      observedAt: "2026-07-30T07:00:00Z",
      freshForDecisionAt: "2026-07-30T09:12:20Z",
      activeSessionCount: 1,
      blockedOrSleepingSessions: [],
      accountQuotaStatus: "unverified",
      machineHeadroom: "verified_enough_for_focused_verification",
      ownedFileBound: true,
      contradictions: ["quota note expired before decision"],
    },
    forbiddenSubstitute: "borrow_other_account",
    expectedDecision: "refuse",
    expectedReason: "failed live capacity check cannot be satisfied by borrowing authority",
  },
  {
    lane: "j14",
    historicalPoolLabel: "available",
    currentCapacityRecord: {
      observedAt: "2026-07-30T09:12:05Z",
      freshForDecisionAt: "2026-07-30T09:12:20Z",
      activeSessionCount: 3,
      blockedOrSleepingSessions: ["j6"],
      accountQuotaStatus: "verified_enough_for_expected_turn",
      machineHeadroom: "insufficient_for_focused_verification",
      ownedFileBound: true,
      contradictions: [],
    },
    forbiddenSubstitute: "change_CODEX_HOME",
    expectedDecision: "refuse",
    expectedReason: "failed machine headroom check cannot be satisfied by changing CODEX_HOME",
  },
  {
    lane: "j14",
    historicalPoolLabel: "available",
    currentCapacityRecord: {
      observedAt: "2026-07-30T09:12:05Z",
      freshForDecisionAt: "2026-07-30T09:12:20Z",
      activeSessionCount: 3,
      blockedOrSleepingSessions: ["j6"],
      accountQuotaStatus: "verified_enough_for_expected_turn",
      machineHeadroom: "verified_enough_for_focused_verification",
      ownedFileBound: false,
      contradictions: [],
    },
    forbiddenSubstitute: "speculative_background_child",
    expectedDecision: "standby",
    expectedReason: "unbounded ownership cannot be satisfied by speculative background work",
  },
];

for (const fixture of cases) {
  const actual = routeCodexChild(fixture);
  assert.deepEqual(
    actual,
    {
      decision: fixture.expectedDecision,
      reason: fixture.expectedReason,
    },
    `${TEST_NAME} failed for ${fixture.lane}`,
  );
}

console.log(`PASS ${TEST_NAME}`);
```

### Fixed routing exercise summary

The coordinator records the following cases as the local contract for this plan:

| Case | Historical pool label | Fresh capacity record | Forbidden substitute attempted | Expected decision |
| --- | --- | --- | --- | --- |
| `stale-available-refuses` | `available` | absent or older than the current dispatch turn | none | `refuse` with `missing-current-capacity` |
| `contradictory-quota-stands-by` | `available` | active-session count conflicts with account/quota status | none | `standby` with `contradictory-capacity` |
| `fresh-owned-capacity-dispatches` | `available` | active sessions, blocked/sleeping sessions, verified account/quota status, machine headroom, and owned-file bound are all current and sufficient | none | `dispatch` |
| `failed-capacity-cannot-borrow` | any value | quota or headroom is insufficient | borrowed account, changed `CODEX_HOME`, or speculative background child | `refuse` with `capacity-substitution-forbidden` |

Inverting any expected decision above must fail the exercise. In particular, a stale `available`
label cannot dispatch, a fresh capacity record cannot be ignored when it is sufficient and file-bound,
and a borrowed account or changed `CODEX_HOME` cannot turn failed capacity into permission.

Concrete fixture for that exercise:

| Case | Historical pool label | Current capacity signal | Substitute attempted | Expected routing decision |
| --- | --- | --- | --- | --- |
| stale-pool | `available` | no fresh active-session count, quota/account state not verified | none | `standby`, reason `missing-current-capacity` |
| live-capacity | `available` | active sessions below cap, blocked/sleeping list checked, quota/account verified, machine headroom sufficient, unit still owned-file bound | none | `dispatch` |
| failed-capacity | `available` | quota or headroom check failed | borrow account, change `CODEX_HOME`, or start speculative child | `refuse`, reason `capacity-check-failed` |

The `live-capacity` case is the only fixture row that may dispatch. Inverting either refusal row to
`dispatch`, or accepting any substitute in `failed-capacity`, fails the exercise.

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

## Machine-checkable routing test

```json
{
  "schemaVersion": 1,
  "tests": [
    {
      "name": "Update routing decisions from real Codex capacity instead of stale pools.",
      "decisionRule": {
        "dispatchRequiresAllFacts": [
          "fresh_active_session_count",
          "blocked_or_sleeping_sessions_recorded",
          "verified_current_quota_status",
          "verified_current_account_status",
          "machine_headroom_sufficient",
          "owned_file_bound"
        ],
        "dispatchForbiddenWhenAnyFactAppears": [
          "capacity_signal_absent",
          "capacity_signal_stale",
          "capacity_signal_contradictory",
          "account_or_quota_unverified",
          "quota_check_failed",
          "machine_headroom_failed",
          "borrowed_account",
          "changed_codex_home",
          "speculative_background_work"
        ],
        "allowedRefusalDecisions": ["refuse", "standby"],
        "allowedDispatchDecision": "dispatch"
      },
      "scenarios": [
        {
          "name": "stale available pool label is not authority",
          "historicalPoolLabel": "available",
          "currentCapacityFacts": ["capacity_signal_stale"],
          "decision": "refuse",
          "reasonRecorded": true
        },
        {
          "name": "fresh verified capacity permits dispatch",
          "historicalPoolLabel": "available",
          "currentCapacityFacts": [
            "fresh_active_session_count",
            "blocked_or_sleeping_sessions_recorded",
            "verified_current_quota_status",
            "verified_current_account_status",
            "machine_headroom_sufficient",
            "owned_file_bound"
          ],
          "decision": "dispatch",
          "reasonRecorded": true
        },
        {
          "name": "substitutes cannot satisfy failed capacity",
          "historicalPoolLabel": "available",
          "currentCapacityFacts": [
            "quota_check_failed",
            "borrowed_account",
            "changed_codex_home",
            "speculative_background_work"
          ],
          "decision": "standby",
          "reasonRecorded": true
        }
      ]
    }
  ]
}
```
