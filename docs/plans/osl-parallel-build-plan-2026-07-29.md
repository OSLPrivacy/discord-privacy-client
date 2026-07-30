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

Machine-checkable routing test:

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
