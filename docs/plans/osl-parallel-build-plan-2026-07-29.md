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
