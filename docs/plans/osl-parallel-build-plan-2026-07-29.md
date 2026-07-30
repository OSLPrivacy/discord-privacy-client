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

### Fixed routing exercise

The coordinator records the following cases as the local contract for this plan:

| Case | Historical pool label | Fresh capacity record | Forbidden substitute attempted | Expected decision |
| --- | --- | --- | --- | --- |
| `stale-available-refuses` | `available` | absent or older than the current dispatch turn | none | `refuse` with `missing-current-capacity` |
| `contradictory-quota-stands-by` | `available` | active-session count conflicts with account/quota status | none | `standby` with `contradictory-capacity` |
| `fresh-owned-capacity-dispatches` | `available` | active sessions, blocked/sleeping sessions, verified account/quota status, machine headroom, and owned-file bound are all current and sufficient | none | `dispatch` |
| `failed-capacity-cannot-borrow` | any value | quota or headroom is insufficient | borrowed account, changed `CODEX_HOME`, or speculative background child | `refuse` with `capacity-substitution-forbidden` |

Inverting any expected decision above must fail the exercise. In particular, a stale `available`
label cannot dispatch, a fresh capacity record cannot be ignored when it is sufficient and file-bound,
and a borrowed account or changed `CODEX_HOME` cannot turn failed capacity into permission.
