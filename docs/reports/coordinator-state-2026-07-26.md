# Coordinator state — 2026-07-26 evening

Written so a fresh context can resume coordination without re-deriving it. This is the
*coordination* layer only: who owns what, what is decided, what is live, what is blocked. Product
authority remains `docs/design/osl-master-decision-2026-07-26.md` (currently **r16**), and the
scoreboard remains `docs/design/osl-internal-build-checklist.md` (**100 / 303**, authoritative
checklist commit `c52da2efac63a6c2be828d651da365d3b963ad21`).

## 2026-07-27 exact release-truth reconciliation — no new score

This table reconciles point-ready or near-ready exact objects reported after the independently
accepted H7 checkpoint at 99/303. It uses the row values frozen in `c52da2e`; A8 is already 4/4
and is not reconsidered here. Older 97/303 arithmetic retained later in this report is historical,
not the current scoreboard.

Status vocabulary in this table is strict:

- `test-proven-only` means exact committed source plus local static/unit evidence; it is not a
  named runtime, hosted CI run, deployed Worker, VM result, or release.
- `implemented-unwired` means relevant source exists but the required production caller or UI
  event does not.
- `pending-independent-audit` means only the author-side committed-object report exists.
- `REJECT` means an independent exact-object audit found a decisive false green or missing
  boundary; the author-side passing tests do not override it.
- `runtime-proven` and `verified-live` require named execution or deployed-state evidence. None of
  the new objects below reaches either tier.

