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
