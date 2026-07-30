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

Machine-readable coordinator exercise:

```json
{
  "schema": "osl-codex-routing-capacity-v1",
  "cases": [
    {
      "name": "stale available pool is not authority",
      "historicalPoolLabel": "available",
      "currentCapacity": {
        "fresh": false,
        "activeSessionCount": null,
        "blockedOrSleepingSessions": null,
        "quotaAccountStatus": "unverified",
        "expectedTurnCapacity": "unknown",
        "machineHeadroom": "unknown",
        "ownedFileBound": false
      },
      "forbiddenSubstitute": "none",
      "expectedDecision": "standby",
      "expectedReason": "missing-current-capacity"
    },
    {
      "name": "fresh bounded capacity can dispatch",
      "historicalPoolLabel": "available",
      "currentCapacity": {
        "fresh": true,
        "activeSessionCount": 2,
        "blockedOrSleepingSessions": ["docs-waiting-review"],
        "quotaAccountStatus": "verified-enough",
        "expectedTurnCapacity": "enough",
        "machineHeadroom": "enough",
        "ownedFileBound": true
      },
      "forbiddenSubstitute": "none",
      "expectedDecision": "dispatch",
      "expectedReason": "fresh-capacity-and-owned-files"
    },
    {
      "name": "borrowed account cannot satisfy failed capacity",
      "historicalPoolLabel": "available",
      "currentCapacity": {
        "fresh": true,
        "activeSessionCount": 9,
        "blockedOrSleepingSessions": [],
        "quotaAccountStatus": "verified-exhausted",
        "expectedTurnCapacity": "insufficient",
        "machineHeadroom": "enough",
        "ownedFileBound": true
      },
      "forbiddenSubstitute": "borrowed-account",
      "expectedDecision": "refuse",
      "expectedReason": "forbidden-substitute"
    },
    {
      "name": "changed CODEX_HOME cannot satisfy failed capacity",
      "historicalPoolLabel": "green",
      "currentCapacity": {
        "fresh": true,
        "activeSessionCount": 8,
        "blockedOrSleepingSessions": ["quota-blocked"],
        "quotaAccountStatus": "verified-exhausted",
        "expectedTurnCapacity": "insufficient",
        "machineHeadroom": "enough",
        "ownedFileBound": true
      },
      "forbiddenSubstitute": "changed-CODEX_HOME",
      "expectedDecision": "refuse",
      "expectedReason": "forbidden-substitute"
    },
    {
      "name": "speculative background child cannot satisfy failed headroom",
      "historicalPoolLabel": "idle",
      "currentCapacity": {
        "fresh": true,
        "activeSessionCount": 4,
        "blockedOrSleepingSessions": [],
        "quotaAccountStatus": "verified-enough",
        "expectedTurnCapacity": "enough",
        "machineHeadroom": "insufficient",
        "ownedFileBound": true
      },
      "forbiddenSubstitute": "speculative-background-work",
      "expectedDecision": "refuse",
      "expectedReason": "forbidden-substitute"
    }
  ]
}
```