| Exact object | Candidate checklist boundary | Committed file/test evidence | Independent verdict and remaining boundary | Score action |
|---|---|---|---|---:|
| Website `d158c88a188a6a006ea21c0a9fc0c630f513e91a`; checklist `c52da2efac63a6c2be828d651da365d3b963ad21` | A8 is already **4/4** | `scripts/check-claims.mjs`; public gate 16/16, self-test 419/419, inherited receiver corpus 38/38. The census binds 18 backends, nine claims and 44 source anchors. | Two exact website audits accepted the source/static object. The owner has frozen `c52da2e` at **100/303**. Runtime, release and A6 remain unverified. | Already accounted; **+0 here** |
| VM integrity precursor `b1e8c10a13622aa2afea39361ed5c4309e168ca5`, tree `48c75ceb9b9fc241f0ee95fbc6f8dfe028e8e65b` | C4 remains **1/4**; F1 remains **4/6** | Seven VMQA documentation/script/test paths. Exact archive ran Python 39/39 and shell grading 89/89; coherent executable/npm-log replacement, old-seal replay, producer mutation and starved evidence were refused by the applicable graders. | `ACCEPT` for code/fixture integrity only. The producer account and protected seal store were absent, so production creation refused and no Windows/VM runtime occurred. Status `test-proven-only`. | **+0** |
| VM provisioning `2279be3b789f4aadbcc35db57d6105ab820e3bf6`, tree `a9b9d5027f6123a4a1c4b012641ae2f7f43ec299` | C4 **1/4**; F1 **4/6** | Seven `scripts/vmqa/**` paths, including `vmqa_f1_provisioning_preflight.py`, `vmqa-f1-windows-provisioning-preflight.ps1` and their fixtures. Author gates reported provisioning 12/12, producer 27/27, PowerShell 26/26, inherited VMQA 40/40 and shell 89/89. | `REJECT`. Local-ready omitted Azure CLI/runtime, tenant/subscription/resource-group/VM, interactive session, warm lineage and cloud-build checks; PowerShell-absent positives false-greened, and no full `Invoke-WindowsProvisioningPreflight` positive existed. Two later bounded watches found no committed VMQA successor; current provisioning-plan paths are mutable/unaudited `unknown`, not evidence. No Azure or live VM evidence. | **+0** |
| Cipher-store lease `25d021b14f920566236d8ac0a21e4c41ba5fbdff`, tree `6998ad79fe6403916fac12a9bba31509a479f2ad` | D2 remains **1/5** | Ten `cipher-store-cf/**` paths, including migration `0008_attachment_sweep_claims.sql`, `src/lib/attachment-sweep-claims.ts`, `src/lib/sweep.ts` and concurrency fixtures. Author gates reported 23/23 pool-worker tests, 9/9 D2 closure tests and typecheck green. | `REJECT`. Completion can enter `completing` without fencing the sweep claim; the sweeper can delete the newly completed R2 object, after which completion can commit `ready` metadata referencing missing ciphertext. No completion-versus-sweep fixture or deployed migration/runtime evidence. | **+0** |
| Sender-filter successor `ab357f2b3ecde35114dd1ae9d16c1e5b8319154a`, tree `f5178994e4db78900f2b5d7e855f176f07c45aa5` | B5 remains **2/4** | Exactly `crates/keystore/src/sender_filter_rollout.rs`, `keyserver-cf/SENDER_FILTER_ROLLOUT.md`, and the rollout contract/test. Author gates reported 28/28 focused Vitest, TypeScript typecheck and Rust formatting. | `REJECT`, superseding the earlier pending label. Deleting both locally resolved floor records resets to `NeverObserved`; dead-branch and typed-delete-alias mutations false-green the regex closure; trusted producer and verifier-store authority remain public caller inputs. No deployment. | **+0** |
| Native row attribution `9c8d9caed5d11267054278589876717be1f822bc`, tree `0f183123b9a204126b1b601e63809eaa22431258` | A3 remains **0/5**; C5 remains **3/5** | Exactly `apps/osl-hub/src/native_discord_adapter.rs` and `apps/osl-hub-ui/src/carrier-replay-attribution.test.ts`. Author-side results: attribution Vitest 13/13, native-provider Rust 3/3 and replay/reorder/cross-row/window/poster-substitution Rust 1/1. Source binds native message/poster geometry, scope/generation/order and the production callback. | `pending-independent-audit`; author evidence is source/test only. No Windows or cross-target run proved Discord's required AutomationId/avatar resource/geometry contract. The rejected ancestor `2158d085…` remains only historical context. | **+0 pending** |
| Release admission `1a045707473a3098648d7c7445a0174401a968f3`, tree `3b5f3bef1b12947fe5f48bf61ea4e43e583cd77e` | I3 remains **1/3**; I4 remains **2/4** | Nine release-owned workflow/policy/candidate/evidence paths. The frozen receipt binds ten Git objects; the verifier admitted 18/18 with floor 18 and three nonempty files. Starved, floor-minus-one, runner-nonzero, zero-floor, malformed/empty output, drift, tampering and duplicate-key controls refused independently. | `ACCEPT +0`, status `test-proven-only`. This proves a fail-closed local measured-suite admission contract, not a GitHub-hosted green Rust/TypeScript/selector/security run, signed candidate, VM promotion, reproducible release or rollback execution. Keyserver floors remain `BLOCKED`. | **+0** |
| ExplicitExport bridge `075ad3f0cba91b117da4de12ca32a157ee7691f8`, successors `a8bc66093cebc4192fb18503faaeb70906b1c893` and `18a799f01370d3e7529311f956b62bc7507ff9aa` | F2 remains **3/5**; F10 remains **1/3** | `075ad3f` adds `scrub-export-chooser.ts` and its test; `a8bc660` adds an exported `main.ts` dispatch seam; `18a799f` changes `scrub-local.ts` and the chooser test. The terminal object reuses the native-owned import ID, bounds chunks to 256 messages/12 MiB, cancels the exact active import after post-init failures, and checks final encrypted/deletion-disabled/readback counters. | All three exact objects `REJECT`. The live file-input handler still bypasses the dispatch seam; there is no production provider selector or non-mock parser registry, and no five-provider parser-to-native behavioral positive or exact attachment/gap propagation. The cancellation/size sub-boundary improved, but the feature remains `implemented-unwired`. | **+0** |

No independently accepted exact object in this reconciliation crosses a new checklist row. The
authoritative result therefore remains **100 / 303**; no checklist edit is warranted.

## Lane roster and exclusive ownership

Eight tabs, disjoint by construction. Collisions are the main failure mode: two tabs editing one
file broke the build today, and a third drove another lane's running application through six UI
steps. Ownership is not advisory.

| Lane | Owns exclusively | Working on |
|---|---|---|
| **crypto** | `apps/osl-hub/src/{security,broker,main}.rs`, `apps/osl-hub-ui/src/{overlay,security.test}.ts`, `crates/keystore/**`, `crates/ipc/**` | four VM agent verbs; control-inbox dead-letter |
| **scrub** | all of `/home/liamw/osl-newest-integration` | review host-driver draft → verify loop → re-snapshot → film |
| **truth** | `oslprivacy-web`, `docs/**` except other lanes' report files. **Single writer of the checklist.** | applies points; website v1 reframe; zero-caller claim triage |
| **release** | `.github/**`, `scripts/{ci,release}/**`, VM release gate doc, `rust-toolchain.toml` | cold snapshot lineage; signed candidate |
| **keyserver** | `cipher-store-cf/**`, `keyserver-cf/**` | open-registration residual; smoke suite |
| **vm** | `scripts/vmqa/**`, the Azure share/agent, crypto's VM pair | build the rig |
| **store** | `crates/store/**` (released by crypto) | six audit defects; burn is terminal |
| codex-master | — | Telegram `/osl` bot |

