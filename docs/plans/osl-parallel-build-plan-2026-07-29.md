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

### Update routing decisions from real Codex capacity instead of stale pools.

This is the machine-readable coordinator exercise for `risk-codex-quota`. A runner or reviewer
passes the test only by evaluating the current-capacity inputs below, not by trusting the historical
pool label.

```json
{
  "test_name": "Update routing decisions from real Codex capacity instead of stale pools.",
  "decision_rule": {
    "dispatch_requires": [
      "fresh_capacity_signal",
      "active_session_count",
      "blocked_or_sleeping_session_list",
      "verified_account_and_quota_status",
      "enough_machine_headroom",
      "owned_file_bound"
    ],
    "refusal_triggers": [
      "absent_capacity_signal",
      "stale_capacity_signal",
      "contradictory_capacity_signal",
      "unverified_account_or_quota_state",
      "failed_quota_check",
      "failed_headroom_check",
      "missing_owned_file_bound",
      "borrowed_account",
      "changed_CODEX_HOME",
      "speculative_background_child"
    ]
  },
  "cases": [
    {
      "name": "stale_available_pool_refuses",
      "historical_pool_label": "available",
      "current_capacity_record": {
        "fresh": false,
        "active_session_count": null,
        "blocked_or_sleeping_sessions": null,
        "account_and_quota_status": "unverified",
        "machine_headroom": "unknown",
        "owned_file_bound": true
      },
      "forbidden_substitute": null,
      "allowed_decisions": ["refuse", "standby"],
      "forbidden_decisions": ["dispatch"],
      "reason_required": true
    },
    {
      "name": "fresh_owned_capacity_may_dispatch",
      "historical_pool_label": "available",
      "current_capacity_record": {
        "fresh": true,
        "active_session_count": 2,
        "blocked_or_sleeping_sessions": [],
        "account_and_quota_status": "verified_enough_for_expected_turn",
        "machine_headroom": "verified_enough_for_focused_verification",
        "owned_file_bound": true
      },
      "forbidden_substitute": null,
      "allowed_decisions": ["dispatch"],
      "forbidden_decisions": ["refuse_without_reason", "standby_without_reason"],
      "reason_required": false
    },
    {
      "name": "failed_check_cannot_be_satisfied_by_borrowing",
      "historical_pool_label": "available",
      "current_capacity_record": {
        "fresh": true,
        "active_session_count": 7,
        "blocked_or_sleeping_sessions": ["lane-b"],
        "account_and_quota_status": "verified_quota_exhausted",
        "machine_headroom": "verified_enough_for_focused_verification",
        "owned_file_bound": true
      },
      "forbidden_substitute": "borrowed_account",
      "allowed_decisions": ["refuse", "standby"],
      "forbidden_decisions": ["dispatch"],
      "reason_required": true
    },
    {
      "name": "fresh_capacity_without_owned_file_bound_refuses",
      "historical_pool_label": "available",
      "current_capacity_record": {
        "fresh": true,
        "active_session_count": 1,
        "blocked_or_sleeping_sessions": [],
        "account_and_quota_status": "verified_enough_for_expected_turn",
        "machine_headroom": "verified_enough_for_focused_verification",
        "owned_file_bound": false
      },
      "forbidden_substitute": null,
      "allowed_decisions": ["refuse", "standby"],
      "forbidden_decisions": ["dispatch"],
      "reason_required": true
    }
  ],
  "mutation_checks": [
    {
      "from_case": "fresh_owned_capacity_may_dispatch",
      "mutation": "set current_capacity_record.fresh to false",
      "must_forbid": "dispatch"
    },
    {
      "from_case": "fresh_owned_capacity_may_dispatch",
      "mutation": "set current_capacity_record.account_and_quota_status to unverified",
      "must_forbid": "dispatch"
    },
    {
      "from_case": "fresh_owned_capacity_may_dispatch",
      "mutation": "set forbidden_substitute to changed_CODEX_HOME",
      "must_forbid": "dispatch"
    }
  ]
}
```
