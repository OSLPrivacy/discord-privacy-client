# OSL-RN default-on go/no-go dossier

**Decision requested:** should wire type `0x10` carry all real pairwise direct-message traffic by default?

**Current recommendation: NO-GO for default-on.** This is an evidence dossier, not an activation
approval. The runtime gate remains closed until the owner has separately accepted the remaining
risks and the missing live-pair evidence exists.

## Evidence register

| Question | Evidence | Result | Decision meaning |
|---|---|---|---|
| Is the historical desync reproducible and recoverable? | T19-G5, `vmqa/rn_desync.ps1` | **Not evidenced.** The required two-client injection script and its live run record are absent. | **Red.** A detector test is not a reproduction of the historical failure. Do not infer one. |
| Does it survive restart? | T19-G3 | Automated RN persistence/restart coverage exists in the ratchet suite. | Green only for the tested persistence path; it does not close the live desync row. |
| Does it survive offline? | T19-G4 | Queued-delivery behavior has automated coverage. | Green only for the tested queued path. |
| Can concurrent writers reuse a body nonce? | T19-A5, T19-A6 | The RN session store uses exclusive creation and tests cover concurrency/rollback. | Green for the tested nonce and persistence invariants. |
| Can a mixed-version pair silently lose a message? | T19-E5, T19-B5 | Selection is capability-gated and a pinned peer refuses a legacy downgrade. | Green for dispatcher/selection coverage; mixed live-client evidence remains valuable. |
| Can a pinned peer become permanently stuck? | T19-E3 and OQ-1 | **Yes, potentially.** The pin deliberately has no lowering operation; the owner has not chosen the escape hatch. | **Red.** This is an unresolved owner decision, not a passing security property. |
| Does the ratchet impose an availability cost? | T19-D3 | **Yes.** The skipped-key ceiling can refuse sufficiently delayed messages; legacy v3 did not have this ratchet-specific bound. | **Yellow.** Disclosed regression, not a reason to claim transparent delivery. |
| Is public wording constrained to proven limits? | T19-H1, `docs/design/osl-public-claim-allowlist.md` A16 | The allowlist includes forward secrecy, recovery limits, skipped-key availability loss, and lack of review. | Green for wording only; wording cannot substitute for evidence. |
| Has an external cryptographic review happened? | `crates/osl-ratchet-next/DESIGN.md` §10; claim allowlist A16 | **No.** There has been no formal analysis or external cryptographic review. | **Red, permanently, until an independent review is completed.** |

## Required live desync proof (still missing)

T19-G5 is the decision-critical proof. On a real two-client pair it must independently inject all
three historical failure classes: rotate B's key, leave a TOFU change unaccepted, and kill A between
seal and persistence. For each injection, evidence must show:

1. detection and durable user-visible desync state;
2. automatic reset/re-handshake and healing;
3. no message loss; and
4. a v3 control pair is unaffected.

The negative control is mandatory: with the detector disabled, each injection must reproduce a
silent permanent failure. Without that red run, a green test could merely be exercising an unrelated
failure path. No such run record is available with this dossier.

## Security and product limits that remain true

- OSL-RN is pairwise direct-message encryption only. It does not deliver post-quantum
  authentication, groups, multi-device synchronization, sealed-sender delivery, or authenticated
  prekey bundles.
- Classical post-compromise recovery takes one round trip. Post-quantum recovery takes roughly 82
  round trips and requires bidirectional traffic.
- Recovery and reset messages lack forward secrecy.
- The skipped-key ceiling is an availability regression: a delayed message can be unavailable rather
  than silently treated as delivered.
- A pinned peer cannot silently fall back to v3, but the current no-lowering design creates the
  unresolved same-identity recovery problem in OQ-1.

## Conditional staging proposal — not approval

After every evidence row above is independently re-run, and only after the owner resolves OQ-1,
the narrowest eligible rollout is `RnPolicy::Opportunistic` for **new conversations only** with
identities advertising RN capability bit 1. Report desync counters for two weeks before considering
existing conversations. Do not change the runtime gate or default existing conversations based on
this report.

## Owner decision

**Do not enable default-on traffic today.** Reopen this decision only when T19-G5's live injection
and sabotage evidence is attached, OQ-1 has an explicit owner decision, and the lack of external
cryptographic review is either remedied or explicitly accepted as a release risk.