Point claims never edit the checklist. A lane ends its report with an "Acceptance rows this earns"
section; **truth** judges and applies it in one atomic edit. A half-edited checklist makes the bot
discard the whole import.

## Live in production

| Thing | State |
|---|---|
| Keyserver migration `0029` | **applied**; all 111 identities quarantined until re-registration; snowflakes refused (`400 Discord identifiers are not OSL identities`) — closes audit CRITICAL P0-3 |
| Keyserver worker | `verified-live`: version 169, exact UUID `3f92f0f5-c6ac-4426-9a83-1555f5c6394b`, 100%. Exact Git commit `unknown`; Cloudflare has no Git annotation. |
| cipher-store worker | `verified-live`: version 15, exact UUID `0a17547d-577e-4f70-8159-f5d90e9c9e31`, 100%. Exact Git commit `unknown`; Cloudflare has no Git annotation. |
| Attachment part upload | **`201`**. Was `500` for an unbounded period; verified fixed by live probe |
| `0028` link-grant | applied but **dark** behind default-off `LINK_GRANT_ENABLED` in `src/index.ts` |
| `0027` | deployed. The "NOT DEPLOYED" header in that migration file is stale — ignore it |
| Backups | D1 pre-`0029` dump + tree snapshots under `/home/liamw/osl-backups/` |

**Not live:** exact `8802225` migration `0030`/reserved-namespace refusal and exact `e8fbd3f`
stale-upload reclamation are local-only. Neither commit is mapped to the active Worker UUIDs, no
deployment occurred, and production behavior must not be inferred.

## Owner decisions made — do not re-litigate

- **Pricing:** Pro is $5/month; compute credits are separate one-time purchases.
- **Crypto-shred: not now.** `crates/store/src/lib.rs:194` never writes `wrapped_key`, so burn
  deletes bookkeeping rather than decryption capability. Design it, document blast radius, do not
  execute. "Cryptographic burn" stays a banned claim.
- **Control-inbox dead-letter:** snowflake-shaped sender post-`0029` can *never* resolve → retire
  with a recorded reason. Everything else → bounded retry, then quarantine. Never silently delete an
  authenticated row. Surface the count.
- **Duress TPM:** tri-state (`evicted` / `no-TPM-nothing-to-evict` / `failed`). Never return `Ok(())`
  to paper over it — that reports destruction that did not happen.
- **VM input injection:** banned on the owner's desktop, **allowed and expected on an isolated VM**.
  A harness that refuses to click on a disposable VM cannot prove a consent flow.
- **Website framing:** pre-launch marketing for v1, not a status dashboard. Honesty concentrates in
  the support matrix and at checkout, not as a "Planned" badge on every card.
- **QA-only TLS cert** for seeded IMAP: approved, inside the disposable VM only.
- **PR #5:** release rebases and takes mechanical conflicts; scrub adjudicates scrub-semantic ones by
  intent. `permissions/hub.toml` (+110 vs +40) is a contested capability surface — security review,
  not a merge.

## Open blockers

1. **Aug 2 Scrub demo.** F2 harness is code-ready at exact `c5b516f`, but runtime capture is
   blocked. The strict-verifier-green real VM grant/import/revoke sequence still needs explicit
   owner authorization for disposable identity creation, then actual execution; authorization
   alone is not evidence.
2. **Two-identity proof (B6, 0/3).** Gates B7, C5, D3–D7. Needs the VM rig.
3. **Rust CI red.** Three defects release cannot fix. Branch protection requires only
   `["TypeScript gate","audit"]` — Rust is *not* required and `enforce_admins` is **false**, so main
   can merge with Rust red and an admin can bypass.
4. **No golden snapshots for the release gate.** One warm iteration snapshot now exists
   (`OSL-Independent-Client-1-WARM-iteration-20260726`, tagged `warm_verified: NO`). The gate needs a
   separate **cold** lineage — two lineages, never one.
5. **`NCryptDeleteKey` result discarded** on machines that *do* have a TPM — a failed delete was
   reported as a successful wipe. Already shipping. Crypto owns.
