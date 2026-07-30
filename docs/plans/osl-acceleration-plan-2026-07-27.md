# OSL acceleration plan — 2026-07-27

Authority: `docs/design/osl-master-decision-2026-07-26.md` revision r17 and
`docs/design/osl-internal-build-checklist.md` at 100/303.

This plan optimizes for a finished, honestly proven product. It does not optimize for easy points,
commit count, number of busy tabs, or additional harness layers.

## Outcomes and priority

1. **Truthful Scrub demonstration**
   - Real picker → native per-profile grant → nonempty import → revoke → restart → persisted reread.
   - Review list reaches the native attended IMAP authority through registered, ACL-bound commands.
   - A seeded local IMAP item is deleted and independently verified absent; negative controls remain.
   - The already-frozen Windows bundle is audited and run once. Do not rebuild it unless rejected.

2. **Two-way private messaging**
   - Recovery produces the scheme-1 identity authority and exact `osl1_` identity.
   - Shipping client uses the signed scheme-1 prekey contract; server/client bytes and rollout match.
   - Hub/IPC preserves authenticated attribution, sequence, scope, downgrade, and Burn ordering.
   - The same exact candidate binary sends A→B and B→A on two isolated identities/VMs.
   - This first smoke proves two-way messaging only; full B6 lifecycle coverage remains separate.

3. **One honest release candidate**
   - Reconcile accepted feature leaves into one source-owner descendant lineage.
   - Run focused tests while editing and one batched full gate after the candidate freezes.
   - Test the exact candidate bytes in the runtime proofs; never rebuild after runtime evidence.
   - Hosted CI, signing, independent receipt audit, promotion, and rollback follow in that order.

## Dependency chains

### Scrub

`F1 consent/runtime → F2 account ownership → F3 review → IMAP command reachability → F4 verified
attended deletion → F10 exact-build capture`

Defer broader provider parsing, cloud AutoScrub, and new harness architecture unless they block this
chain. Use the seeded local IMAP fixture; do not imply live-provider deletion.

Behavioral receipt test to add before promoting F4:
`seeded_windows_imap_receipt_package_positive_and_negative`.

Pass condition: the receipt package names the exact immutable Windows executable, run id, fixture
mailbox, reviewed manifest, positive seeded message locator, and at least one negative-control
locator. It must show the attended IMAP authority deleting the seeded positive, then independently
re-reading the mailbox and finding the positive absent while every negative-control message remains
present.

Fail condition: the package is rejected if deletion is inferred from a UI transition, lacks the
post-delete IMAP re-read, has no retained negative control, is not tied to the exact executable/run,
or describes live-provider/account-wide deletion beyond the seeded fixture.

Structured receipt-package contract:

```json
{
  "schemaVersion": 1,
  "test": "seeded_windows_imap_receipt_package_positive_and_negative",
  "requiredEvidence": [
    "immutable_windows_executable",
    "run_id",
    "fixture_mailbox",
    "reviewed_manifest",
    "positive_seeded_message_locator",
    "negative_control_locator",
    "attended_imap_authority",
    "post_delete_imap_reread"
  ],
  "positiveRequirement": {
    "seededMessageAbsentAfterReread": true,
    "deletionAuthority": "attended_imap_authority"
  },
  "negativeRequirement": {
    "minimumRetainedNegativeControls": 1,
    "negativeControlsRemainPresent": true
  },
  "rejectedIf": [
    "ui_transition_only",
    "missing_post_delete_imap_reread",
    "missing_negative_control",
    "not_bound_to_exact_executable_or_run",
    "claims_live_provider_or_account_wide_deletion"
  ],
  "status": "receipt-contract-defined"
}
```

### Messaging

`A2 recovery-derived full-bundle identity → scheme-1 prekey client/server → A3 attribution and A4
registration → shipping Hub/IPC path → two-way smoke → full B6 → independent B7 review`

Do not deploy until the client path exists and the exact source/migration/Worker sequence is reviewed.
Do not claim ratcheting, forward secrecy, or peer-copy deletion from the current v3/Burn work.

### Release

`frozen accepted leaves → clean source-owner integration → local focused gates → one batched full
gate → exact Windows artifact → Scrub and two-way runtime receipts → hosted CI → signed draft RC →
independent audit → protected promotion/rollback rehearsal`

## Fleet structure

Only three lanes implement during a wave:

- `scrub_ingest`: Scrub critical-path implementation.
- `crypto_hub`: Hub/provider lifecycle and shared native integration.
- `crypto_keystore`: identity/prekey authority.

Independent auditors consume frozen owner packets:

- `audit_truth`: Hub/profile lifecycle and final truth/status.
- `store_security`: transport/Burn.
- `cipher_store`: Store migration.
- `scrub_consent`: release claim gate.
- `audit_crypto`: identity/prekey.

Other roles:

- `release`: sole integration ledger and final batched-gate owner.
- `keyserver`: fix-only standby after freezing its candidate.
- `crypto_transport`: fix-only standby after freezing its candidate.

