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

## Test: Update routing decisions from real Codex capacity instead of stale pools.

This acceptance test is evaluated against the coordinator's routing decision, not against this
document's wording. Feed each fixture record to the routing policy and assert the decision class and
reason. A stale historical pool label is never a capacity grant.

```json
{
  "test": "Update routing decisions from real Codex capacity instead of stale pools.",
  "cases": [
    {
      "name": "stale available label refuses without current capacity",
      "input": {
        "historical_pool_label": "available",
        "current_capacity_record": null,
        "requested_work": { "owned_file_bound": true, "expected_turns": 1 },
        "substitute": null
      },
      "expected_decision": ["refuse", "standby"],
      "expected_reason": "current capacity signal absent"
    },
    {
      "name": "fresh verified capacity can dispatch",
      "input": {
        "historical_pool_label": "available",
        "current_capacity_record": {
          "recorded_at": "fresh",
          "active_session_count": 2,
          "blocked_or_sleeping_sessions": [],
          "account_quota_status": "verified_enough_for_expected_turn",
          "machine_headroom": "verified_enough_for_focused_verification",
          "contradictory": false
        },
        "requested_work": { "owned_file_bound": true, "expected_turns": 1 },
        "substitute": null
      },
      "expected_decision": ["dispatch"],
      "expected_reason": "fresh verified capacity and owned-file bound"
    },
    {
      "name": "contradictory live signal refuses despite available label",
      "input": {
        "historical_pool_label": "available",
        "current_capacity_record": {
          "recorded_at": "fresh",
          "active_session_count": 2,
          "blocked_or_sleeping_sessions": ["lane-a"],
          "account_quota_status": "verified_enough_for_expected_turn",
          "machine_headroom": "verified_enough_for_focused_verification",
          "contradictory": true
        },
        "requested_work": { "owned_file_bound": true, "expected_turns": 1 },
        "substitute": null
      },
      "expected_decision": ["refuse", "standby"],
      "expected_reason": "current capacity signal contradictory"
    },
    {
      "name": "failed quota cannot be repaired with forbidden substitute",
      "input": {
        "historical_pool_label": "available",
        "current_capacity_record": {
          "recorded_at": "fresh",
          "active_session_count": 2,
          "blocked_or_sleeping_sessions": [],
          "account_quota_status": "failed_quota_check",
          "machine_headroom": "verified_enough_for_focused_verification",
          "contradictory": false
        },
        "requested_work": { "owned_file_bound": true, "expected_turns": 1 },
        "substitute": "borrow_account"
      },
      "expected_decision": ["refuse", "standby"],
      "expected_reason": "failed quota check; forbidden substitute"
    },
    {
      "name": "unbounded file ownership refuses even with quota",
      "input": {
        "historical_pool_label": "available",
        "current_capacity_record": {
          "recorded_at": "fresh",
          "active_session_count": 1,
          "blocked_or_sleeping_sessions": [],
          "account_quota_status": "verified_enough_for_expected_turn",
          "machine_headroom": "verified_enough_for_focused_verification",
          "contradictory": false
        },
        "requested_work": { "owned_file_bound": false, "expected_turns": 1 },
        "substitute": null
      },
      "expected_decision": ["refuse", "standby"],
      "expected_reason": "requested work is not bounded to owned files"
    }
  ],
  "inversion_check": "If stale-label-only routing dispatches, if a borrowed account repairs failed quota, or if missing owned-file bounds dispatch, this test fails."
}
```