6. **C4 harness inadmissible.** Do not run the owner Discord send using exact `31edb63`.
   Artifact-rooted feature/process attestation, retained native command/readback authority,
   semantic screenshot binding, replay consumption, and collector/verifier schema parity must land
   first.

## Closed evidence since r12 — no acceptance points

- **Public signed-burn overclaim closed by `dae12da`.** `README.md:85-87` now says the
  peer-notification path is not proved end to end, is not available as a working peer action today,
  and must not be relied on to remove another member's copy. Node 24 reports 7,509 scanned units and
  0 violations on current shared bytes. That closes the **SOLD** string blocker; it does not make
  `BurnAlertPayload` reachable. Triage is now **UNCLAIMED**, status `implemented-unwired`.
- **Keyserver `284f0a5`: `test-proven-only`.** The committed real-D1 test proves a 100-row-per-table
  scheduled cleanup bound, request-receipt cleanup, live-row preservation and second-tick progress.
  It is not evidence of a live Worker deploy or the B5 client production contract.
- **Release `b9aa48e`: `test-proven-only`.** The clean release worktree is one commit ahead of
  `origin/release-lane-2026-07-26`; the keyserver test-count floor accepts the complete local output
  and refuses starvation, but the edited workflow has not run on GitHub-hosted CI. I3 stays 1/3;
  I2, I4 and I6 are unchanged.
- **UI `82b9238`: `test-proven-only`.** An exact-archive Node 24 run passes 13/13 focused tests.
  Removing recovery-proof invalidation makes the focus-loss semantic test fail, and removing the
  friend-removal click binding makes the rendered-control semantic test fail. This advances the
  evidence behind A1 and A5, but neither row crosses its live product boundary.
- **Release `c0c18b1`: local floor only.** The exact commit adds the 34-test floor for the
  36-test `qa_selftest_request` module and the floor independently refuses a zero-test collection.
  The reported focused `36 passed` and full-suite `741 passed; 1 failed; 1 ignored` results came
  from a disposable integration tree containing product bytes outside the release commit; an
  exact committed product archive currently stops at compilation. The named full-suite blocker is
  `broker::tests::the_text_drain_applies_inbound_revocations_instead_of_deleting_them`. The release
  branch remains unpushed.
- **Correction — public CI remains unchanged.** The earlier “public CI now requires” milestone was
  retracted: `c0c18b1` changed only an unpushed workflow. No remote ref, branch setting or
  GitHub-hosted run changed, so no public CI claim or I3 point moved. I3 stays 1/3.
- **Urgent A8/B3 adjudication: +1 net.** A8 stays 3/4: the store's exact v4/v3 evidence is
  `test-proven-only` and does not cover every legacy migration, an executed older-reader
  downgrade, attachment selector-to-metadata validation, or non-store persistence; graph shape
  remains visible. B3 moves 1/5 → 2/5: exact ratchet and dependency-closure archives prove the
  row's structural persistence/replay/reorder/skipped-key/restart boundary with negative controls.
  The ratchet remains `implemented-unwired`; no live traffic or dependent row is earned.
- **B5 chronology correction: `a509cb2` was retracted; later `5ba5a02` earns the narrow +1.** Exact
  `c0279dc` proves the keystore client and source call shape, but its source-text broker test
  executes neither drain and its real head-of-line test fails because all three relay fakes omit the
  mandatory echo. Exact later `aca9dae` passes two shared-fetch-helper tests and four refusal cases,
  but still executes neither actual drain, uses two-byte type markers, and preprograms A then B in a
  stateless fake rather than proving 64 foreign blockers remain untouched. Those commits did not
  cross the row. Exact `5ba5a029445ba0ce500aad2db8de4295cbd605ff` subsequently passes 8/8 in
  an exact archive and executes the production text drain plus attachment list/take with A behind
  64 retained foreign IDs and B independently drainable. Four filter refusals and independent
  text/attachment unfiltered mutations are load-bearing. B5 is now 2/4, `test-proven-only`; live
  D1, attachment blob download/decryption, provider, and two-identity behavior are not implied.
- **`79f12eb`: no A6/D2/D4 point.** The helper refuses a PDF before its synthetic download,
  decrypt, staging and durable write, and an image helper control continues. Removing the helper's
  refusal fails; removing its call from production `open_pending_inner` leaves the test green.
  Source ordering improved, but the protected-viewer production seam is not behaviorally proved.