An implementation owner stops editing after publishing a candidate. A rejected candidate returns only
to its owner for the exact rejected conditions; it does not trigger general cleanup.

## Evidence packet

Every frozen owner candidate publishes one packet:

- full commit, tree, parent, and exact changed paths;
- focused commands and exact outcomes;
- the failure-capable control observed before or against the fix;
- highest honest status tier and explicit unknowns;
- one downstream integration dependency.

Auditors consume this packet and `git show`. They do not repeat whole-tree inventories or full suites.
They may run at most one focused spot check when a claimed invariant lacks evidence.

## Test and resource policy

- Cargo runs only through `osl-cargo`; broad non-Cargo work only through `osl-heavy`.
- During implementation, use focused tests only.
- Release owns one batched full gate after all leaves freeze and receive dispositions.
- Never hold the heavy lock for an exhaustive suite while unrelated iteration still needs it.
- Never nest either fleet lock.

Resource states:

- **Green:** load1 < 8 and at least 10 GiB available: at most two fast children globally, one per
  lane, for truly disjoint mechanical work.
- **Yellow:** load1 8–12 or 6–10 GiB available: no new children or broad jobs; focused tests only.
- **Red:** load1 ≥ 12, available memory < 6 GiB, or rapidly growing swap: start nothing new; let the
  exact active command finish, checkpoint, and wait.

Fast children handle boilerplate, fixture generation, bounded searches, mechanical inventories, and
repetitive mutation execution. Route that work only through `osl-fast-delegate`, and only while the
resource state is Green. Main Sol/Terra tabs retain architecture, cryptography, destructive behavior,
conflict resolution, acceptance decisions, and final review. Broad or heavy verification remains
behind `osl-heavy`; Cargo verification remains batched through `osl-cargo` under the release owner.
Never switch Codex accounts or change `CODEX_HOME` unless Liam explicitly changes that instruction.
The guarded launcher inherits an already-selected `CODEX_HOME` unchanged; it never clears, replaces,
or synthesizes one.

Use the guarded launcher instead of invoking nested Codex directly:

```text
osl-fast-delegate -C <owned-worktree> "<bounded task prompt>"
```

Every child prompt must state: exact owned files; forbidden files/actions; one measurable
deliverable; focused verification; no Git writes/deploys/account changes; and the result format.
The child may edit only its exclusive files. The parent must inspect the final diff, verify nothing
outside the owned set changed, and rerun the smallest meaningful check before accepting it. A
zero-byte, truncated, or missing result is failure—not “no findings.”

Document-owned acceptance controls:

- **Keep Codex account and heavy-resource routing explicit.** Pass only when the plan names the
  allowed account behavior, preserves an inherited `CODEX_HOME`, routes Cargo and broad verification
  through the guarded lanes, and requires child prompts to forbid account changes. It fails if a
  delegate can clear, synthesize, switch, or infer a Codex account, or if heavyweight work can bypass
  the named guarded routes.
- **Pin mirror sessions by identifier before prompting or resuming them.** Pass only when every
  mirror session is inspected and selected by concrete session ID before any prompt or resume action.
  It fails if a title, stale screen text, or unverified background-terminal report is enough to route
  work into a mirror session.

## Idle and dead-lane handling

- A completed result plus an empty prompt is immediately routed to audit, integration, or fix-only
  standby.
- A reported background terminal is checked by PID; text alone is not proof of activity.
- An unchanged screen for ten minutes with no attributable process is dead and must be reprompted.
- Repeated sleep/poll loops with no branch or file movement are coordination spin; stop polling and
  enter explicit standby.
- Every mirror session is inspected and pinned by concrete session ID immediately before prompting
  or resuming it. Never route by title, stale screen text, or an unverified background-terminal
  report.

## Runtime discipline

- Stop adding F1 harness layers. Audit and run the frozen bundle.
- Use two VMs for messaging, the same candidate executable on both, distinct OSL identities/profile
  roots, fresh run IDs, and no stale artifacts.
- Keep cold and warm VM lineages separate. Credentials are just-in-time and never baked into images.
- Codex prepares artifacts and exact commands; Liam performs installations, VM actions, launches,
  and manual tests unless he explicitly authorizes a specific exception.
- The tested executable is immutable after runtime evidence. Any rebuild invalidates the receipt.

## Current wave completion conditions

The current wave ends when all of these exist:

1. Scrub IMAP command reachability is frozen and independently audited.
2. Hub profile lifecycle is frozen and independently audited.
3. Scheme-1 identity authority is frozen and independently audited.
4. Burn apply-before-acknowledgement and Store anchored migration each have independent verdicts.
5. Release records one coherent integration order and final batched-gate command set.
6. The F1 bundle receives an independent verdict and, if accepted, an exact operator command for the
   one real Windows run.

Then create one clean integration candidate. Do not start another source-expansion wave first.
