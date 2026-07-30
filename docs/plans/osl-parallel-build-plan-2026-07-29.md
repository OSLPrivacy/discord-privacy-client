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

## Behavioral acceptance fixture

Behavioral test:
`Update routing decisions from real Codex capacity instead of stale pools.`

The coordinator exercise passes only when the evaluator derives the route from
the current capacity record below. The historical pool label is included in each
case as adversarial history, not as authority.

| Case | Historical pool label | Current capacity record | Forbidden substitute attempted | Expected decision | Required recorded reason |
| --- | --- | --- | --- | --- | --- |
| stale-history-only | `available` | absent | none | `refuse` | `capacity_signal_absent` |
| stale-capacity-sample | `available` | active-session count exists but is stale; blocked/sleeping sessions unknown; quota/account state unverified | none | `standby` | `capacity_signal_stale_or_unverified` |
| fresh-owned-file-capacity | `available` | fresh active-session count, fresh blocked/sleeping-session list, verified current quota/account status, enough machine headroom, owned-file bound | none | `dispatch` | `fresh_capacity_verified` |
| failed-quota-borrowed-account | `available` | fresh active-session count and headroom, but quota check failed | borrowed account | `refuse` | `failed_capacity_check_no_substitution` |
| unbounded-file-scope | `available` | fresh active-session count, verified quota/account status, and enough headroom, but requested work is not bounded to the recipient's owned files | none | `refuse` | `owned_file_bound_missing` |

The inversion that must fail is any evaluator that dispatches from the
historical `available` label alone, treats an absent/stale/unverified capacity
record as permission, recovers a failed quota or headroom check by changing
accounts or `CODEX_HOME`, starts speculative background work, or dispatches work
that is not bound to the recipient's owned files.