- **Six exact completed commits, +0.** `c5b516f` makes F2 identity evidence binding
  mutation-sensitive (24/24) but performs no real VM take; F2 is code-ready/runtime-blocked.
  `6cc103b` passes 15/15 host regressions and closes a real whole-desktop/6×6-marker false green,
  but its final VM positive refuses the surface and emits no artifact. `8802225` proves local
  Worker-first/migration-second namespace safety; `0030` is undeployed. `e8fbd3f` proves local
  D1/R2 stale-upload reclamation and retry ordering; it is undeployed. `f0bd0e1` records F5's
  rejected incoherent exact candidate. `31edb63` passes 17 harness tests but is inadmissible after
  fake executable/screenshot, replay, caller-binding, synthesized-receipt and unreachable-source
  controls. F2 stays 3/5, F5 1/5, C4 1/4; all other rows stay unchanged.
- **F1 evidence correction: −1.** Exact `ae9d5a1` supersedes its earlier recommendation and
  explicitly withdraws the F1 +1. The native Brave reader scoping result remains
  `runtime-proven`, and direct native persistence is `test-proven-only`; no live renderer picker →
  Tauri grant/revoke → persisted UI re-read workflow ran. Exact VM capture remains blocked. F1 is
  4/6, not 5/6.
- **Website candidate `15fa16c`: +0.** The website worktree is clean at the exact local commit,
  three commits ahead of the local `origin/main` ref. Build-identity 38/38, live-verifier 12/12
  local-fixture, pricing, status, and claim gates pass. It is unpushed and undeployed; Pages
  dashboard command/environment, Pages build record, live SHA-bound `/build.json`, and keyserver
  redemption evidence are absent or `unknown`. Independent exact-archive mutation audit also
  accepts false/duplicate metadata, an undeclared served file, and a changed `_headers`; nested
  asset HTML is not stamped, and the required planted ignored-file refusal does not occur even
  though the bytes are excluded. The pre-existing crypto-checkout gate is 9/10. This is not locally
  promotion-ready; H1 remains 3/4.
- **Net arithmetic:** B5 `+1`, F1 `−1`, H1 `+0`; **97 + 1 − 1 + 0 = 97 / 303**.
- **D7 safety tie-breaker: +0.** Committed product HEAD
  `05282a493fbb165e2011edc2991a273ead3dddc6` retains D7 at 1/4 for the authenticated,
  sent-record-correlated receipt foundation, but production `Opened` emission and admission lack
  locked mutual consent. Status is `open-security-finding`; the consent contract is
  `implemented-unwired`. Immediate closure is fail-closed suppression/rejection of `Opened` while
  preserving `Received`. Restore requires durable scope/identity-bound signed, expiring, revocable
  mutual consent checked under the receipt-state lock on both sides. Pending crypto work is not
  credited.
- **Website candidate `4e225633`: +0, superseding `15fa16c` as the local candidate.** Exact
  `4e2256333c53e6b6e17462657260f5d6499ec9ee` is clean and local-gate green. Checkout is disabled;
  the branch is four commits ahead of the local `origin/main`, unpushed and undeployed. Pages
  dashboard/build/live SHA proof and keyserver redemption remain absent/`unknown`; H1 stays 3/4.
- **Historical arithmetic at that checkpoint:** D7 `+0`, H1 `+0`;
  **97 + 0 + 0 = 97 / 303**.

## Coordination infrastructure built today

- **`osl-say`** (`~/.local/bin`) — sends a prompt straight into a lane tab through its mirror FIFO
  using bracketed paste, Enter sent separately. Routes via `~/.osl-lanes.json` pinned session ids,
  because Claude Code rewrites terminal titles by activity and title matching misroutes.
  `osl-say <lane> "msg"`, `osl-say <lane> < file`, `osl-say --all`, `osl-say --list`.
- **`~/osl-lanewatch/lanewatchd.py`** — polls every 5 min. Passive by construction: never builds,
  never takes the cargo lock (an earlier version did and helped cause an OOM crash). Guards memory
  and indexer count, nudges stalled lanes, and **relays a lane's report into the coordinator tab**
  when it goes quiet *and* has written something new. Digest, not decision — routing stays human.
- **`docs/testing/azure-vm-qa-workflow.md`** — the shared VM design. 10 Windows VMs, vault
  `osl-test-secrets-a7d5d9`, build-on-host, file rendezvous, warm-vs-cold lineages.

## The pattern that defined this session

Six separate systems reported success without ever doing the thing:

