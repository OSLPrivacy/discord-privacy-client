# Reviewer findings report b15

Date: 2026-07-29

Scope: independent review of the ratchet lane remediation surface, with the follow-on owner for
`crates/ipc/src/commands.rs` expected to close the IPC command-surface item before sign-off.

## Findings

| finding_id | severity | status | owner_unit | source | required_remediation | rn_enabled |
| --- | --- | --- | --- | --- | --- | --- |
| b20_session_reset_symptom_deadlock | high | open | b20 | ratchet-lane-review | Honor authenticated fresh SESSION_RESET control messages without requiring a same-side local v4 decrypt failure, while keeping replay, staleness and honor-throttle refusal intact. | no |
| b20_recovery_result_observability | medium | open | b20 | ratchet-lane-review | Return an explicit applied-versus-ignored sentinel from SESSION_RESET handling so callers cannot mistake a refused recovery control message for user plaintext. | no |
| b36_requires_re_review_signoff | medium | pending_re_review | b36 | dependency-chain | Re-review the b20 command-surface remediation and record sign-off only after the named IPC acceptance test exists and exercises the closed behavior. | no |

Acceptance test: `reviewer_produces_findings_report`

The report is valid only if the Findings table preserves all three review findings with concrete
owners, open or pending-review status, remediation text that can drive follow-up work, and `rn_enabled`
set to `no` for every row. Removing a finding, changing a row to authorize RN, or weakening the
fail-closed binding language must make the test fail.

## Reviewer basis

The independent review found that a one-directional v4 desync can deadlock if the receiver of a
valid SESSION_RESET is required to have observed its own decrypt failure first. In that state the
peer that needs to reset may have no local symptom, while the other side is already asking for a
reset through an authenticated v2 control message. The remediation must therefore treat successful
v2 authentication plus freshness, replay-dedupe and honor-throttle checks as sufficient authority
to drop the local ratchet state.

This is not permission to loosen consent, binding or authority checks. A control message that
does not authenticate, is stale, replays a nonce, or trips the honor throttle must still be
ignored. Absence of a valid binding remains refusal.

## Non-goals

- Do not enable OSL-RN. `RN_WIRE_IN_ENABLED` remains false.
- Do not add a lowering operation for RN pins.
- Do not expose ratchets, keyservers or recovery internals as user-facing concepts.
