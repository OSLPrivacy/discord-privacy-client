# T2-84 — adversarial audit of composition rules

**Audit date:** 2026-08-01  
**Disposition:** **not ready to certify**. This is a prerequisite failure, not a
finding that the required interactions are correct.

## Evidence checked

The Group G premise is stale in this checkout. The planned composition-test
sources do not exist, and there are no `TF-70` through `TF-79` references in
the repository. Consequently none of the scenarios below has the required
failure-injection evidence. A unit test of an isolated helper is not a
substitute: each scenario exists to prove behaviour across the relevant
feature boundaries.

| Scenario | Normative result | Required evidence | Audit result |
|---|---|---|---|
| T2-70 / §8.1: burn after view-once opened | Burn succeeds; instruction is sent; peer acknowledges `already_absent`; display reason is Burn. | `compose_burn_after_viewonce.rs` with the consumed-object burn failure injection. | Missing |
| T2-71 / §8.2: expiry during receipt | Record the late receipt, without restarting time or restoring content. | `compose_expiry_receipt_race.rs` with payload-less receipts dropped. | Missing |
| T2-72 / §8.7: failed eager download | Preserve the reservation semantics: retry before commit, resume in-window, and report Gone after the window. | `compose_failed_download.rs` with first-byte consumption. | Missing |
| T2-73 / §8.5: attachment + view-once + burn | Wipe partial data and render nothing when burn interrupts transfer. | `compose_attachment_viewonce_burn.rs` with partial rendering. | Missing |
| T2-74 / §8.6: server already gone | `NotFound` removes only an unmaterialized copy; a held copy remains. | `compose_notfound_is_instruction.rs` with unconditional `NotFound` destruction. | Missing |
| T2-75 / §8.4: old peer | Unknown destructive reasons fail closed; unknown non-destructive types are ignored. | `compose_old_peer.rs` with unknown destructive reason made a no-op. | Missing |
| T2-76 / R2: display reason | Devices receiving the same events in different orders display the same reason. | `compose_reason_precedence.rs` with arrival-order selection. | Missing |
| T2-77 / §4.4: multi-device acknowledgements | A partial acknowledgement never reports completion and another device can still read. | `compose_multidevice_acks.rs` with boolean roll-up. | Missing |
| T2-79 / §8.11, D28: offline burn | Local copy is gone immediately; server and peer effects stay pending until confirmed; reconnect drain is idempotent. | `compose_offline_burn.rs` with completion marked on enqueue. | Missing |

## Owner-decision checks

The audit applies the later binding decisions, rather than treating the earlier
track wording as authoritative:

- D12 requires both server enforcement and client instruction for destructive
  features.
- D28 makes offline burn three independently observable effects and forbids a
  completed state before server confirmation.
- D29 requires eager fetch, so the failed-download scenario is the ordinary
  delivery path rather than an edge case.
- D30 requires offline view-once enforcement for an already-held payload and
  honest refusal for server-enforced effects while offline.
- X3 and §8.10 mean any passing client-side burn scenario would still be
  insufficient evidence for a server-enforced-burn claim until that defect is
  repaired.

## Re-audit gate

Do not mark this audit certified until every named composition test exists,
runs green, and has been observed red using its listed failure injection. The
next auditor should also reconcile the stale phrase “seven interactions” with
the nine non-VM Group G scenarios now listed (T2-70–77 and T2-79); T2-78 is a
separate real-Windows capture test.
