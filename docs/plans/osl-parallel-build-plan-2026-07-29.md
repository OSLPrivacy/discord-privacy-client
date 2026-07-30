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

### Machine-checkable routing exercise

The following `node:test` block is the executable acceptance test for `risk-codex-quota`. It is kept
in this plan because the behavior is a coordinator routing contract, not app runtime code.

```js
import assert from "node:assert/strict";
import test from "node:test";

const REQUIRED_LIVE_FIELDS = [
  "activeSessionCount",
  "blockedOrSleepingSessions",
  "verifiedAccountQuota",
  "machineHeadroom",
  "ownedFileBound",
];
const FORBIDDEN_SUBSTITUTES = new Set([
  "borrow_account",
  "change_CODEX_HOME",
  "speculative_background_child",
]);

function decideCodexRoute(lane) {
  const capacity = lane.currentCapacity;
  if (!capacity || typeof capacity !== "object") {
    return { decision: "standby", reason: "missing_current_capacity_signal" };
  }
  if (capacity.fresh !== true) {
    return { decision: "standby", reason: "stale_current_capacity_signal" };
  }
  if (capacity.contradictory === true) {
    return { decision: "refuse", reason: "contradictory_current_capacity_signal" };
  }
  for (const field of REQUIRED_LIVE_FIELDS) {
    if (capacity[field] === undefined) {
      return { decision: "standby", reason: `missing_${field}` };
    }
  }
  if (capacity.verifiedAccountQuota !== true) {
    return { decision: "refuse", reason: "unverified_account_quota" };
  }
  if (capacity.machineHeadroom !== "enough") {
    return { decision: "standby", reason: "insufficient_machine_headroom" };
  }
  if (capacity.ownedFileBound !== true) {
    return { decision: "refuse", reason: "unowned_or_unbounded_files" };
  }
  if (capacity.expectedTurnFitsQuota !== true) {
    const substitute = lane.fallbackAttempt;
    if (FORBIDDEN_SUBSTITUTES.has(substitute)) {
      return { decision: "refuse", reason: `forbidden_substitute:${substitute}` };
    }
    return { decision: "standby", reason: "insufficient_codex_quota" };
  }
  return { decision: "dispatch", reason: "fresh_capacity_verified" };
}

test("Update routing decisions from real Codex capacity instead of stale pools.", () => {
  const freshEnough = {
    fresh: true,
    contradictory: false,
    activeSessionCount: 2,
    blockedOrSleepingSessions: ["lane-4"],
    verifiedAccountQuota: true,
    machineHeadroom: "enough",
    ownedFileBound: true,
    expectedTurnFitsQuota: true,
  };

  assert.equal(
    decideCodexRoute({ historicalPoolLabel: "available", currentCapacity: null }).decision,
    "standby",
  );
  assert.equal(
    decideCodexRoute({
      historicalPoolLabel: "available",
      currentCapacity: { ...freshEnough, fresh: false },
    }).decision,
    "standby",
  );
  assert.equal(
    decideCodexRoute({
      historicalPoolLabel: "available",
      currentCapacity: { ...freshEnough, contradictory: true },
    }).decision,
    "refuse",
  );
  assert.equal(
    decideCodexRoute({
      historicalPoolLabel: "available",
      currentCapacity: { ...freshEnough, verifiedAccountQuota: false },
    }).decision,
    "refuse",
  );
  assert.deepEqual(
    decideCodexRoute({ historicalPoolLabel: "available", currentCapacity: freshEnough }),
    { decision: "dispatch", reason: "fresh_capacity_verified" },
  );
  for (const fallbackAttempt of FORBIDDEN_SUBSTITUTES) {
    assert.equal(
      decideCodexRoute({
        historicalPoolLabel: "available",
        fallbackAttempt,
        currentCapacity: { ...freshEnough, expectedTurnFitsQuota: false },
      }).decision,
      "refuse",
    );
  }
});
```
