# OSL internal company build checklist

> This is the simple internal project view and the intended source for Telegram `/osl` milestone
> updates. Product authority remains
> [`osl-master-decision-2026-07-26.md`](osl-master-decision-2026-07-26.md). The layman product view is
> [`osl-simple-spec.md`](osl-simple-spec.md).

## How to read it

- ✅ = verified on the required real boundary.
- 🟨 = partly built/proved; important work remains.
- 🧪 = implemented but only test-proven or not wired into the live product.
- 🛑 = blocked by security, a dependency, or an external app.
- ⬜ = designed/not started.
- ♻️ = evidence is changing or conflicting; recheck the exact current build.

“Tests pass” is not automatically ✅. Bilateral features need two identities. Visual/native-app
features need the exact Windows/app build and screenshots.

Planning weights total 303 acceptance points (300 + 3 added 2026-07-26 for J7). The current numbers are a **provisional baseline**, not
a promise or percent-complete theater. Agents replace estimates with decomposed, evidenced points as
they qualify work. Scope additions increase the denominator; regressions remove earned points.

## Active deadline

**2026-08-02, America/Los_Angeles**

- **Hard:** working/video-demonstrable Scrub vertical slice for Zhao to show his father.
- **Parallel target:** gated two-identity proof of the stronger encryption path.
- Safety and claim truth do not get relaxed for the date. If ETA slips, `/osl` warns early and
  proposes the smallest honest demo cut while preserving the full backlog.
- Deadline acceptance and update rules live in master section 2.1. Future deadlines must be stored
  with absolute dates in every AI account's compact OSL memory card.

## Progress snapshot

```text
Provisional verified progress: 100 / 303 points = 33%   (was 85 / 303 = 28%, which understated section H by 2)
Confidence: low (several dirty concurrent worktrees and exact-build rechecks remain)
Critical path:
security identity/attribution → reviewed ratchet → two-identity proof
→ finish Discord adapter contract → qualify every offered app → integration/release

Parallel paths:
Scrub/AutoScrub · website truth/demo · Notes minimum · test/mirror infrastructure

Resolved external blocker: keyserver migration 0027 is deployed; B5 remains partial because the
client-side prekey/wrapped-key production path is not established.
```

**2026-07-27 stale-spec correction pass (+0; score unchanged).**

- **F1 ownership:** exact `7bbf2e9e966634e7d434ec230b75a7f666db0003` exits `78` at B6 before
  reaching the F1 UI. The first owner is Hub for an F1-only startup-reachability successor. VM
  provisioning and live capture are downstream and cannot cure this product exit.
- **Scheme 1:** exact server/admission candidate
  `e273436dfaf735daab44adbbeb205a3c95ecd4cc` plus keystore construction lineage
  `a1b82a008f53d4864459d353bb6a89fc8753a446` supersede the old assertion that canonical proof
  and client shapes do not exist. Their honest tier is
  `test-proven-only`; the shipping register/fetch/replenish caller is absent. The blocked rollout
  order is migrations `0033` then `0034` before the matching Worker, not historical `0030`
  Worker-first.
- **Scrub IMAP:** exact `07384f67a829398527ed869034789304c7e74d87` is independently accepted
  `test-proven-only` for committed UI → Tauri → main-only ACL → native authority. No production
  caller arms `authorize_attended_imap_batch_reviewed`, and no live fixture ran; F3 is not
  code-complete.
- **Provider cleanup:** exact `085b4e2d8837a8a2facc9ea09d73fb6a5f6e60ae` owns the lifecycle in
  `apps/osl-hub/src/main.rs` and `security.rs`, not `cleanup.rs`/`services.rs`. It is independently
  `REJECT +0` pending a security successor.
- **Revocation broker:** exact `0572893105d1d75f9a0d43a4fae86539896b5321` is independently accepted
  `test-proven-only`. The predecessor broker-defect assignment is superseded; remaining work is
  integration and controlled two-identity runtime.

These corrections change ownership and blockers only. No claim is promoted, no row changes, and
the authoritative score remains **100 / 303**.

**2026-07-27 exact newly-landed-set acceptance audit (+0).**

- F1 stays **4/6**. Exact product `7bbf2e9e966634e7d434ec230b75a7f666db0003`
  (tree `0998d54e2d52de9b42150dd9838a352736ade255`) and manual bundle
  `4458341f1d4e567351117a9a03e4a1a8aa055492`
  (tree `ed32ee52939c171bb3659980ec5472bd2e0ce939`) are `test-proven-only`.
  They deliberately contain no Windows execution. The row literally requires the exact-build live
  picker → grant IPC → nonempty import → revoke IPC → restart persisted reread receipt, so the
  runtime boundary remains `blocked`.
- A7 stays **0/5**. Exact Store anchor
  `8bfe4385b07ec8e18733663ab12d994dadfe61c0`
  (tree `9b3e6c8d4ea16a8f44311920e835f41630ae48b8`) adds tested coherent-rollback
  detection, but it is `implemented-unwired`: the only `open_anchored` callers are its tests and
  the provider is an in-memory double. The nearest receipt is a production caller using a
  platform-backed monotonic provider, with restart proof that stale coherent SQLite state refuses
  and the exact one-generation crash ambiguity retries safely.
- D6 stays **0/4**. Exact IPC `b25f6e80e06ea85efac1c2e12133c9556ff66f4c`
  (tree `452dbd4bfab2318f422d31fd9400427053a61694`) is `test-proven-only`: it
  durably records local destructive-control disposition before inbox deletion and retains failures
  for retry. The row still needs pending migration 0031 plus a two-authenticated-identity runtime
  receipt showing forced Store/peer-map persistence failure retains the row, then retry applies the
  same burn and the peer observes the correct scoped result.
- B3/B6 gain no point from exact Hub
  `e6a8e0aeda4da756cb25d6e6cf23ac114dbdbc76`
  (tree `d9234b7fd973bc95ccd60b1ae19076e4539b2cf5`). It wires the shipping v3
  sequence admission calls in production source and is `test-proven-only`; B3's literal boundary
  is ratchet persistence/replay/reorder/skipped-key/crash recovery, while B6 requires the controlled
  two-identity handshake/send/receive/offline-queue/restart/drain/attribution receipt.
- F2/F3/F10 stay **3/5, 3/5, 1/3**. The Scrub chain
  `ddc8782a3fb67339fa3a891f78c291a270d99b95` →
  `f13dbad4ead9a758970403bb6c472b3b82a71ff5` →
  `d5b6881302223f2ced89ea9a1d1c2f41510ea197` →
  `aade1014c9c0998f2e3749e2a1f74404539f06c8`
  is `test-proven-only`. The final successor closes the independently found Discord invalid-date
  and shared unbounded-deflate defects, but Meta still accepts impossible calendar dates through
  `Date.parse` normalization. WhatsApp honestly remains scan-only with no provider-authoritative
  account-wide inventory or owner binding. No exact product/VM import-review/status receipt exists,
  so no literal row boundary is crossed.
- Product claim gate `3e139540568e7078073fa6ea433bee9de1906eed`
  (tree `633a2a9967631bcfe3ef675e96a06b9661d970ad`) and website claim gate
  `2ab6b298fe6711c49b4c09010e39fbf29437b689`
  (tree `02a88e6e9100b8224f5f2fcf5688304d59f71ed0`) remain
  `test-proven-only` pending independent exact-object acceptance. They are frozen and the checklist
  writer does not self-score them. A8 and H8 are already full; a string gate cannot earn A6, and H1
  still requires production promotion plus the keyserver redemption boundary.

No named object crosses a new literal acceptance boundary. The authoritative result remains
**100 / 303**.

**2026-07-26 recheck — why the number went down.** Three earned points were withdrawn after
source verification, and three points of scope were added; both are shown rather than netted
away, per master §16.3.

- A7 −1 · duress/auto-lock/10-attempt burn have no production caller; burn is not cryptographic.
- B5 −1 · keyserver deploy state is `unknown-recheck-required`, not `DONE`.
- D6 −1 · bilateral burn is inert *and* the drain destroys the revocation notice.
- J7 +2 earned, +3 weight · public-claim allowlist created.

**2026-07-26 truth lane, second pass (+2).**

- H1 +1 · one pricing manifest now provably drives all 15 pages (0 marker drift, 0 conflicting
  price/renewal/entitlement claims), and the missing no-renewal limitation now ships on `index.html`.
  The last point is held for promotion to production and the keyserver redemption change.
