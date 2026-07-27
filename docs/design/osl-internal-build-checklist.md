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
Provisional verified progress: 96 / 303 points = 32%   (was 85 / 303 = 28%, which understated section H by 2)
Confidence: low (several dirty concurrent worktrees and exact-build rechecks remain)
Critical path:
security identity/attribution → reviewed ratchet → two-identity proof
→ finish Discord adapter contract → qualify every offered app → integration/release

Parallel paths:
Scrub/AutoScrub · website truth/demo · Notes minimum · test/mirror infrastructure

Owner-blocked: keyserver migration 0027 deploy state (B5) — blocks Tab 5.
```

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
**2026-07-26 — recorded, deliberately NOT awarded.** Open defects earn nothing; they are
logged here so they cannot be quietly forgotten or later re-counted as new work.

- **Control-inbox retry bound (crypto lane, open).** The revocation/control lane must behave as a
  **retryable queue, not a dropped notice** — a 404 or absent lane has to be retried, not discarded.
  Related to the already-recorded D6 defect where the drain DELETEs an inbound revocation frame
  without applying it. No point until a bound is implemented and exercised.
- **Windows duress defect (open).** The duress/auto-lock path is unreachable in production
  (`crates/keystore/src/duress.rs` has no production caller) and has a known TPM-less Windows
  failure mode. A7 already sits at 0 for exactly this reason; nothing here changes it.
- **Cryptographic burn is not implemented** (`crates/store/src/lib.rs`: `put` never populates
  `wrapped_key`). Owner decision 2026-07-26: **NOT NOW**, revisit after the deadline. The threat
  model and the public-claim allowlist already ban the phrase, so this is an architectural gap, not
  a live false claim.
- **Received/Opened receipts collapse into one `acknowledgmentCount`** (`broker.rs`), losing order —
  the operator cannot distinguish delivered from read.

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

## A · Identity, trust, storage, and security — 40 points (8 earned)

- 🟨 **A1 · Local OSL identity and recovery** — create/import/unlock exists; recovery/capture and
  password-at-rest claims need reconciliation. `needs: none` `weight: 6` `earned: 3`
- 🛑 **A2 · Full-bundle identity binding** — identity must authenticate Ed25519, X25519, ML-KEM and
  capability bundle; no keyserver substitution. `needs: A1` `weight: 6` `earned: 0`
- 🛑 **A3 · Sender authentication equals displayed attribution** — no caller-supplied identity can
  relabel authenticated plaintext. `needs: A2` `weight: 5` `earned: 0`
- 🛑 **A4 · Proven platform-account registration** — nobody can pre-register another owner's public
  service ID. `needs: A2` `weight: 4` `earned: 0`
- 🟨 **A5 · Friends, safety numbers, scoped trust, held key changes** — foundations exist; full
  ceremony and all call paths need proof. `needs: A2` `weight: 5` `earned: 2`
- 🛑 **A6 · No protected plaintext at rest** — attachment staging and metadata findings remain.
  `needs: none` `weight: 5` `earned: 0`
- 🛑 **A7 · Honest Burn/duress and retry** — failed/skipped wipes, TPM errors, rollback, and key
  handlers must be fail-closed/retryable. **Recheck 2026-07-26 removed the earned point:** duress
  has zero production callers (`crates/keystore/src/duress.rs` — only the
  `crates/keystore/src/lib.rs:58-59` re-export and keystore tests reference the password/duress
  API), so the 10-attempt auto-burn and 15-minute auto-lock are not user-reachable; and burn is
  not cryptographic — `MessageStore::put` (`crates/store/src/lib.rs:194-206`) never writes
  `wrapped_key`, so every `wrapped_key = NULL` on the burn paths nulls an already-null column.
  `needs: A1` `weight: 5` `earned: 0`
- 🟨 **A8 · Secret zeroization and metadata minimization** — **+1 awarded 2026-07-26 by the checklist
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
  wipe would be undefined behaviour. Not full marks, because master §10 medium 9
  also covers plaintext identifiers **at rest**, and no comprehensive at-rest audit was delivered.
  `needs: none` `weight: 4` `earned: 3`

## B · Encryption and two-identity communication — 30 points (6 earned)

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
  and sealed. `needs: B2` `weight: 5` `earned: 1`
- ⬜ **B4 · Capability negotiation and monotone downgrade pin** — no silent fallback after a peer
  proves stronger support. `needs: B2` `weight: 4` `earned: 0`
- ♻️ **B5 · Keyserver/prekey/control-inbox production contract** — `unknown-recheck-required`.
  Three repository sources disagree on whether migration 0027 is deployed and none names a Worker
  version or deploy timestamp; remote D1 state is external to this checkout. **Escalated to the
  owner** — one read-only command in Conflict C2 of
  [`osl-completion-plan-2026-07-26.md`](osl-completion-plan-2026-07-26.md) resolves it.
  Blocks Tab 5. `needs: A2` `weight: 4` `earned: 1`
- ⬜ **B6 · Controlled two-identity proof** — handshake, send, receive, offline queue, restart,
  drain, peer attribution. `needs: B3,B4,B5` `weight: 3` `earned: 0`
- ⬜ **B7 · Independent crypto review** — required before uncontrolled traffic/public superiority
  claims. `needs: B2-B6` `weight: 3` `earned: 0`

## C · Discord reference adapter — 35 points (18 earned)

- ✅ **C1 · Carrier generation and encrypted commit** — exercised in live QA sends.
  `needs: B1/B2 gated path` `weight: 4` `earned: 4`
- ✅ **C2 · Native composer locate/write/full readback** — live multi-line exact writes proved on
  dated QA builds. `needs: none` `weight: 4` `earned: 4`
- 🟨 **C3 · Three send-authority modes** — existing automation work does not yet prove the locked
  clipboard/default, double-Enter, and single-Enter product contract. `needs: C2` `weight: 4`
  `earned: 1`
- 🟨 **C4 · Production tri-state sent proof** — sends have landed while receipts said failure;
  duplicate-safe production proof needs exact-build verification. `needs: C3` `weight: 4`
  `earned: 1`
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

- 🟨 **D1 · Structure-compatible text/image/video/file carriers** — text shaping works in QA;
  literal line/size/file-type parity and host transformations need proof. `needs: C contract`
  `weight: 5` `earned: 2`
- 🛑 **D2 · Encrypted attachment transport** — **status marker corrected 2026-07-26 from 🧪 to 🛑;
  the point is NOT withdrawn, and the reason matters.** Attachment upload was **broken in
  production, and had been before today**: the body was piped through a `TransformStream`, and R2
  requires a known length, so **every upload failed**. It passed its tests only because the R2 test
  double accepts any stream — a textbook false green, the same family as a harness that confirms
  whatever it happens to find. The server side is now fixed **and deployed**: cipher-store version `0a17547d`, production
  re-probed, part upload returns 201 where it previously returned HTTP 500. **Apparent conflict
  reconciled (master §0.2):** `docs/reports/server-lane-2026-07-26.md` states "nothing here is
  `verified-live`. No deploy has happened", while the owner reports deploying `0a17547d` and
  re-probing production. Both are true at different scopes — the lane is describing what *its own
  work* proved (a real local workerd, not production), and the owner deployed separately. Not a
  contradiction, so nothing is marked `superseded`. The writer did **not** independently probe
  production; the 201 rests on the owner's report (authority order item 1), and the lane's own
  ceiling remains `runtime-proven` (local). The single earned point stands because it was
  awarded for substantial source existing, **not** for uploads succeeding; withdrawing it would be
  over-correction. But 🧪 "test-proven" was an untrue label for a path that could not execute, so
  the marker is corrected. **Treat any "attachment sent successfully" reported anywhere before
  2026-07-26 as unproven.** Pending cover handoff, media measurement, cleanup, and
  no-plaintext-at-rest all remain. `needs: A6,D1` `weight: 5` `earned: 1`
- 🧪 **D3 · View-once text** — mechanisms exist; two-identity second-open refusal unproved.
  `needs: B6` `weight: 4` `earned: 1`
- 🧪 **D4 · View-once image/protected viewer** — viewer/link foundations exist; first-paint
  protection and end-to-end path unproved. `needs: D2,B6` `weight: 4` `earned: 1`
- 🧪 **D5 · Timed deletion** — scheduler/ledger pieces exist; production wiring and all lifecycle
  outcomes unproved. `needs: B6,D2` `weight: 4` `earned: 1`
- 🛑 **D6 · Bilateral Burn** — control-lane work exists but is **inert with an open defect**, so
  the earned point is withdrawn. The text/receipt drain recognises an inbound revocation frame and
  DELETEs it from the control inbox without applying it
  (`apps/osl-hub/src/broker.rs:2635-2640`); `apply_peer_revocation` and friends still have zero
  callers outside `apps/osl-hub/src/security.rs`. Because the row is destroyed rather than skipped,
  wiring the apply path later will not recover burns issued in the meantime. Corrected description
  in [`../qa/two-identity-p2p-verification.md`](../qa/two-identity-p2p-verification.md) §6 item 4;
  Tab 5 owns the fix. Authenticated offline/retry/peer receipt proof and security audit still
  required. `needs: A7,B6` `weight: 4` `earned: 0`
- 🧪 **D7 · Receipts** — sent/received/opened/deleted/expired outcomes must be separate, mutual where
  required, and evidence-bound. `needs: B6,D3-D6` `weight: 4` `earned: 1`

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

## F · Scrub and AutoScrub — 45 points (18 earned)

- 🟨 **F1 · Local all-browser/all-profile detection** — **+1 2026-07-26, claimed by the Scrub lane and
  approved by the checklist writer.** The row's outstanding scope was "remaining browsers, UI,
  revocation, exact release proof"; this closes the **UI and revocation** halves at runtime.
  Root cause it fixes: `set_browser_profile_consent` and `list_browser_profile_choices` were
  registered in **no** Tauri command, so detection returned zero in any shipped build — which
  explains the standing "0 accounts detected" blocker. Verified independently by the writer:
  both are now in `generate_handler!` (`apps/osl-hub/src/main.rs:4192-4193`) with matching ACL in
  `capabilities/hub.json` (`allow-list-browser-profile-choices`, `allow-set-browser-profile-consent`)
  and `permissions/hub.toml:474` — no registration/ACL mismatch in either direction. Runtime proof on
  Windows against a real Brave tree, `seeded_profile_is_read_only_after_consent`
  (`apps/osl-hub/src/native_apps.rs:6960`): default-deny yields zero, one grant yields findings, and
  the real `Default` profile was never read. **A negative control was performed** — forcing `allows()`
  to always grant makes the test fail on the default-deny assertion — which is what makes this proof
  rather than a green light. *Note: the lane's report calls the second command
  `list_browser_profile_candidates`; the actual name is `list_browser_profile_choices`.*
  Remaining point: browsers beyond the Chromium path, the Firefox tier, and exact release proof —
  explicitly not claimed. `needs: none` `weight: 6` `earned: 5`
- 🟨 **F2 · Detected sites/accounts and ownership** — broader findings and native-app account work
  landed in dirty integration; exact walkthrough needed. `needs: F1` `weight: 5` `earned: 3`
- 🟨 **F3 · Free Scrub account/category/scan/review flow** — substantial contracts/UI; exact
  end-to-end proof remains. `needs: F2` `weight: 5` `earned: 3`
- 🟨 **F4 · Attended per-target deletion and verification** — coverage expanded; live provider
  execution/receipts need qualification. `needs: F3` `weight: 5` `earned: 2`
- 🧪 **F5 · Native-app/hosted-session Scrub port** — architecture/hosting foundations; adapters and
  safe execution incomplete. `needs: F2,F3` `weight: 5` `earned: 1`
- 🟨 **F6 · Pro AutoScrub native authority** — consent/tier/manifest/rails active in separate tree;
  transport-scoped background behavior and global stop/status need proof. `needs: F3,F4` `weight: 6`
  `earned: 2`
- ⬜ **F7 · Optional proprietary module boundary** — separate install/consent, open build, network
  proof, licensing/package lifecycle. `needs: F6` `weight: 4` `earned: 0`
- ⬜ **F8 · Optional cloud AutoScrub** — honest high-sensitivity consent, isolation, wiping, receipt,
  credential revocation. `needs: F6,security review` `weight: 3` `earned: 0`
- 🧪 **F9 · Website username-only hook** — Worker scaffold and research exist; deployment/demo and
  calibrated discriminators remain. `needs: website contract` `weight: 3` `earned: 1`
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

## H · Website — 25 points (9 earned)

- 🟨 **H1 · One canonical branch/deployment and one pricing model** — **pricing decided 2026-07-26**
  (master 7.14: prepaid one-month $5 code, period starts at redemption, nothing stored, separate
  one-time compute credits). Canonical production line is GitHub `main`; production serves exactly
  `main` (verified by the `?v=` asset stamp). Single manifest `data/pricing.json` now **drives every surface**: 19 markers across all 15 pages, `pricing-sync --check` reports 0 drift, and `check-claims` reports 0 conflicting price/renewal/entitlement claims on every page. The allowlist A6 limitation ("Nothing renews, and OSL never stores your payment details") now ships on `index.html`, which previously showed two `$5 / month` checkout buttons with no renewal disclosure at all, and is pinned in `required_phrases` so it cannot regress. Build-identity stamp and claim crawler are done on branch `web-pricing-truth-2026-07-26`. **The remaining point is held for actual promotion to production plus the keyserver redemption change** — nothing is deployed. The "your month starts when you enter the code" claim stays
  unpublished until the keyserver redemption change lands.
  `needs: keyserver redemption period (DEC-2026-07-26-PRO-CODES)` `weight: 4` `earned: 3`
- 🟨 **H2 · Responsive visual fixes from Zhao/Jester screenshots** — preview branches contain newer
  work; exact screenshot mapping and canonical integration remain. `needs: screenshot refs`
  `weight: 3` `earned: 1`
- ⬜ **H3 · Phone username-only Scrub demo** — clear boundary, honest coverage receipt, desktop CTA.
  `needs: F9 contract` `weight: 4` `earned: 0`
- 🟨 **H4 · PWS and Burn explanation/animation** — safe FAQ text exists in parts; coherent accessible
  animation/page missing. `needs: D/A truth` `weight: 3` `earned: 1`
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
- ⬜ **H7 · DeleteMe research/comparison** — honest scope/data/verification/price analysis.
  `needs: research` `weight: 2` `earned: 0`
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
  equivalent text description (the audit proves no image lacks `alt`, which is weaker).
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
  until Rust is green. The three defects are not owned by that lane, which is why this is a cut and
  not a criticism. `needs: integration` `weight: 3` `earned: 1`
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
  shown to actually reproduce. `needs: I2,I3` `weight: 4` `earned: 2`
- ⬜ **I5 · Public docs/site truth reconciliation** — claims match exact binary.
  `needs: all release features,H` `weight: 2` `earned: 0`
- 🟨 **I6 · Repo governance/branch protection/PR cleanup/releases** — **held at 1; the lane proposed
  full marks and the writer declined.** Branch protection is genuinely enabled (see I2), which is a
  real action. But this row also names **PR cleanup** and **releases**, and neither happened:
  `gh pr list --state open` returns **6 open PRs** (#1–#6, several long-stale), and
  `gh release list` returns **nothing at all — zero releases**. Full marks while two of the four
  named items are untouched would be exactly the rubber-stamp this role exists to prevent.
  `needs: I2` `weight: 2` `earned: 1`
- 🟨 **I7 · Safe storage cleanup/archive** — policy exists; cleanup not authorized/executed.
  `needs: I1,I2` `weight: 2` `earned: 1`

## J · Agent/test/mirror operating infrastructure — 23 points (15 earned)

- ✅ **J1 · Master control spec, derived views, design-feel guide, and tab prompts** — created and
  internally reconciled; ongoing tasks must keep them current. `weight: 4` `earned: 4`
- 🟨 **J2 · Off-screen virtual-display launch, self-test, acceptance harness, recorder** — working
  temporary/current tools; durable repo packaging and full capsule matrix remain. `weight: 5`
  `earned: 3`
- 🟨 **J3 · Multi-Codex account/usage routing** — codex/codex2/codex3/switch/usage exist; ongoing
  update resilience required. `weight: 3` `earned: 3`
- ⬜ **J4 · Safe multi-Claude account routing** — design/official mechanism/concurrency test needed.
  `weight: 2` `earned: 0`
- 🧪 **J5 · Telegram `/osl` mode, alerts, hierarchy, progress/ETA, suggestions** — 20 focused tests
  pass and new code is loaded; owner activation and live proof remain, with registry import and UX
  polish deferred. `weight: 4` `earned: 0`
- 🟨 **J6 · Shared memory/wiki/compact reports/trap ledger** — versioned 36-line memory card and
  future-deadline/trap rules exist; adoption across every account still requires evidence.
  `weight: 2` `earned: 2`
- 🟨 **J7 · Public-claim allowlist** — closes master §24 item 5. Every permitted website/app wording
  is now bound to a status label, `file:line` evidence, and a mandatory limitation, with an explicit
  NOT-ELIGIBLE list, in
  [`osl-public-claim-allowlist.md`](osl-public-claim-allowlist.md). A feature can no longer become
  `Available` through a copy edit. **The crawler now exists** (`scripts/check-claims.mjs` in the website repo): it asserts that no page
  contains a forbidden phrase and that every badge matches the manifest, and it carries a known-bad
  fixture suite (`--self-test`, 14/14 fixtures caught, including the honest-negation cases that must
  *not* be flagged) so it cannot decay into an all-green source-shape test. It caught real live false
  claims on 2026-07-26. **Scope addition 2026-07-26: +3 points to the denominator.** Residual gap:
  it is a pre-deploy command, not a CI gate, so nothing yet blocks a deploy that skips it.
  `needs: none` `weight: 3` `earned: 3`

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
6. never mark ✅ from agent confidence or unit tests alone.
