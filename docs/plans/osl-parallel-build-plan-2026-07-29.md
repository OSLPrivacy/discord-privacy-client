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

## Coordinator routing exercise

Test name: Update routing decisions from real Codex capacity instead of stale pools.

Decision rule:

1. Treat the historical pool label as non-authoritative input.
2. Refuse or place the lane on standby unless the current capacity record is fresh, internally
   consistent, tied to a verified account/quota state, bounded to the recipient's owned files, and
   shows enough Codex quota plus machine headroom for the expected turn.
3. Refuse or place the lane on standby when the route attempts to satisfy a failed check by borrowing
   another account, changing `CODEX_HOME`, or starting speculative background work.

Machine-readable exercise:

```json
{
  "test_name": "Update routing decisions from real Codex capacity instead of stale pools.",
  "decision_contract": {
    "dispatch_requires": [
      "fresh_capacity_record",
      "consistent_capacity_record",
      "verified_account_quota",
      "owned_file_bound",
      "enough_codex_quota",
      "enough_machine_headroom"
    ],
    "forbidden_substitutes": [
      "borrow_account",
      "change_CODEX_HOME",
      "speculative_background_child"
    ],
    "historical_pool_label_authority": false
  },
  "cases": [
    {
      "name": "stale available label without current signal",
      "historical_pool_label": "available",
      "current_capacity_record": null,
      "attempted_substitute": null,
      "expected_decisions": ["refuse", "standby"],
      "required_reason": "missing current capacity record"
    },
    {
      "name": "fresh verified capacity permits dispatch",
      "historical_pool_label": "available",
      "current_capacity_record": {
        "fresh": true,
        "consistent": true,
        "active_sessions": 2,
        "blocked_or_sleeping_sessions": [],
        "account_quota_verified": true,
        "owned_file_bound": true,
        "enough_codex_quota": true,
        "enough_machine_headroom": true
      },
      "attempted_substitute": null,
      "expected_decisions": ["dispatch"],
      "required_reason": "fresh verified capacity"
    },
    {
      "name": "failed quota cannot be repaired by account borrowing",
      "historical_pool_label": "available",
      "current_capacity_record": {
        "fresh": true,
        "consistent": true,
        "active_sessions": 4,
        "blocked_or_sleeping_sessions": ["lane-3"],
        "account_quota_verified": true,
        "owned_file_bound": true,
        "enough_codex_quota": false,
        "enough_machine_headroom": true
      },
      "attempted_substitute": "borrow_account",
      "expected_decisions": ["refuse", "standby"],
      "required_reason": "failed quota check"
    },
    {
      "name": "unverified account state refuses despite idle label",
      "historical_pool_label": "idle",
      "current_capacity_record": {
        "fresh": true,
        "consistent": true,
        "active_sessions": 0,
        "blocked_or_sleeping_sessions": [],
        "account_quota_verified": false,
        "owned_file_bound": true,
        "enough_codex_quota": true,
        "enough_machine_headroom": true
      },
      "attempted_substitute": null,
      "expected_decisions": ["refuse", "standby"],
      "required_reason": "unverified account/quota state"
    },
    {
      "name": "failed headroom cannot start speculative child",
      "historical_pool_label": "available",
      "current_capacity_record": {
        "fresh": true,
        "consistent": true,
        "active_sessions": 1,
        "blocked_or_sleeping_sessions": [],
        "account_quota_verified": true,
        "owned_file_bound": true,
        "enough_codex_quota": true,
        "enough_machine_headroom": false
      },
      "attempted_substitute": "speculative_background_child",
      "expected_decisions": ["refuse", "standby"],
      "required_reason": "failed machine headroom check"
    }
  ]
}
```