- J7 +1 · the claim crawler required by master §8.6 exists and is proved by known-bad fixtures.
- **Bookkeeping error found and fixed.** Section H`s header said 5 earned while its own rows summed
  to 7. Rows are the authority, so the real baseline was 87, not 85 — the snapshot had been
  understating the project. Nothing was built to close that gap; it was an addition error.
- H8 −1 · corrected an overclaim: H8 sat at full marks while its own text named an open defect.
  200% zoom and the accessibility checks in master §8.6 are still unmeasured.

**2026-07-27 exact-commit A8/B3 adjudication (+1).**

- A8 +0 · schema-v4/v3-scrub evidence is strong, but the held comprehensive at-rest audit remains
  open across v2/unstamped legacy paths, a raw-byte v1 migration check, an executed older-reader
  downgrade test, attachment selector-to-metadata validation, and non-store persistence.
- B3 +1 · exact ratchet and later dependency-closure archives prove sealed persistence plus
  replay/reorder/skipped-key/eviction/rollback/restart behaviour with non-vacuous negative controls.
  Status remains `test-proven-only` on an `implemented-unwired` path; no runtime row moved.

**2026-07-27 signed per-sender drain counteradjudication (+0).**

- B5 +0 · the `a509cb2` award on exact `c0279dc` is retracted. Its two-sender success runs only
  through the keystore client, while `broker.rs:6463` is source-text inspection. All three broker
  relay fakes omit mandatory `filtered_sender_id`; the actual head-of-line broker test fails before
  delivery and still characterizes starvation. Source reachability is not production-drain
  behaviour.
- B5 +0 · exact later commit `aca9dae` strengthens the shared fetch helper but still stops below the
  frozen broker-drain boundary. Its two passing tests call `fetch_peer_control_inbox` directly, not
  either actual drain; its type-marker rows are not authenticated/decrypted notices; and its
  stateless fake preprograms A then B, so it does not prove 64 foreign blockers remain untouched.
  The four missing/mismatched/cross-sender/unfiltered refusals are real `test-proven-only` evidence,
  but do not supply the required positive.
- A6/D2/D4 +0 · exact `79f12eb` proves its policy helper, but deleting the helper call from
  `open_pending_inner` leaves the focused test green. Source ordering improved; the real viewer seam
  and image continuation are not behaviorally proved.

**2026-07-27 six-commit exact adjudication (+0).**

- F2 +0 · exact `c5b516f` binds all four identity evidence objects to client, run, and semantic
  state with mutation-sensitive tests. It is `test-proven-only`; no authorised strict-verifier-green
  real VM walkthrough exists. F2 is code-ready but runtime capture remains `blocked`.
- I4 +0 · exact `6cc103b` closes a real whole-desktop/6×6-marker false green and passes 15 host
  regressions, but its final exact VM positive refuses an off-screen `GetWindowRect` surface and
  produces no artifact. Fail-closed VM binding is not a promoted candidate.
- A2/B5 +0 · exact `8802225` proves local Miniflare/D1 namespace refusal and requires Worker-first,
  migration-second rollout. Migration `0030` and this Worker are not deployed; scheme 1 remains
  `implemented-unwired`.
- D2 +0 · exact `e8fbd3f` proves local D1/R2 stale-upload reclamation, R2-before-D1 ordering, and
  retry on abort failure. Exact recovery source `3938a73` is `runtime-proven` locally; exact release
  contract `1e9e635` is `test-proven-only`. Migration `0010` is unapplied and its matching Worker is
  inactive, so production wrong-size/abandoned recovery and quota release remain `unknown`.
- F5 +0 · exact `f0bd0e1` records rejection of the proposed isolated scan-only commit: the retained
  exact tree is not type-coherent and the passing exploratory tests were removed.
- C4 +0 · exact `31edb63` is inadmissible as shipping evidence. Its verifier accepts caller-authored
  build/process facts, replayed bundles, fake screenshot content, and a source substring whose real
  Send call can be unreachable. No owner Discord send should run against this harness.

**2026-07-27 B5/F1/website exact adjudication (net +0).**

- B5 +1 · exact `5ba5a029445ba0ce500aad2db8de4295cbd605ff` supplies the behavioral
  broker boundary that `c0279dc` and `aca9dae` did not. Its exact-archive suite passes 8/8. The
  stateful positive invokes the production text drain and attachment list/take consumers, opens A's
  authenticated text and attachment notice/open plan behind 64 retained foreign row IDs, then
  drains B independently.
  Missing echo, mismatched echo, cross-sender rows under a matching echo, and an unfiltered fallback
  are refused for both consumers. Independent text-only and attachment-only regressions to the
  unfiltered client call each exit 101 at the intended nonempty assertion. Status is
  `test-proven-only`: this is not live D1, provider, or two-identity evidence.
- F1 −1 · exact `ae9d5a1` supersedes and withdraws the earlier F1 +1 recommendation. The native
  browser reader's default-deny/one-profile scoping remains `runtime-proven`, and direct native
  grant/revoke persistence is `test-proven-only`; the renderer picker, Tauri grant/revoke IPC, and
  persisted **UI** revocation were never exercised in a live Windows workflow. Exact VM capture is
  `blocked`. F1 returns 5/6 → 4/6.
- H1 +0 · website candidate `15fa16c95123e1b524858719b8d097aa434b05a6` is a clean local,
  `test-proven-only` candidate, not a promotion-ready artifact. Build-identity 38/38, live-verifier
  12/12 local-fixture, pricing, status, and claim gates pass, but the branch is three commits ahead of the local
  `origin/main` ref and is unpushed/undeployed. Pages dashboard command/environment, a Pages build,
  live SHA-bound `/build.json`, and the keyserver redemption half remain `unknown` or absent.
  An independent exact-archive audit also found that false/duplicate HTML meta can satisfy the
  textual check, nested asset HTML is not stamped, `_headers`/`_redirects` are outside artifact
  comparison, and undeclared served files pass verification. The required planted ignored-file
  refusal also does not occur, although the ignored bytes are correctly excluded from the artifact.
  The pre-existing crypto-checkout gate is 9/10 (`Pay once` versus `One month`). H1 stays 3/4.
- Arithmetic is explicit: **97 + 1 − 1 + 0 = 97 / 303**.

**2026-07-27 D7/website non-scoring safety reconciliation (+0).**

- D7 stays **1/4**. On committed product HEAD
  `05282a493fbb165e2011edc2991a273ead3dddc6`, the native-overlay receipt lane provides a real
  authenticated, sender/recipient/scope/expiry-correlated receipt foundation and admits a receipt
  only against an existing sent-message record. That supports the existing foundation point.
  However, all three production `Opened` emission branches in `drain_peer_inbox_text` call
  `send_native_overlay_acknowledgment` without a locked mutual-consent decision, and incoming
  `Opened` acknowledgments can pass validation into the encrypted ledger/UI without one.
  `OpenedReceiptConsent` and `opened_receipt_status` exist only as an unwired contract/test
  foundation. This is an `open-security-finding`, not a point withdrawal or a new point.
  Immediate fix: fail closed by suppressing production `Opened` emission and rejecting incoming
  `Opened` before ledger/UI admission while leaving `Received` behavior intact. Restore condition:
  durable, scope-bound, identity-bound, signed, expiring and revocable mutual consent, checked under
  the receipt-state lock at both emission and admission. Pending dirty or later crypto bytes are
  not evidence until committed and independently adjudicated.
- H1 stays **3/4**. Exact local website candidate
  `4e2256333c53e6b6e17462657260f5d6499ec9ee` supersedes `15fa16c` as the candidate under review.
  Its local gates pass and its checkout is deliberately disabled because the promised paid-code
  redemption/one-month enforcement is absent. It is clean, four commits ahead of the local
  `origin/main` ref, unpushed and undeployed. No Pages build/live SHA proof or keyserver redemption
  exists, so status remains `test-proven-only` and H1 earns +0.
- Arithmetic is unchanged: **97 + 0 + 0 = 97 / 303**.

**2026-07-27 exact A8, website, and current-lane reconciliation (+0).**

- A8 stays **3/4; +0** under the unsplit rule. Exact
  `ae8a59b47187059f07170d218ac0144f75e65d1f` is a five-path store-only commit whose exact archive
  passes 52/52 tests. It closes the store-local v2/unstamped migration, raw SQLite/WAL/SHM/sidecar,
  burned-parent attachment, selector-to-decrypted-metadata, and exact `adff4e45` older-reader gaps.
  Classification is `test-proven-only`, not product-wide at-rest proof. The held clause is false:
  `crates/ipc/src/peer_map.rs:405-412` and `membership.rs:241-248` write plaintext JSON when the
  main-password storage key is absent, and `main_password.rs:823-827` documents that fallback.
  Committed UI source adds plaintext browser storage: `main.ts:400-402` names localStorage keys;
  `:605-629` restores muted person IDs, person-ID-keyed unread counts, and notification
  IDs/titles/event timestamps; `:4343`, `:5187`, and `:5192` persist them as JSON. These are further
  product-wide A8 blockers, not store-local defects.
  Independent review also bypassed the proposed known-file sink inventory with
  `File::create_new` plus `std::io::copy`; the product-wide inventory was therefore removed rather
  than blessed. Remediation is a separate crypto/native persistence task, not more store fixtures.
- H1 stays **3/4; +0**. Independent review rejected `f39b805` because “An activation code grants
  30 days of Pro” bypassed its exact phrases, then rejected `5a84d77` because “One month of Pro
  comes with each activation code” bypassed its verb-dependent rule while honest planned and
  once-implemented copy false-failed. Independent review also rejects `4aea9fe`: activation-key
  and paid-Pro-voucher synonyms plus a two-paragraph “It lasts 30 days” continuation bypass it,
  while “is a planned feature,” “is unimplemented,” and “if automatic expiry is implemented”
  false-fail. Independent review also rejects `be16e7c`: product-key and redemption-token
  synonyms, entity/inline-tag-obscured durations, and both orders of cross-sentence claims bypass
  it, while “are planned to provide” and “provided expiry is implemented” false-fail. Exact
  successor `a409fb1d951278f8bcb72be77689d52dfc34e4f5` parses decoded rendered text, joins inline
  fragments, preserves block boundaries, and evaluates one bounded adjacent segment in either
  direction. Its adversarial positives and honest negatives make 50/50 crawler fixtures pass;
  16/16 public pages, 18 pricing markers with zero drift, and 10/10 fail-closed checkout tests also
  pass. Exact keyserver `90da747` adds a source-owned,
  non-operator-configurable refusal at paid issuance boundaries, but it implements no redemption
  clock and is not deployment-proven. The website is nine commits ahead of its local
  `origin/main`, unpushed and undeployed; live Pages identity remains `unknown`. The successor is
  `test-proven-only` pending independent acceptance.
- C4 stays **1/4; +0**. `48162bd` publishes a real Rust return through a renderer seam, while
  `266e6f3` hardens the v3 verifier/ledger. Exact `80ed9a2` adds a declarative test boundary, but
  independent review rejects it as production-wired: nothing in the app, Rust crates, workflows,
  build, or release path calls it, and captured integration defects prevent using it as evidence.
  Its ceiling is `implemented-unwired`; no production native receipt binds build flavour, target
  process/HWND/executable, carrier/readback, and one-time challenge consumption. The evidence run
  remains `blocked`.
- D7 stays **1/4; +0**. Exact `c06eed3` wires the immediate fail-closed safety response into
  production source: suppress outbound `Opened` and reject inbound `Opened` while preserving
  `Received`. This is `test-proven-only`, not `verified-live`; the durable mutual-consent contract
  remains `implemented-unwired`, and the complete receipt outcome matrix still depends on B6/D3-D6.
- Scrub/VM stay **F1 4/6, F2 3/5; +0**. Exact `be5355d` has coherent production source but its
  acceptance harness is rejected: it cannot prove Tauri IPC, durable reread, or commit-bound
  executable identity, and its receipt accepts unknown versions/fields. VM harness `fedd9e2`
  improves retained evidence binding, but no exact-build picker → grant IPC →
  nonempty scoped import → revoke IPC → persisted restart re-read has run. The retained VM pair is
  narrow `runtime-proven` harness diagnostics, not an F1/F2 positive; the product walkthrough
  remains `blocked`. Exact later VM commit `2da489a` is rejected for execution because its live
  agent cannot emit the accepted V2 verdict and its build/Azure provenance remains forgeable.
- Release stays **I3 1/3; +0**. Exact `caa4ba9` is `test-proven-only` structural policy: its local
  administrator JSON is forgeable and no integrated candidate, push, exact-SHA hosted Rust/UI/audit
  result, tag, promotion, or rollback exists. Exact `474c629` adds local QA-registration and
  window-targeting guards; independent review accepts the scanner's narrow refusal proof, but
  there is still no hosted run or integrated candidate and separate assembler/release-use defects
  remain. It is also `test-proven-only`; the current release worktree's later bytes are dirty and
  `unknown`.
- Server evidence adds no point. Keyserver `16fcf49` is local `runtime-proven` for its control-inbox
  retention state machine but undeployed; `90da747` is `test-proven-only`. Exact
  `03b5a0215df69390345b2eb4af7be9c68ea075eb` is also `test-proven-only`: independent clean-archive
  review reproduced the deterministic parent failure and accepted the production register/pubkeys
  boundary plus D1 result as a genuine nonempty RN capability-advertisement proof, not a timing fix
  or fake endpoint. It does not make any live client advertise nonzero RN capability, so B4 stays
  0/4.
  A read-only Wrangler check at 2026-07-27 10:13-10:18Z found keyserver deployment
  `8fa82d0b…`, version 169 at 100%, and cipher-store deployment `785314a9…`, version 15 at 100%.
  Neither carries a Git SHA. Remote D1 reports keyserver migrations 0030 and 0031 pending and
  cipher-store 0007 pending, so 0031 retention is definitively not live and `16fcf49`, `90da747`,
  `9de23ba`, and `fc91401` are not deployment-proven. Live checkout refusal, RN advertisement,
  cron, and a natural D2 cleanup cycle remain `unknown`. Cipher-store `05282a4`, `9de23ba`, and
  `fc91401` improve fail-closed D2 proof tooling, but the promotion helper has no production caller
  and is `implemented-unwired`.
- Arithmetic is unchanged: **97 + 0 = 97 / 303**.

**2026-07-27 post-H4 exact reconciliation (+0).**

- Product truth +0 · independent exact-archive review accepts
  `62e362e4e05a0431d0842c384454271359223648` as `test-proven-only` truth hardening.
  Reachable UI now describes authorization expiry/refusal and says local authorization was
  consumed; the broker stores no per-message key and destroys no such key. This corrects claims
  without adding a key lifecycle, runtime proof, or checklist point.
- F1 +0 · exact `3a5875f24b2871094a6b5c7d9b5ebab195db9db9` is rejected for VM
  admission. Its 25/25 mutation matrix closes malformed-evidence cases, but the VM evidence author
  can still synthesize the agent-selected native journal and its unkeyed public-hash chain. F1
  remains 4/6 and no VM run is authorized by this object.
- VMQA +0 · exact `b9369ec9e9ebcd549fa5866a88392db955b07484` is rejected. Independent
  reproduction created accepted build evidence from a clean unrelated product-shaped repository
  plus arbitrary `/bin/true` while supplying matching caller-selected commit/tree facts. VM
  execution remains blocked; no F1, F2, release, or infrastructure row moves.
- Website/A8 +0 · exact `5ec4d4fbd0953350a6ad9460e653b4ecc31f166c` contains truthful
  qualified copy, but its semantic gate still accepts the false broader sentence “All private state
  is encrypted at rest.” It is unpushed and undeployed. H1 and A8 remain 3/4.
- Keyserver +0 · independent review accepts exact
  `91bfcaac4e319a5ab0fad77ecf9b0002cbf09779` as a `test-proven-only` trusted-admission
  successor. It binds exact clean source, recomputed trees/tools/artifacts, internally captured
  D1/deployment evidence, and stable active-version reads. Full admission was not run because it
  requires disk-heavy clean builds/installs; production lacks 0031, so Artifact A is presently
  eligible and Artifact B must refuse. No deploy, migration, or point follows.
- At this reconciliation point the VM and Scrub repairs after the rejected objects are uncommitted
  dirty work and are excluded. They require exact commits and independent acceptance before any
  narrative or score change.
- Website H7 +1 · exact website chain ending at
  `71c9420583c9da02cd3cfcc55abaf6730c82355b` (tree
  `17beb7b37f3f9b83bc072f82b66ea49a433df2e3`) is independently accepted for the
  source-bound comparison half. It preserves seven fixed dimensions, 15 official DeleteMe/Abine
  sources, explicit conflicts/unknowns, exact `Planned` and `sellable:false` OSL limitations, and
  exactly 20 named semantic mutations; the final 12/12 field-isolation audit rejects replacement
  or appended operational wording independently in every referenced capability `evidence` and
  `public_note`. This is local `test-proven-only` evidence, unpushed and undeployed; it proves no
  DeleteMe efficacy, live source refresh, or OSL runtime equivalence.
- Website/A8 +1 · exact website chain ending at
  `d158c88a188a6a006ea21c0a9fc0c630f513e91a` (parent
  `8abdc2ff28c9c3cf6385bc66250f30407845a894`, tree
  `6295020f7f20900c9ff869d479b744f4e7d131ef`, gate blob
  `51a8b0551e91155597c84f7b2abfd9295bc6cd31`, archive SHA-256
  `f5cafc221a879438fbaf08105a75ce591ec32eaeba96b0d9fbb1fc04907d5e5a`) received
  **ACCEPT +1 from both immutable independent exact-object audits**: `store` receipt
  `20260727T170053Z-5a7af576` and `audit_points` receipt
  `20260727T170139Z-35a6fd5f`. Both are source/static-test-tier verdicts. The authoritative machine-readable
  census has raw SHA-256
  `a7468b174b07301873eee1a1a9e5e01f5af30716df2dbb4bac13f65820c83246` and
  accounts for exactly 18 backends, nine bound public claims, and 44 source anchors. The final
  gate passes 16/16 public surfaces, 419/419 self-test fixtures, and 38/38 inherited receiver
  controls; it closes all eight previously rejected DOM alias/order/lifecycle cases, including
  harmless remove and front-insertion distinctions. This is unpushed and undeployed and proves
  no named runtime or release bytes; A6 remains 0/5.
- Arithmetic is now **99 + 1 = 100 / 303**.

**2026-07-26 — recorded, deliberately NOT awarded.** Open defects earn nothing; they are
logged here so they cannot be quietly forgotten or later re-counted as new work.

- **Control-inbox retry bound (crypto lane, open).** The revocation/control lane must behave as a
  **retryable queue, not a dropped notice** — a 404 or absent lane has to be retried, not discarded.
  Related to the already-recorded D6 defect where the drain DELETEs an inbound revocation frame
  without applying it. No point until a bound is implemented and exercised.
- **Duress/auto-lock reachability (open).** `crates/keystore/src/duress.rs` still has no production
  caller, so no user-reachable or runtime evidence exists. A7 already sits at 0 for this reason;
  nothing here changes it.
- **Superseded 2026-07-27 — causal record of the former Store gap.** On 2026-07-26 the then-current
  `MessageStore::put` did not populate `wrapped_key`, so burn nulled an already-null column. Exact
  `a7a09a7` and `c92ce11` supersede that source fact with test-proven v5 message and v6 attachment
  content-key envelopes. Exact successor `15c393e` adds the test-proven schema-v7 authenticated
  attachment inventory and terminal attachment stubs described in A7. These changes do not
  supersede A7's external-anchor, runtime, caller, or duress/auto-lock blockers, and no point is
  awarded.
- **In-app copy gate — CLOSED 2026-07-26, no point claimed.** `scripts/check-app-claims.mjs` scans
  `apps/osl-hub-ui/src/**` string and template literals plus `README.md` against allowlist section D,
  which it **parses from the allowlist itself** so there is no second list to drift. Wired into the
  `TypeScript Test` workflow. First clean run: 28 phrases parsed, 7,345 strings scanned, 0
  violations. Floors (>=8 phrases, >=300 strings, README non-empty) proven to fire by starving them.
  12/12 fixtures, four of them added after the first run produced **false positives** on ordinary UI
  copy ("Every batch is reviewed and confirmed") — a gate that cries wolf gets switched off, so
  single common words now fire only in security context while multi-word bans stay absolute.
  **No point claimed:** this closes a gap the truth lane opened itself, and the H/J rows already
  cover claim tooling.
- **Received/Opened receipts collapse into one `acknowledgmentCount`** (`broker.rs`), losing order —
  the operator cannot distinguish delivered from read.
- **Zero-caller claim seam (truth lane, no point claimed or withdrawn).** Current-source
  re-verification confirms `post_wrapped_key`, `fetch_wrapped_key`, `BurnAlertPayload`, and the
  `osl_notes`/`osl_assets`/`osl_lan` cluster have no production caller. Triage: wrapped-key POST/GET
  are **INTERNAL ONLY**; the Notes cluster is now **INTERNAL ONLY** after removing one website terms
  sentence that implied it worked. **Superseding r12:** commit `dae12da` removes README's
  present-tense signed-notice promise; `README.md:85-87` now says the peer path is not proved, not
  available today, and must not be relied on to remove a peer's copy. The signature layer is now
  **UNCLAIMED**, while remaining `implemented-unwired`. The newer `0x0A` revocation path is separate
  and does not make that signature layer reachable. Node 24 reports 7,509 scanned units and 0
  violations on current shared bytes; this remains string evidence only. Negative control:
  `pricing-sync --check`,
  `build-status --check`, and `check-claims` all passed while the website leaks were present. These
  are string/status gates, not call-graph evidence. J7/H8 keep their points because their real
  boundary is the allowlist and tested string/status consistency; no row here claims a reachability
  gate. The open seam is recorded in the allowlist and truth report.
- **B5 deployment blocker resolved, no point restored.** Coordinator evidence names keyserver
  Worker `3f92f0f5`, cipher-store `0a17547d`, and deployed migration `0027`; the earlier recheck
  entry that called B5 owner-blocked is historical and no longer on the critical path. B5 remains
  partial because its prekey/wrapped-key client production path is not established.

- H5 +1 · the master §8.4 versioned support matrix now exists at `/docs/status`, generated from the
  pricing manifest and guarded by `build-status --check`. Added in the owner reframe (master r7),
  which moved site honesty out of per-card badges and into two concentrated surfaces.
- Not earned, recorded instead: five capability badges were **downgraded** after source
  verification — `per-message-sealing` and `image-send` and `scrub-discovery` and
  `scrub-guided-deletion` and `cover-carrier-text`. No checklist points were withdrawn, because
  none of those badges had ever earned a point; they were website overclaims, not recorded progress.

None of these is a regression in the code. They are corrections to status that was previously
recorded more favourably than the evidence supported.

The Telegram bot must calculate the live percentage/ETA from current weighted acceptance items and
timestamped deltas, not keep this number manually forever.

## A · Identity, trust, storage, and security — 40 points (9 earned)

- 🟨 **A1 · Local OSL identity and recovery** — create/import/unlock exists; recovery/capture and
  password-at-rest claims need reconciliation. `needs: none` `weight: 6` `earned: 3`
- 🛑 **A2 · Full-bundle identity binding** — identity must authenticate Ed25519, X25519, ML-KEM and
  capability bundle; no keyserver substitution. Exact server/admission candidate
  `e273436dfaf735daab44adbbeb205a3c95ecd4cc` and keystore construction lineage
  `a1b82a008f53d4864459d353bb6a89fc8753a446` provide
  `test-proven-only` canonical proof/client shapes, superseding the older “absent” wording. The
  shipping register/fetch/replenish caller is still missing; migrations `0033` then `0034` and the
  matching Worker remain unapplied/unshipped. `needs: A1` `weight: 6` `earned: 0`
- 🛑 **A3 · Sender authentication equals displayed attribution** — no caller-supplied identity can
  relabel authenticated plaintext. `needs: A2` `weight: 5` `earned: 0`
- 🛑 **A4 · Proven platform-account registration** — nobody can pre-register another owner's public
  service ID. `needs: A2` `weight: 4` `earned: 0`
- 🟨 **A5 · Friends, safety numbers, scoped trust, held key changes** — foundations exist; full
  ceremony and all call paths need proof. `needs: A2` `weight: 5` `earned: 2`
- 🛑 **A6 · No protected plaintext at rest** — attachment staging and metadata findings remain.
  **Evidence note 2026-07-27:** exact Store commits `a7a09a7` (message schema v5) and `c92ce11`
  (attachment schema v6) test-prove per-record content-key envelopes for Store message and
  attachment bodies. Exact successor `15c393e` test-proves an encrypted schema-v7 attachment
  inventory for that same Store backend, with complete coverage only for newly observed sets and
  explicitly incomplete coverage for migrated sets. That narrows one backend; it does not close
  the authoritative at-rest census, staging, backup/rollback, caller-root, runtime, or
  physical-media boundaries. No score changes.
  `needs: none` `weight: 5` `earned: 0`
- 🛑 **A7 · Honest Burn/duress and retry** — failed/skipped wipes, TPM errors, rollback, and key
  handlers must be fail-closed/retryable. **Superseding recheck 2026-07-27:** exact `a7a09a7`
  test-proves a fresh random DEK and master-wrapped key for each live v5 message; exact `c92ce11`
  does the same for each live v6 attachment and transactionally shreds the selected message plus
  its attachment wrappers. **Exact successor `15c393e` (schema v7) supersedes the former
  no-manifest and trim-resurrection findings:** its exact archive passes 84 Store tests; new
  attachment sets receive complete encrypted/authenticated manifests, migrated sets are explicitly
  incomplete, and open/get/list reject missing, extra, reordered, swapped, truncated, stale, or
  wrong-owner row/manifest state. Row/manifest update and burn/delete failures roll back together,
  unrelated manifests remain byte-exact, attachment stubs remain terminal across trim, and
  v4→v5→v6 plus v6→v7 interrupted migrations remain coherent and retryable. Mutation controls fail
  when validation, rollback, terminal-stub preservation, or the complete/incomplete distinction is
  removed. Highest honest tier remains `test-proven-only`, not runtime or release proof. A7 remains
  0/5: the independent audit restored an older attachment row together with its older authenticated
  manifest and the Store accepted the coherent replay, because no external monotonic anchor binds
  the manifest generation or database backup. An incomplete migrated manifest also cannot prove
  whether a row disappeared before migration. Caller OS-root, runtime, real power-loss, backup
  rollback, and physical-media evidence remain absent. Separately, duress, 10-attempt auto-burn,
  and 15-minute auto-lock still have zero production caller and therefore no user-reachable/runtime
  evidence.
  `needs: A1` `weight: 5` `earned: 0`
- ✅ **A8 · Secret zeroization and metadata minimization** — **+1 awarded 2026-07-26 by the checklist
  writer, which the crypto lane did not claim.** Both halves of the row got real work with evidence
  of the right kind. Zeroization: `crates/keystore/src/storage.rs:75` now derives
  `Zeroize, ZeroizeOnDrop` on the secret-carrying struct, with
  `zeroizing_an_inner_identity_clears_every_secret_field` (:298) asserting every secret field clears
  including recovery entropy, and `secret_carriers_wipe_themselves_on_drop` (:344) pinning the
  derive. Metadata minimization: identifier redaction in `crates/ipc` (e.g.
  `cipher_store_client.rs:811-812`, `:847`, `wire_v2.rs:1098`). A unit test is the *appropriate*
  proof for a memory-wiping property — it cannot be observed end to end — so this is not the
  "shallow test" failure the rule guards against. Verified by source inspection, not by running
  cargo: this lane does not contend for the cargo lock. **Re-checked 2026-07-26 against the
  feature-gate exclusion and it does NOT apply here.** `qa_selftest_request` is gated at
  `apps/osl-hub/src/lib.rs:68` by `#[cfg(all(feature = "core", feature = "discord-qa-shell"))]`, so
  `--features core` compiles it out — but these tests live in `crates/keystore/src/storage.rs` with
  **no cfg feature gate at all** and run under `cargo test -p keystore`, which never involves the
  osl-hub feature set. **The award rests on source inspection the writer performed directly**; the
  lane's inherited 176/0/1 is corroboration only, and an inherited number is not a measurement.
  Strengthening the case: the test documents its own **negative control** — it does not compile
  against the pre-fix code, because `InnerIdentity` had no `Zeroize` derive — and honestly labels
  that a compile-time rather than runtime control, noting that reading a freed buffer to observe the
  wipe would be undefined behaviour. **2026-07-27 no-point adjudication:** exact `48b3ef1` passes
  48 store tests and proves current schema-v4 identifier labels, raw-file v3 scrubbing, v1/v3
  readability and a v4 version stamp. A dual mutation disabling both `secure_delete` and the
  pending `VACUUM` makes the v3 scrub test fail on a recoverable channel id. This is
  `test-proven-only` and does not close the held comprehensive boundary: v2 and unstamped legacy
  paths have no dedicated fixtures; the v1 test does not scan post-migration raw bytes; the
  downgrade test asserts the version stamp rather than executing an older reader; attachment blind
  selectors are not cross-checked against sealed metadata; and persistence outside `crates/store`
  remains unaudited. Exact later `ae8a59b` closes those named **store-local** gaps with 52/52
  exact-archive tests and mutation controls, but earns +0 under the unsplit boundary. The product
  still deliberately persists plaintext JSON without a main-password storage key in
  `peer_map.rs:405-412` and `membership.rs:241-248`, as documented at
  `main_password.rs:823-827`. Committed `apps/osl-hub-ui/src/main.ts` also persists plaintext
  browser metadata: muted person IDs (`:400`, `:605-607`, `:4343`), unread counts keyed by person
  ID (`:401`, `:611-617`, `:5187`), and notification IDs/titles/event timestamps (`:402`,
  `:621-629`, `:5190-5192`). A lexical sink inventory was independently bypassed with
  `File::create_new` plus `std::io::copy`, so it was not accepted as product completeness.
  Deterministic blind indexes also preserve equality/frequency, counts, sizes, order and burn
  timing, so A6/social-graph secrecy is explicitly unearned. **Final A8 truth-accounting point
  awarded 2026-07-27:** exact website chain `d158c88a188a6a006ea21c0a9fc0c630f513e91a`
  binds every public at-rest claim to one machine-readable 18-backend/44-anchor census and
  independently passes 419/419 static gate fixtures, including contradictory additions,
  missing/unwired backends, unreachable truthful strings, and DOM order/lifecycle distinctions.
  The census expressly limits itself to source-inspected/static-test-proven evidence; runtime,
  release, migration, swap, backup, and A6 claims remain unproved.
  `needs: none` `weight: 4` `earned: 4`

## B · Encryption and two-identity communication — 30 points (8 earned)

- 🟨 **B1 · Current message encryption primitives** — hybrid confidentiality exists **and is
  genuinely live** (ML-KEM-768 in every recipient slot, `crates/ipc/src/wire_v2.rs:731`);
  ratcheting claims are not merely insufficient but absent — the `v=4` DM Double Ratchet is dead
  code (`crates/ipc/src/commands.rs:2842`) and `v=5` group sender keys are gated off (`:2938`,
  default documented false at `crates/ipc/src/state.rs:280`). All traffic is stateless `v=3`:
  no forward secrecy against recipient compromise, no post-compromise security. Reconciled in
  `docs/THREAT_MODEL.md` 2026-07-26. `needs: A2,A3` `weight: 5` `earned: 2`
- 🧪 **B2 · Ratchet-next protocol** — implementation/tests exist; **no production call path.**
  `crates/ipc/src/wire_rn.rs` uses the crate, but nothing uses `wire_rn` — its only non-test
  reference is the `crates/ipc/src/lib.rs:76` module declaration. Wire integration and review
  both incomplete; do not treat master §2's "gate 1 is clear" as consumed.
  `needs: A2,A3` `weight: 6` `earned: 2`
- 🟨 **B3 · Persistence, replay, reorder, skipped keys, crash recovery** — must be structurally safe
  and sealed. **+1 awarded 2026-07-27 at the row's structural boundary:** exact `86f1d0e` passes
  the ratchet crate's replay, reorder, eviction, skipped-key, rollback and restart tests; retaining
  a consumed skipped key makes the restart-replay test fail with an authenticated replay opening.
  That historical archive's IPC integration gate is blocked by missing later IPC/keystore APIs,
  so it is not used as integration proof. Exact dependency closure `1d8bfa8` separately passes 35
  `wire_rn` tests, including sealed save/load, persist-before-return, save-failure refusal and
  crash-reload non-reuse. Status is `test-proven-only` on an `implemented-unwired` ratchet: no
  real traffic, two-identity, or runtime point is implied. `needs: B2` `weight: 5` `earned: 2`
- ⬜ **B4 · Capability negotiation and monotone downgrade pin** — no silent fallback after a peer
  proves stronger support. Exact `03b5a0215df69390345b2eb4af7be9c68ea075eb` is accepted only as
  local Worker/D1 `test-proven-only` evidence: its clean archive binds the production
  register/pubkeys boundary to a nonempty RN capability advertisement and fails the relevant
  omission/stale/wrong-identity/empty-response mutations. No live client advertises nonzero RN
  capability, so the production negotiation/downgrade boundary remains unproved.
  `needs: B2` `weight: 4` `earned: 0`
- 🟨 **B5 · Keyserver/prekey/control-inbox production contract** — server deployment is
  `verified-live`: migration `0027` is deployed and the 2026-07-27 read-only Wrangler check found
  active deployment `8fa82d0b…`, version 169 at 100%. Cloudflare provides no Git annotation, so its
  exact source commit is `unknown`. Remote D1 reports migrations 0030 and 0031 pending; therefore
  0031 retention, checkout refusal and RN advertisement are not live-proven. The stale
  `NOT DEPLOYED` migration header does not override the narrower live evidence. The broader row is
  not complete: `post_wrapped_key` and `fetch_wrapped_key` are `implemented-unwired`, and the
  hand-checked evidence does not establish the prekey client path. No point added merely for
  resolving the old deployment uncertainty. **2026-07-27 no-point adjudication:** commit `284f0a5`
  bounds scheduled control-inbox and request-receipt cleanup to 100 rows per table per tick and is
  `test-proven-only`; local worker tests do not prove the changed Worker is deployed or complete the
  client production contract. **2026-07-27 correction:** the `a509cb2` +1 on exact `c0279dc` is
  retracted because its broker gate is source-text inspection and its exact relay fakes omit the
  mandatory echo; its actual head-of-line broker test fails before nonempty delivery. Exact later
  `aca9dae` passes two shared-fetch-helper tests and genuinely refuses four unconfirmed/widened
  pages, but it still calls neither actual drain, processes only two-byte type markers, and uses a
  stateless A-then-B fake rather than retaining 64 foreign blockers. **Later exact commit
  `5ba5a029445ba0ce500aad2db8de4295cbd605ff` crosses that narrow behavioral boundary:** its 8/8
  exact-archive suite invokes the production text and attachment consumers, opens A behind 64
  retained foreign IDs, proves B independently drainable, and refuses four missing/wrong/widened
  filter responses. Independent unfiltered mutations of the text and attachment production calls
  each fail at their intended nonempty assertion. This point is `test-proven-only`; authenticated
  live D1 selection, a mapped deployed Git commit, prekeys, wrapped keys, provider runtime, and
  attachment blob download/decryption, and two-identity receive remain unearned.
  `needs: A2` `weight: 4` `earned: 2`
- ⬜ **B6 · Controlled two-identity proof** — handshake, send, receive, offline queue, restart,
  drain, peer attribution. `needs: B3,B4,B5` `weight: 3` `earned: 0`
- ⬜ **B7 · Independent crypto review** — required before uncontrolled traffic/public superiority
  claims. `needs: B2-B6` `weight: 3` `earned: 0`

## C Discord reference adapter — 35 points (18 earned)

**Category C release gate.** Discord evidence is admissible only when it is
bound to the committed source object or exact executable under review, the
production Discord adapter path, the verified native Discord process/window and
conversation, and the configured user send authority. A synthetic harness,
source substring, caller-authored build fact, fake screenshot, replayed bundle,
or renderer-only return is supporting evidence at most; it cannot promote a C
row by itself. Missing consent, binding, authority, native-target proof, or
honest tri-state outcome is a refusal, not a degraded pass.

- ✅ **C1 · Carrier generation and encrypted commit** — exercised in live QA sends.
  `needs: B1/B2 gated path` `weight: 4` `earned: 4`
- ✅ **C2 · Native composer locate/write/full readback** — live multi-line exact writes proved on
  dated QA builds. `needs: none` `weight: 4` `earned: 4`
- 🟨 **C3 · Three send-authority modes** — existing automation work does not yet prove the locked
  clipboard/default, double-Enter, and single-Enter product contract. `needs: C2` `weight: 4`
  `earned: 1`
- 🟨 **C4 · Production tri-state sent proof** — sends have landed while receipts said failure;
  duplicate-safe production proof needs exact-build verification. Exact `31edb63` does not qualify:
  an independent audit accepts a marker-free fake executable with caller-authored shipping facts,
  identical fresh-bundle replay, fake PNG content, and an unreachable Send call preserved as a
  source substring. The collector also synthesizes native authority instead of retaining the
  production command response. Exact `48162bd` adds a renderer publication seam for the Rust return,
  and exact `266e6f3` makes the standalone v3 verifier/ledger mutation-sensitive, but neither adds
  the authoritative native emitter/caller. Exact `80ed9a2` remains an unwired test boundary, not
  transport/native authority; its captured current integration also had stale conformance
  fixtures/tests and a filename-to-challenge binding defect. There is still no production caller.
  The earlier accepted commits remain `test-proven-only`; the evidence run remains `blocked`.
  `needs: C3` `weight: 4` `earned: 1`
- 🟨 **C5 · Eye decrypt and place every authenticated row** — `runtime-proven`: `placedRowCount`
  `0 → 3`, `unplaceableRowCount` `3 → 0`, `rel_l −8 → 0` on QA executable SHA-256
  `6b6a36945b42…` (`osl-rehydrate-geometry-diagnosis.md`). This supersedes the `BLOCKED` row in
  the completion plan (Conflict C1). Final live appearance, peer rows, scroll/resize/DPI remain,
  and the measurement is on the QA shell, not the shipping build. `needs: C1,B5` `weight: 5`
  `earned: 3`
- 🟨 **C6 · Window/composer lifecycle** — adoption, drag, minimize, focus, close, first launch,
  corners/ring have had repeated fixes/regressions. `needs: harness` `weight: 5` `earned: 2`
- 🟨 **C7 · Scroll/resize/DPI/theme/Nitro/typography** — some fixes/probes exist; full matrix not
  proved. `needs: C5,C6` `weight: 3` `earned: 1`
- 🧪 **C8 · Adapter profile/change resistance** — crate/tests/scaffolding exists but production
  trust/update/rollback wiring is incomplete. `needs: C1-C7` `weight: 3` `earned: 1`
- 🟨 **C9 · App onboarding and plaintext safety** — cyan ring/alerts exist in work; exact locked
  meaning and tutorial need proof. `needs: C3,C6` `weight: 3` `earned: 1`

## D · Attachments and lifecycle — 30 points (7 earned)

<!-- category_D_release_gate: source-owned admission marker; scripts/admit-category-d-lifecycle.mjs enforces fail-closed row accounting. -->

- 🟨 **D1 · Structure-compatible text/image/video/file carriers** — text shaping works in QA;
  literal line/size/file-type parity and host transformations need proof. `needs: C contract`
  `weight: 5` `earned: 2`
- 🛑 **D2 · Encrypted attachment transport** — **status marker corrected 2026-07-26 from 🧪 to 🛑;
  the point is NOT withdrawn, and the reason matters.** Attachment upload was **broken in
  production, and had been before today**: the body was piped through a `TransformStream`, and R2
  requires a known length, so **every upload failed**. It passed its tests only because the R2 test
  double accepts any stream — a textbook false green, the same family as a harness that confirms
  whatever it happens to find. **Superseding evidence correction, 2026-07-27:** historical Worker
  `0a17547d` was not bound to a reviewed source object; its production 201 proves only that a
  known-length part upload succeeded on that Worker. It does not prove the newer recovery logic.
  Exact recovery source `3938a73` is `runtime-proven` locally against the real local
  Workerd/D1/R2 boundary. Exact `1e9e635` is a `test-proven-only` release contract; it is not a
  deployment receipt or Cloudflare observation. Migration `0010` is unapplied and the matching
  Worker is inactive. Production recovery of wrong-size or abandoned completing rows, and the
  corresponding quota release, therefore remain `unknown`. The single earned point stands because it was
  awarded for substantial source existing, **not** for uploads succeeding; withdrawing it would be
  over-correction. But 🧪 "test-proven" was an untrue label for a path that could not execute, so
  the marker is corrected. **Treat any "attachment sent successfully" reported anywhere before
  2026-07-26 as unproven.** Pending cover handoff, media measurement, cleanup, and
  no-plaintext-at-rest all remain. Exact `e8fbd3f` locally proves stale legacy upload reclamation,
  R2-before-D1 ordering and retryable abort failure, but it is not deployed and production cleanup
  remains `unknown`. Exact `b944e9a` is a later one-file Hub candidate for legacy deletion recovery:
  it keeps the shipping lowercase 32-hex attachment object ID and rejects wrong length, uppercase,
  cross-origin, cross-object and symbolic-link substitutions. This is source/test evidence only,
  not deployment, migration, or two-identity runtime evidence. Exact `05282a4`, `9de23ba`, and
  `fc91401` strengthen a fail-closed promotion proof and its starvation/bypass controls, but the
  helper is `implemented-unwired`; it does not bind an active Worker UUID to source or prove a
  natural production scheduled cycle. None of
  `e8fbd3f`, `3938a73`, or `1e9e635` is deployment proof.
  `needs: A6,D1` `weight: 5` `earned: 1`
- 🧪 **D3 · View-once text** — mechanisms exist; two-identity second-open refusal unproved.
  `needs: B6` `weight: 4` `earned: 1`
- 🧪 **D4 · View-once image/protected viewer** — viewer/link foundations exist; first-paint
  protection and end-to-end path unproved. `needs: D2,B6` `weight: 4` `earned: 1`
- 🧪 **D5 · Timed deletion** — scheduler/ledger pieces exist; production wiring and all lifecycle
  outcomes unproved. `needs: B6,D2` `weight: 4` `earned: 1`
- 🛑 **D6 · Bilateral Burn** — the earlier destructive control-inbox drop is superseded in local
  source: crypto `e4c9318` consumes explicit disposition, while keyserver `16fcf49` locally proves
  retry/quarantine/retention behavior. The keyserver half is local `runtime-proven`, but remote D1
  reports its required migration 0031 pending, so that retention contract is definitively not live.
  The combined product is not deployed or proved with two authenticated identities, and peer burn
  semantics remain gated by A7/B6. No point is restored.
  `needs: A7,B6` `weight: 4` `earned: 0`
- 🛑 **D7 · Receipts** — **1/4 retained.** The authenticated,
  correlation-bound native-overlay receipt/ledger foundation is real, so the existing foundation
  point stands. Exact `c06eed3` implements the immediate fail-closed response in production source:
  outbound `Opened` is suppressed and inbound `Opened` is rejected while `Received` is preserved.
  That safety fix is `test-proven-only`, not `verified-live`. The durable scope/identity-bound,
  signed, expiring and revocable mutual-consent contract remains `implemented-unwired`.
  Sent/received/opened/deleted/expired outcomes must still become separate, mutual where required,
  and evidence-bound.
  `needs: B6,D3-D6` `weight: 4` `earned: 1`

## E · Every offered app — 40 points (4 earned)

- 🟨 **E1 · Shared adapter contract/playbook/harness** — Discord lessons documented; generic signed
  profile and bench still incomplete. `needs: C` `weight: 6` `earned: 3`
- 🛑 **E2 · Discord release qualification** — finish all C/D required gates on production build.
  `needs: C,D` `weight: 6` `earned: 1`
- 🟨 **E3 · Signal** — hosting/QA foundations only; needs complete adapter and two identities.
  `needs: E1,B6` `weight: 7` `earned: 0`
- 🟨 **E4 · WhatsApp** — substantial separate QA worktree; needs current adapter qualification.
  `needs: E1,B6` `weight: 7` `earned: 0`
- 🛑 **E5 · Telegram** — stable app may lack accessible message rows; recheck and honestly block if
  impossible. `needs: E1` `weight: 5` `earned: 0`
- ⬜ **E6 · Outlook/OSL Mail** — separate mail-specific identity/content/lifecycle product.
  `needs: A,B,D contract` `weight: 6` `earned: 0`
- ⬜ **E7 · Versioned public support matrix** — site/app status matches exact live evidence.
  `needs: E2-E6` `weight: 3` `earned: 0`

## F · Scrub and AutoScrub — 45 points (17 earned)

- 🟨 **F1 · Local all-browser/all-profile detection** — **correction 2026-07-27:** exact
  `ae9d5a1` withdraws the earlier +1 because it rounded source wiring and native tests into a live
  UI/revocation result. The seeded Windows Brave test still makes native reader default-deny and
  one-profile scoping `runtime-proven`, including its `allows() → true` negative control. Direct
  native grant/revoke persistence is `test-proven-only`. The renderer tests parse choices and
  inspect `main.ts` strings; they do not execute the picker, Tauri grant/revoke handlers, or a
  persisted UI revoke/re-read. No exact-build live Windows UI/IPC walkthrough exists, and exact VM
  capture remains `blocked`. Exact `be5355d` has coherent source but its acceptance harness cannot
  prove Tauri IPC, durable reread or commit-bound executable identity and accepts unknown receipt
  versions/fields; it is not a runtime positive. Re-award only after picker
  → grant IPC → nonempty import → revoke IPC → persisted restart re-read succeeds on the exact
  build. Browsers beyond the proved Chromium path, Firefox tier, and exact release proof also
  remain. `needs: none` `weight: 6` `earned: 4`
- 🟨 **F2 · Detected sites/accounts and ownership** — broader findings and native-app account work
  landed in dirty integration. Exact `c5b516f` makes the strict identity-binding verifier
  code-ready with 24 mutation-sensitive tests, but no authorised real-VM five-frame walkthrough has
  passed. Exact `fedd9e2` retains stronger VM facts, but independent grading classifies the pair as
  narrow `runtime-proven` harness diagnostics rather than an F2 positive. Product runtime remains
  `blocked`; the exact walkthrough is still needed.
  `needs: F1` `weight: 5` `earned: 3`
- 🟨 **F3 · Free Scrub account/category/scan/review flow** — substantial contracts/UI; exact
  `07384f67a829398527ed869034789304c7e74d87` is independently accepted
  `test-proven-only` for UI → Tauri → main-only ACL → native IMAP authority. No production caller
  arms `authorize_attended_imap_batch_reviewed`, and no live fixture ran; end-to-end proof remains
  and F3 is not code-complete. `needs: F2` `weight: 5` `earned: 3`
- 🟨 **F4 · Attended per-target deletion and verification** — coverage expanded; live provider
  execution/receipts need qualification. `needs: F3` `weight: 5` `earned: 2`
- 🧪 **F5 · Native-app/hosted-session Scrub port** — architecture/hosting foundations; adapters and
  safe execution incomplete. Exact `f0bd0e1` rejects, rather than lands, the proposed isolated
  scan-only commit: its exact tree fails typecheck and the passing exploratory tests were removed.
  Status remains `blocked`, not `test-proven-only` for that candidate.
  `needs: F2,F3` `weight: 5` `earned: 1`
- 🟨 **F6 · Pro AutoScrub native authority** — consent/tier/manifest/rails active in separate tree;
  transport-scoped background behavior and global stop/status need proof. `needs: F3,F4` `weight: 6`
  `earned: 2`
- ⬜ **F7 · Optional proprietary module boundary** — separate install/consent, open build, network
  proof, licensing/package lifecycle. `needs: F6` `weight: 4` `earned: 0`
- ⬜ **F8 · Optional cloud AutoScrub** — honest high-sensitivity consent, isolation, wiping, receipt,
  credential revocation. `needs: F6,security review` `weight: 3` `earned: 0`
- 🧪 **F9 · Website username-only hook** — Worker scaffold and research exist; deployment/demo and
  calibrated discriminators remain. `needs: website contract` `weight: 3` `earned: 1`
  Required scaffold test: `test/integration/username-only-worker-scaffold.test.ts`.
  Behavioral name: `test/integration/username-only-worker-scaffold.test.ts`.
  Pass condition: the website repo is located unambiguously, the username-only Worker entrypoint is
  present, the public request contract accepts only a username/handle-shaped input, and the response
  matches the frozen F9 schema with honest coverage/status fields and no invented account-wide
  results. Negative controls must refuse missing username, credential-like input, unsupported
  provider/account bindings, and any response fixture that reports deletion, private mailbox access,
  browser-profile access, or calibrated risk percentages without supporting evidence.
  Fail condition: a marketing-only page, mock-only JSON, unlocated repo, unbound Worker route,
  permissive credential input, or schema drift counts as unimplemented, not partial proof.
  Current scaffold confirmation:

  ```json
  {
    "schemaVersion": 1,
    "test": "test/integration/username-only-worker-scaffold.test.ts",
    "websiteRepo": {
      "path": "/home/liamw/projects/oslprivacy-web",
      "role": "static_pages_checkout",
      "located": true
    },
    "workerScaffold": {
      "repo": "this_worktree",
      "route": "POST /v1/username-coverage",
      "entrypoint": "keyserver-cf/src/endpoints/username-coverage.ts",
      "router": "keyserver-cf/src/index.ts",
      "responseVersion": 1
    },
    "requestContract": {
      "exactBodyKeys": ["username"],
      "acceptedUsernameExamples": ["alice.example_1"],
      "refusedInputs": [
        "missing_username",
        "extra_provider",
        "credential_like_input",
        "unsupported_provider_binding",
        "discord_snowflake"
      ]
    },
    "responseContract": {
      "resultStatus": "not_scanned",
      "signalsEmpty": true,
      "forbiddenClaims": [
        "deletion",
        "private_mailbox_access",
        "browser_profile_access",
        "calibrated_risk_percentage"
      ]
    },
    "status": "scaffold-confirmed-test-proven-only"
  }
  ```
- 🟨 **F10 · Scrub tests/receipts/status projection** — many tests exist; full real-account,
  challenge/stop/restart matrix and simple reporting remain. `needs: F1-F9` `weight: 3`
  `earned: 1`

## G · Notes and Creative minimum — 15 points (5 earned)

- 🟨 **G1 · Encrypted local notes/storage foundations** — extensive specialized source exists.
  `needs: A storage` `weight: 5` `earned: 3`
- 🟨 **G2 · Basic notes/link/search/file UX** — partial specialized work; Hub integration unproved.
  `needs: G1` `weight: 4` `earned: 1`
- 🟨 **G3 · Encrypted file sharing and safe import/export** — foundations; release path unproved.
  `needs: D/B6` `weight: 3` `earned: 1`
- ⬜ **G4 · Integrate minimum milestone; label rest Coming soon** — exact app/release proof.
  `needs: G1-G3` `weight: 3` `earned: 0`

## H · Website — 25 points (11 earned)

- 🟨 **H1 · One canonical branch/deployment and one pricing model** — **pricing decided 2026-07-26**
  (master 7.14: prepaid one-month $5 code, period starts at redemption, nothing stored, separate
  one-time compute credits). Canonical production line is GitHub `main`; a prior observation used
  the `?v=` asset stamp, but the current live Pages SHA remains `unknown`. Single manifest
  `data/pricing.json` now **drives every surface**: 18 markers across all 16 pages,
  `pricing-sync --check` reports 0 drift, and `check-claims` reports 0 conflicting
  price/renewal/entitlement claims on every page. The allowlist A6 limitation ("Nothing renews, and
  OSL never stores your payment details") now ships on `index.html`, which previously showed two
  `$5 / month` checkout buttons with no renewal disclosure at all, and is pinned in
  `required_phrases` so it cannot regress. Build-identity stamp and claim crawler are done on branch
  `web-pricing-truth-2026-07-26`. **The remaining point is held for actual promotion to production
  plus the keyserver redemption change** — nothing is deployed. The "your month starts when you enter the code" claim stays
  unpublished until the keyserver redemption change lands.
  **Independent reviews rejected local `f39b805`, `5a84d77`, `4aea9fe`, and `be16e7c`. The latest
  rejection proves product-key/redemption-token, entity/inline-markup, and forward/reverse
  cross-sentence bypasses plus planned/provided limitation false positives. Exact successor
  `a409fb1d951278f8bcb72be77689d52dfc34e4f5` uses decoded rendered text and bounded bidirectional
  adjacent context. It passes 16/16 public pages, 50/50 crawler known-bad/honest fixtures, 18
  pricing markers with zero drift, and 10/10 checkout tests. It is a clean `test-proven-only`
  commit pending independent acceptance, nine commits ahead of the local `origin/main` ref,
  unpushed and undeployed.
  Keyserver `90da747` fails closed at paid issuance boundaries in local source but intentionally
  implements no redemption clock and is not deployment-proven. Pages dashboard/build evidence and
  a public SHA-bound `/build.json` remain `unknown`; redemption is absent.**
  `needs: keyserver redemption period (DEC-2026-07-26-PRO-CODES)` `weight: 4` `earned: 3`
- 🟨 **H2 · Responsive visual fixes from Zhao/Jester screenshots** — preview branches contain newer
  work; exact screenshot mapping and canonical integration remain. `needs: screenshot refs`
  `weight: 3` `earned: 1`
- ⬜ **H3 · Phone username-only Scrub demo** — clear boundary, honest coverage receipt, desktop CTA.
  `needs: F9 contract` `weight: 4` `earned: 0`
- 🟨 **H4 · PWS and Burn explanation/animation** — exact local website commit
  `a52e9b30e4ced423274a7f15e1f3c495d7ea330a` provides one coherent accessible explanation:
  PWS acts before disclosure, Burn acts after disclosure, both are visibly **Planned**, and all four
  Burn boundaries plus “not cryptographic erasure” remain explicit. Independent exact-archive
  review passed 16/16 pages and 54/54 claim fixtures, 270/270 responsive/JS-off/reduced-motion
  captures, and 120 accessibility combinations; removing either the erasure limitation or the
  unavoidable-copies/screenshots boundary made the gate fail. This is `test-proven-only`, unpushed,
  and undeployed; the final point remains held for public promotion and live identity verification.
  `needs: production promotion` `weight: 3` `earned: 2`
- 🟨 **H5 · Messenger/email comparison and sources** — **the versioned support matrix half is done**:
  `/docs/status` is generated from `data/pricing.json` (so it cannot drift), is dated, and carries
  per-connector protected send, protected receive, attachments, Scrub, verification date,
  provider-policy risk and status, plus a per-capability table covering all 21 registry entries.
  `build-status --check` fails if the committed page is stale, and `check-claims` fails if the matrix
  omits any capability. Remaining: the honestly sourced messenger/email comparison itself, with OSL
  weaknesses at the same prominence as competitors' and primary sources with last-reviewed dates.
  `needs: research,E7` `weight: 4` `earned: 1`
- ⬜ **H6 · Website retrospective score; app live score distinction** — transparent components, no
  fake live scan. `needs: product model` `weight: 2` `earned: 0`
- 🟨 **H7 · DeleteMe research/comparison** — exact local website chain ending at
  `71c9420583c9da02cd3cfcc55abaf6730c82355b` adds a versioned US-consumer
  DeleteMe/Abine research manifest with exactly seven dimensions and 15 official sources. Facts
  carry source IDs, dates, confidence and qualifiers; scope conflicts and genuine unknowns remain
  explicit; every comparison binds stable OSL capability IDs to the exact current `Planned`,
  `sellable:false` limitations. The ordinary claim gate and self-test both enforce the manifest:
  154/154 fixtures pass, exactly 20 named mutations execute once and fail for their intended
  reasons, 16/16 public pages pass, and the final independent 12/12 evidence/public-note
  replacement-and-append audit passes. Two independent exact-object reviews accept **1/2**.
  This is `test-proven-only`, unpushed and undeployed; the remaining point requires maintained
  source refresh/public comparison evidence and does not follow from competitor claims alone.
  `needs: maintained source refresh/public comparison` `weight: 2` `earned: 1`
- 🟨 **H8 · Accessibility/motion/responsive/claim test matrix** — the responsive and claim halves are
  now evidenced: a 252-capture matrix (14 pages × 320/360/390/768/1024/1440 px × JS-on/JS-off/
  reduced-motion) reports 0 failed and 0 unmeasurable, with meaningful content visible before scroll
  and with JavaScript disabled on every page (worst case 154 visible characters), plus the claim
  crawler in J7. **Restored to 3 later the same day**, after the gap it was docked for was closed:
  `scripts/check-a11y.mjs` now audits 15 pages × 4 widths × {100%, 200% zoom} = 120 combinations and
  reports **0 images missing alt, 0 controls without an accessible name, and 0 horizontal overflow at
  any width or zoom**. Two real bugs were found and fixed by it: `/donate` scrolled sideways at 768px
  because the `.mission-*` illustration has no CSS rules anywhere in the stylesheet, and `/audit`
  buttons were 40.8px tall because `assets/css/audit.css` loads after `style.css` with an id-bearing
  selector, so no class-only rule could reach them. **The 44×44 residual is now closed** — nav/footer links carry
  `min-width: 44px`, and the audit reports **0 images missing alt, 0 controls without a name,
  0 tap targets under 44px, 0 horizontal overflow** across all 120 combinations. Still unasserted,
  recorded not hidden: non-colour-only state communication, and whether every illustration has an
  equivalent text description (the audit proves no image lacks `alt`, which is weaker). The claim
  crawler is also explicitly a string/status gate: its clean result does not prove that code named
  by a truthful sentence has a production caller.
  `needs: H1-H7` `weight: 3` `earned: 3`

## I · Release, repository, and infrastructure — 20 points (6 earned)

- 🛑 **I1 · Preserve/reconcile dirty worktrees** — many unique overlapping changes; no cleanup/merge
  before semantic inventory. `needs: none` `weight: 4` `earned: 0`
- 🟨 **I2 · Authoritative integration line and exclusive central-file waves** — **+1 2026-07-26,
  release lane claimed 1, approved.** `main` is now genuinely protected, verified by the writer via
  `gh api repos/OSLPrivacy/discord-privacy-client/branches/main/protection`: force pushes and
  deletions are both disabled and status checks are required. That is an authoritative integration
  line where there was none. **Recorded gap:** the required contexts are only
  `["TypeScript gate", "audit"]` — **the Rust gate is not required** — and `enforce_admins` is
  `false`, so the protected line can still take a merge while Rust is red, and an admin can bypass
  it. The exclusive central-file wave discipline is process, not proven. `needs: I1` `weight: 3`
  `earned: 1`
- 🛑 **I3 · Green public Rust/TypeScript/selector/security CI** — **+1 2026-07-26; the lane proposed
  +2 and the writer cut it to +1.** Three of the four named gates are genuinely green and a new
  desktop-binary job exists — real work. But **the Rust gate is red**, verified independently by the
  writer with `gh run list`: `Rust Test` = `failure` on the four most recent runs
  (2026-07-27 00:11, 00:20, 00:28, 00:40), all on `release-lane-2026-07-26` rather than `main`.
  The row's headline promise is *green* CI **including Rust**. Scoring 2 of 3 would read as
  "nearly satisfied" when the heaviest gate fails on every run, so the remaining 2 points are held
  until Rust is green. **2026-07-27 no-point adjudication:** `b9aa48e` locally wires and
  refusal-tests the keyserver test-count floor, but the clean release worktree is one commit ahead
  of origin and the edited workflow has no GitHub-hosted run. That is `test-proven-only`, not green
  public CI. Exact later `caa4ba9` is also only `test-proven-only` structural policy: its local
  administrator evidence is forgeable, no integrated candidate exists, and no exact-SHA hosted
  result, push or promotion occurred. Exact `474c629` adds mutation-sensitive local
  QA-registration/window-targeting checks and is `test-proven-only`; it has no hosted run or
  integrated candidate, and release use remains blocked by separately audited assembler defects.
  Current later release edits are dirty and `unknown`. The
  uncommitted phase-two verifier packet rejects its complete fixture mutations, but it was deleted
  after the read-only exercise and is design evidence only. The tracked preflight still inspects
  only the first Rust/TypeScript workflow-name match, ignores duplicate names and named jobs, binds
  local HEAD instead of the release tag, and treats a missing `hub-vm-qa` environment as a warning.
  The three Rust defects are not owned by that lane, which is why this is a cut and not a criticism.
  `needs: integration` `weight: 3` `earned: 1`
- 🟨 **I4 · Signed candidate, VM promotion, reproducible release, rollback** — **+2 2026-07-26,
  approved as claimed. The strongest evidence in this batch.** The promotion gate was proven by
  **refusal**, not by a happy path: `scripts/release/prove-promotion-gate.sh` contains **20
  `expect_reject` cases**, all verified present by the writer, including "attestation describes a
  different binary" (substituted binary), `mutate d["operator"] = "   "` → "no accountable operator"
  (blank operator), "CAPTCHA was automated around", "same golden snapshot restored twice",
  "clean restore attested as a string, not a boolean", and "no installer at all". A gate proven to
  refuse every tested way of promoting an untested or substituted build is a real safety guarantee.
  Rollback has `scripts/release/prove-rollback-guards.sh` and `.github/workflows/osl-hub-rollback.yml`.
  **Held back:** "signed candidate" is not proven *end to end*. Refined 2026-07-26: the updater key
  material is real and is **not** a placeholder — `apps/osl-hub/tauri.conf.json:50` decodes to
  minisign public key `3B6AE4739858E8D4` and `src-tauri/tauri.conf.json:39` to `44AD89E36BC119F8`,
  correctly different for two feeds, verified by the writer. What is still missing is a signed
  release artifact demonstrated on a real candidate, and `required_signatures` is `false` on the
  protected branch — and the reproducible build has not been
  shown to actually reproduce. Exact `6cc103b` makes VM surface binding fail closed and catches a
  real whole-desktop/6×6-marker false green, but its final positive is blocked by invisible resize
  borders and yields no capture artifact; it is not promotion evidence.
  `needs: I2,I3` `weight: 4` `earned: 2`
- ⬜ **I5 · Public docs/site truth reconciliation** — claims match exact binary.
  `needs: all release features,H` `weight: 2` `earned: 0`
- 🟨 **I6 · Repo governance/branch protection/PR cleanup/releases** — **held at 1; the lane proposed
  full marks and the writer declined.** Branch protection is genuinely enabled (see I2), which is a
  real action. The stale PR cleanup half is now explicitly disposed: PRs #1-#6 were closed without
  merge on 2026-07-30 after the preserved-work inventory and branch-protection prerequisites were
  rechecked. Releases are still untouched: `gh release list` returns **nothing at all — zero
  releases**. Full marks still wait for the release half of the row.
  <!-- i15-pr-disposition:start -->
  | PR | Title | Disposition | Rationale |
  |---:|---|---|---|
  | #1 | Harden Tauri remote capabilities | Closed without merge | Long-stale branch; preserved-work inventory is the retained source for any reusable changes. |
  | #2 | Clean up stale docs and test warnings | Closed without merge | Long-stale branch; preserved-work inventory is the retained source for any reusable changes. |
  | #3 | Upgrade keyserver Fastify dependencies | Closed without merge | Long-stale branch; preserved-work inventory is the retained source for any reusable changes. |
  | #4 | Remove Discord password settings surface | Closed without merge | Long-stale branch; preserved-work inventory is the retained source for any reusable changes. |
  | #5 | Scrub: onboarding redesign + session-reuse-first deletion engine | Closed without merge | Superseded by the current integration line; preserved-work inventory is the retained source for any reusable changes. |
  | #6 | Make CI green and prove the release gate can refuse | Closed without merge | Superseded by enforced branch protection and current release-gate work; preserved-work inventory is the retained source for any reusable changes. |

  Post-disposition check: `gh pr list --state open --json number --jq length` = **0**.
  Local guard: `python3 scripts/verify_i15_pr_cleanup.py`.
  <!-- i15-pr-disposition:end -->
  `needs: I2` `weight: 2` `earned: 1`
- 🟨 **I7 · Safe storage cleanup/archive** — policy exists; cleanup not authorized/executed.
  `needs: I1,I2` `weight: 2` `earned: 1`

## J · Agent/test/mirror operating infrastructure — 23 points (15 earned)

- ✅ **J1 · Master control spec, derived views, design-feel guide, and tab prompts** — created and
  internally reconciled; ongoing tasks must keep them current. `weight: 4` `earned: 4`
- 🟨 **J2 · Off-screen virtual-display launch, self-test, acceptance harness, recorder** — working
  temporary/current tools; exact `fedd9e2` strengthens retained executable/script/HWND/DWM/PNG and
  cleanup binding, but independent grading makes it narrow `runtime-proven` diagnostics, not a
  product acceptance positive. Exact later `2da489a` is rejected for VM execution: its live agent
  cannot emit the accepted V2 verdict, and source/executable/build plus Azure provenance remain
  forgeable despite green synthetic tests. Durable repo packaging and the full capsule matrix remain.
  `weight: 5` `earned: 3`
- 🟨 **J3 · Multi-Codex account/usage routing** — codex/codex2/codex3/switch/usage exist; ongoing
  update resilience required. `weight: 3` `earned: 3`
- ⬜ **J4 · Safe multi-Claude account routing** — design/official mechanism/concurrency test needed.
  `weight: 2` `earned: 0`
- 🧪 **J5 · Telegram `/osl` mode, alerts, hierarchy, progress/ETA, suggestions** — 20 focused tests
  pass and new code is loaded; owner activation and live proof remain, with registry import and UX
  polish deferred. `weight: 4` `earned: 0`
- ✅ **J6 · Shared memory/wiki/compact reports/trap ledger** — versioned compact memory-card rules,
  current-window prompts, and the trap ledger maintenance rule are now recorded in the operating
  docs. After each wave, agents must keep compact reports current, update only durable memory-card
  fields whose truth changed, store every future-deadline with an absolute date/time zone, and add
  or retire trap entries only when the short ledger saves real rediscovery cost. Adoption across
  every active account remains tracked by J23, so this row owns the rule staying current rather than
  account-by-account rollout evidence. J24 acceptance guard: **Keep compact reports and trap-ledger
  rules current after each wave.** A wave is not closed until its compact report is current for that
  wave and says what changed, what evidence was run, what remains blocked, and whether the trap
  ledger was updated, left unchanged because no durable trap changed, or intentionally pruned.
  `weight: 2` `earned: 2`
- 🟨 **J7 · Public-claim allowlist** — closes master §24 item 5. Every permitted website/app wording
  is now bound to a status label, `file:line` evidence, and a mandatory limitation, with an explicit
  NOT-ELIGIBLE list, in
  [`osl-public-claim-allowlist.md`](osl-public-claim-allowlist.md). A feature can no longer become
  `Available` through a copy edit. **The crawler now exists** (`scripts/check-claims.mjs` in the website repo): it asserts that no page
  contains a forbidden phrase and that every badge matches the manifest, and it carries a known-bad
  fixture suite (`--self-test`; exact website `a409fb1` passes 50/50 known-bad/honest fixtures,
  including redemption-record/start/expiry, decoded entities, split inline markup,
  code/key/voucher/token/product-key synonyms, forward/reverse bounded context, and honest
  planned/unimplemented/conditional controls) so it
  cannot decay into an all-green source-shape test. It caught real live false
  claims on 2026-07-26. **Scope addition 2026-07-26: +3 points to the denominator.** Residual gap:
  it is a pre-deploy command, not a CI gate, so nothing yet blocks a deploy that skips it.
  It does **not** prove Rust/Tauri reachability; the separately recorded zero-caller seam passed
  this gate by construction. That limit does not withdraw a point from this row because a call-graph
  gate is not J7's acceptance boundary. `needs: none` `weight: 3` `earned: 3`

## Head developer / website suggestions

These are good independent Jester scopes once their listed dependency is ready:

1. Canonical branch/build identity and pricing-manifest plumbing—waiting on Zhao's pricing decision.
2. Responsive username-only Scrub demo—use F9's frozen response schema; do not invent results.
3. PWS/Burn explainer and animation—use A/D truth; include static/reduced-motion version.
4. Screenshot-specific visual pass—requires the referenced photos attached to the task.
5. Comparison-page framework and citations—facts reviewed separately before publishing rankings.
6. Deterministic responsive/JavaScript-off/motion screenshot test suite—independent now.

Every handoff includes commit/branch, preview URL, exact changed files, screenshot matrix, known
limitations, and what is safe to merge.

## Update protocol

When a task changes intent/status/dependencies:

1. update the master first;
2. update this row's icon, plain-language status, `needs`, weight/earned evidence, and critical path;
3. update the layman spec only if the user-facing explanation changed;
4. update the Telegram `/osl` projection and recompute progress/ETA;
5. link the detailed report/evidence;
6. after every wave, close it only after its compact report is current and records exact changed
   files, tests/evidence, blockers, and trap-ledger disposition (`updated`,
   `unchanged-no-durable-trap-change`, or `pruned`);
7. never mark ✅ from agent confidence or unit tests alone.

J24 guard: compact reports and trap-ledger maintenance are live wave-close requirements, not
optional notes. A wave closure is incomplete unless its compact report names the exact changed
files, the evidence actually run, remaining blockers, and one of the three trap-ledger dispositions
above.
