# Coordinator state — 2026-07-26 evening

Written so a fresh context can resume coordination without re-deriving it. This is the
*coordination* layer only: who owns what, what is decided, what is live, what is blocked. Product
authority remains `docs/design/osl-master-decision-2026-07-26.md` (currently **r13**), and the
scoreboard remains `docs/design/osl-internal-build-checklist.md` (**97 / 303**).

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

1. **Aug 2 Scrub demo.** Critical path: VM agent verbs (crypto) → loop verified → re-snapshot →
   film. Capability risk retired; capture is what remains.
2. **Two-identity proof (B6, 0/3).** Gates B7, C5, D3–D7. Needs the VM rig.
3. **Rust CI red.** Three defects release cannot fix. Branch protection requires only
   `["TypeScript gate","audit"]` — Rust is *not* required and `enforce_admins` is **false**, so main
   can merge with Rust red and an admin can bypass.
4. **No golden snapshots for the release gate.** One warm iteration snapshot now exists
   (`OSL-Independent-Client-1-WARM-iteration-20260726`, tagged `warm_verified: NO`). The gate needs a
   separate **cold** lineage — two lineages, never one.
5. **`NCryptDeleteKey` result discarded** on machines that *do* have a TPM — a failed delete was
   reported as a successful wipe. Already shipping. Crypto owns.

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
- **B5 counteradjudication: `a509cb2` +1 retracted; `c0279dc` and `aca9dae` earn +0.** Exact
  `c0279dc` proves the keystore client and source call shape, but its source-text broker test
  executes neither drain and its real head-of-line test fails because all three relay fakes omit the
  mandatory echo. Exact later `aca9dae` passes two shared-fetch-helper tests and four refusal cases,
  but still executes neither actual drain, uses two-byte type markers, and preprograms A then B in a
  stateless fake rather than proving 64 foreign blockers remain untouched. B5 returns 2/4 → 1/4;
  score returns 98 → 97. The required broker-level text-plus-attachment delivery positive remains
  `unknown`.
- **`79f12eb`: no A6/D2/D4 point.** The helper refuses a PDF before its synthetic download,
  decrypt, staging and durable write, and an image helper control continues. Removing the helper's
  refusal fails; removing its call from production `open_pending_inner` leaves the test green.
  Source ordering improved, but the protected-viewer production seam is not behaviorally proved.

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

Machine healthy: 10 GB used, 15 GB available, load 9.1, one indexer, watchdog running.
17 commits today across both trees. Reports on disk: baseline, crypto, server, truth, vmqa
(scrub's lives in its own worktree; release's is pending).

Next coordination beats, in order: crypto dispatches the VM verbs and dead-letter → vm builds the
share and agent → scrub verifies the loop and films → release creates the cold lineage → truth keeps
applying points. Nothing is waiting on the owner except promoting the website branch.
