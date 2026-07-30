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

Executable acceptance test:

```js
import assert from "node:assert/strict";
import test from "node:test";

const FRESH_CAPACITY_MAX_AGE_MS = 30_000;
const FORBIDDEN_SUBSTITUTES = new Set([
  "borrowed_account",
  "changed_codex_home",
  "speculative_background_child",
]);

function routeCodexLane({
  nowMs,
  expectedTurnSessions,
  historicalPoolLabel,
  capacityRecord,
  substitute,
}) {
  const reasons = [];

  if (FORBIDDEN_SUBSTITUTES.has(substitute)) {
    reasons.push(`forbidden_substitute:${substitute}`);
  }
  if (!capacityRecord) {
    reasons.push("missing_current_capacity_record");
  } else {
    if (nowMs - capacityRecord.observedAtMs > FRESH_CAPACITY_MAX_AGE_MS) {
      reasons.push("stale_current_capacity_record");
    }
    if (capacityRecord.accountStatus !== "verified_current") {
      reasons.push("unverified_account_status");
    }
    if (capacityRecord.quotaStatus !== "verified_available") {
      reasons.push("failed_quota_check");
    }
    if (capacityRecord.machineHeadroom !== "enough") {
      reasons.push("failed_headroom_check");
    }
    if (capacityRecord.ownedFileBound !== true) {
      reasons.push("missing_owned_file_bound");
    }
    if (
      !Number.isInteger(capacityRecord.activeSessionCount) ||
      !Array.isArray(capacityRecord.blockedOrSleepingSessions) ||
      !Number.isInteger(capacityRecord.remainingTurnSessions) ||
      capacityRecord.remainingTurnSessions < expectedTurnSessions
    ) {
      reasons.push("insufficient_live_capacity_facts");
    }
  }

  return {
    decision: reasons.length === 0 ? "dispatch" : historicalPoolLabel === "available" ? "standby" : "refuse",
    reasons,
  };
}

test("Update routing decisions from real Codex capacity instead of stale pools.", () => {
  const nowMs = Date.parse("2026-07-29T17:00:00Z");
  const freshVerifiedCapacity = {
    observedAtMs: nowMs - 1_000,
    activeSessionCount: 3,
    blockedOrSleepingSessions: ["unit-a7"],
    accountStatus: "verified_current",
    quotaStatus: "verified_available",
    machineHeadroom: "enough",
    ownedFileBound: true,
    remainingTurnSessions: 2,
  };

  assert.equal(
    routeCodexLane({
      nowMs,
      expectedTurnSessions: 1,
      historicalPoolLabel: "available",
      capacityRecord: null,
    }).decision,
    "standby",
  );

  assert.deepEqual(
    routeCodexLane({
      nowMs,
      expectedTurnSessions: 1,
      historicalPoolLabel: "available",
      capacityRecord: {
        ...freshVerifiedCapacity,
        observedAtMs: nowMs - 90_000,
        accountStatus: "unverified",
      },
    }),
    {
      decision: "standby",
      reasons: ["stale_current_capacity_record", "unverified_account_status"],
    },
  );

  assert.deepEqual(
    routeCodexLane({
      nowMs,
      expectedTurnSessions: 2,
      historicalPoolLabel: "available",
      capacityRecord: freshVerifiedCapacity,
    }),
    {
      decision: "dispatch",
      reasons: [],
    },
  );

  const borrowedAfterQuotaFailure = routeCodexLane({
    nowMs,
    expectedTurnSessions: 2,
    historicalPoolLabel: "available",
    substitute: "borrowed_account",
    capacityRecord: {
      ...freshVerifiedCapacity,
      quotaStatus: "exhausted",
      remainingTurnSessions: 0,
    },
  });
  assert.equal(borrowedAfterQuotaFailure.decision, "standby");
  assert.deepEqual(borrowedAfterQuotaFailure.reasons, [
    "forbidden_substitute:borrowed_account",
    "failed_quota_check",
    "insufficient_live_capacity_facts",
  ]);

  for (const substitute of ["changed_codex_home", "speculative_background_child"]) {
    const result = routeCodexLane({
      nowMs,
      expectedTurnSessions: 1,
      historicalPoolLabel: "green",
      substitute,
      capacityRecord: freshVerifiedCapacity,
    });
    assert.equal(result.decision, "refuse");
    assert.deepEqual(result.reasons, [`forbidden_substitute:${substitute}`]);
  }
});
```