1. An R2 test double accepted any stream → **every attachment upload had been failing in production**.
2. Hand-rolled D1 fakes could only re-assert what their author already believed.
3. A release workflow that had **never once been executed** would have died before reaching the signer.
4. A feature gate (`all(core, discord-qa-shell)`) silently excluded the module deciding whether a
   trigger becomes a status read or the **irreversible send**. Every Rust count quoted was wrong.
5. A default-deny assertion passed vacuously because it read the wrong root — "found nothing" is
   indistinguishable from "correctly denied".
6. Every website gate would have exited 0 on a broken glob — `"scanned 0 files, 0 failed"`.

**Standing rule:** prefer a real runtime over a double; when you must use a double, ask what it would
fail to catch. A test that cannot fail is not evidence, and an inherited number is not a measurement.
Assert non-empty on the positive path so the negative path cannot pass vacuously.

## Standing operational rules

- **Delegation is inverted:** Codex is the default executor, Claude the reviewer. The question is
  "why is this *not* a Codex job?" Only proof-judgement, security calls, adjudication, review and the
  report stay in Claude.
  `CODEX_HOME=$HOME/.codex-b codex exec -m gpt-5.5 -C <dir> "<task>" < /dev/null`, backgrounded.
  `.codex-b` 98% remaining → `.codex` 72% → **avoid `.codex-c`, 8%**. Statusline shows *remaining*.
  `codex2`/`codex3` are `.bashrc` aliases that silently vanish inside a script. `-p fast` does not exist.
- **Max 2 concurrent Codex per lane.** Wait if available memory < 4 GB or load > 12. A 0-byte output
  is a suspected OOM kill, not a result.
- **`flock` is not reentrant.** Wrap cargo you type; never a script that locks internally
  (`scripts/qa/osl-instance-b-build-wsl.sh:88`). Nested acquisition hangs silently with no process in `ps`.
- **`--features core` does not compile `main.rs`,** and excludes `qa_selftest_request`. The honest
  gate is `--features core,discord-qa-shell` (731 tests, one pre-existing `native_discord_adapter` failure).
- **Commit with explicit pathspecs.** `git add <your files> && git commit -- <your files>`. Never
  `-A`. ~17,000 lines sat uncommitted across two trees with zero commits before this rule landed.
- **rust-analyzer is the OOM cause** — it crashed the box twice at ~4 GB per instance. Parents are
  `claude` processes directly, so the serena `languages: []` fix does not fully hold. Watch the count.

## Resume here

Historical machine snapshot: 10 GB used, 15 GB available, load 9.1, one indexer, watchdog running.
That snapshot counted 17 commits across both trees and reports for baseline, crypto, server, truth
and VMQA (Scrub's lived in its own worktree). Current release reconciliation is recorded in the
2026-07-27 exact-object table above.

## Shortest honest next-point map from 100 / 303

| Order | Row | Single missing boundary | Owner / artifact contract | External mutation or confirmation |
|---|---|---|---|---|
| 1 | F1 `4/6 → 5/6` | One exact-build live Windows picker → grant IPC → nonempty import → revoke IPC → persisted re-read must succeed. | Scrub + VM lanes; retained build identity plus bound UI/IPC verdict and before/granted/revoked frames. | **Yes.** The disposable identity/import walkthrough needs owner confirmation and runtime mutation; approval alone earns nothing. |
| 2 | F2 `3/5 → 4/5` | One strict-verifier-green real VM five-frame identity-bound grant/import/revoke walkthrough on the exact build. | Scrub + VM lanes; `./scripts/qa/vm-run-loop.sh --share <share> --timeout 600 --identity-client 1 --confirm-create-identity --verbs identity-status,create-identity,list-browser-profiles,grant-browser-profile,run-browser-import,revoke-browser-profile`; five bound PNG/JSON pairs, positive account list and cleanup. | **Yes.** Disposable identity creation needs owner confirmation and mutates the live keyserver; confirmation itself earns nothing. |
| 3 | I3 `1/3 → 2/3` | The exact candidate must be pushed and the named public Rust gate must finish green alongside the already-named public gates. | Release lane; remote commit SHA plus GitHub-hosted workflow URL showing the required Rust/TypeScript/selector/security jobs green. | **Yes.** Push and hosted CI mutate the remote; an unpushed local floor is not evidence. |

Next coordination beats: VM/Scrub repair the visible-frame capture and wait for explicit identity
authorization; release resolves the named Rust blocker,
pushes, and waits for GitHub-hosted results; truth awards only after each artifact exists.
