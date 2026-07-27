# RELEASE-LANE-2026-07-26 — Green CI, signed-candidate promotion, rollback, governance

**Status:** `test-proven-only` for the CI pipeline (proven by real runs on GitHub-hosted
runners); `designed-only` for anything requiring an actual release, because the repository
has zero releases and zero `hub-v*` tags.

- **Worktree:** `<workspace>/osl-release-lane` (created for this lane; no other lane's tree touched)
- **Branch:** `release-lane-2026-07-26`, based on `origin/main` @ `38d0867`
- **PR:** [#6](https://github.com/OSLPrivacy/discord-privacy-client/pull/6)
- **Master control revision at time of run:** `OSL-MASTER-2026-07-26-r6`
- **Owner of this task:** release lane (tab 4)

## Exclusive files claimed

`.github/**`, `scripts/ci/**`, `scripts/release/**`,
`docs/testing/hub-release-candidate-vm-gate.md`, `rust-toolchain.toml`, and this report.

Nothing under `apps/**`, `crates/**`, `webview/**`, the website, `docs/design/**`, the build
checklist, `<workspace>/osl-newest-integration`, or the root `Cargo.toml` was modified.
`rust-toolchain.toml` was read and left unchanged — it already pins `1.88.0`, matching CI.

## Starting evidence

Established from GitHub Actions history rather than assumption:

- **Rust Test** and **TypeScript Test** failed on **every push to `main` for 20+ consecutive
  runs**, from 2026-07-19 through the tip commit `38d0867` (2026-07-22).
- **Selector CI** (scheduled, every 6 h) and **Public release audit** were green throughout.
- `main` had **no branch protection** (`/branches/main/protection` → 404) and **no rulesets**,
  on a public Apache-2.0 repository.
- **Zero releases. Zero `hub-v*` tags.** The signed-release path had never executed.

## Root causes — two single sites, neither a product defect

| # | Cause | Evidence |
|---|---|---|
| 1 | `cargo clippy --workspace --all-targets -- -D warnings` failed on **one style lint**, `clippy::question_mark`, at `crates/ipc/src/commands.rs:8901` | run 29942516581; exactly one lint name in the whole log |
| 2 | `apps/osl-hub-ui/src/latest.test.ts:65` asserts `runs.length < 24` for a coalescing burst paced by 2 ms / 8 ms `setTimeout`; the Windows runner counted **26** | run 29942516663 |

Cause 1 had a second, worse effect. Because `fmt`, `clippy` and every test lived in one job as
sequential steps, clippy failing first meant **`cargo test --workspace` never executed once** in
that entire window. Nobody could have known whether the Rust tests passed.

Cause 2 is a runner-clock artifact. Windows' default ~15.6 ms timer quantum collapses the
distinction between the 2 ms request pacing and the 8 ms work, so coalescing stops happening and
the count climbs. Measured locally 2026-07-26, `apps/osl-hub-ui` on `origin/main` bytes:

```
20 iterations of: npx vitest run src/latest.test.ts
Linux runner: 20/20 passed, 0/20 failed
```

versus deterministic failure on `windows-latest`.

## Decisions made

1. **Split the Rust job.** `workspace-test`, `hub-core-test`, `hub-desktop-check`, `lint`,
   `quality-checks`. A cosmetic finding can never again mask the release-relevant signal, and
   every failure is attributable without reading a log.
2. **Clippy is classed, not blanket.** The blocking step keeps rustc's own warnings plus
   `clippy::correctness`, `::suspicious`, `::complexity` and `::perf` denied via `-D warnings`,
   and downgrades **only** `clippy::style`. A second **advisory** step runs full `-D warnings`
   and writes every finding to the job summary, so the style debt stays quantified rather than
   discarded. A release pipeline that a style preference can halt is not a release pipeline.
3. **Node suites run on `ubuntu-latest`.** They are pure Node/JSDOM with no Windows-specific
   code path; Linux is cheaper, faster, and has the timer resolution the tests assume. This is
   a correctness argument, not a convenience one — but see *Remaining failures*: it does not
   make the assertion itself sound.
4. **Aggregate gate jobs.** `Rust gate` and `TypeScript gate` exist purely to give branch
   protection stable check names. GitHub matches required checks by name, and a matrix job's
   name changes whenever its matrix does, silently un-enrolling the requirement.
5. **Prove the gates refuse.** Both the promotion gate and the rollback guards now have
   negative-control batteries that run on every PR.
6. **Did not fix source outside this lane** even where the fix is one line. Exact patches are
   handed off below.

## Changed files

| File | Change |
|---|---|
| `.github/workflows/rust-test.yml` | split into 5 jobs + aggregate gate; classed clippy; `--test-threads=1`; new desktop-binary compile job; actionlint, shellcheck, and both gate proofs |
| `.github/workflows/ts-test.yml` | matrix over 5 packages on `ubuntu-latest`; adds the `typecheck` osl-hub-ui always had; concurrency; aggregate gate |
| `.github/workflows/reproducible-build.yml` | retargeted onto the artifact that actually ships; frontend determinism job; deterministic-profile preflight |
| `.github/workflows/osl-hub-rollback.yml` | **new** — attested rollback of the update feed |
| `.github/BRANCH_PROTECTION.md` | rewritten against the real (absent) configuration |
| `.github/branch-protection.json` | **new** — the protection payload, reviewable in a diff |
| `scripts/ci/node-suite.sh` | **new** — per-package Node gate with explicit required steps |
| `scripts/release/prove-promotion-gate.sh` | **new** — 20-case negative-control battery |
| `scripts/release/rollback-guard.sh` | **new** — pure rollback decision logic |
| `scripts/release/prove-rollback-guards.sh` | **new** — 9-case negative-control battery |
| `docs/testing/hub-release-candidate-vm-gate.md` | promotion + rollback + proofs + the open blockers |

## Verification commands and exact results

### Local, on `origin/main` bytes

```
$ bash scripts/release/prove-promotion-gate.sh
promotion gate proof: 20 passed, 0 failed          (exit 0)

$ bash scripts/release/prove-rollback-guards.sh
rollback guard proof: 9 passed, 0 failed           (exit 0)

$ actionlint (v1.7.7, all 9 workflows)             exit 0
$ shellcheck scripts/ci/*.sh scripts/release/*.sh  exit 0

$ apps/osl-hub-ui: npx tsc --noEmit                exit 0
$ apps/osl-hub-ui: npm test        54 files / 379 tests passed
$ apps/osl-hub-ui: npm run build   built in 2.36s
$ webview: typecheck / lint / test  exit 0, 1 test passed
$ keyserver-cf: typecheck + test    exit 0, 3 tests passed
$ cipher-store-cf: typecheck + test exit 0, 18 tests passed
$ keyserver: npm test               pass 72, fail 0

$ apps/osl-hub-ui frontend built twice, dist tree-hash compared:
  b76b99a320084b8739390eceb9907cabee91293a36189df40dd82bc86fef7897
  b76b99a320084b8739390eceb9907cabee91293a36189df40dd82bc86fef7897
  -> byte-reproducible
```

No `cargo` command was run locally. Per the machine constraint, the Rust surface was validated
remotely on CI instead; `cargo check --workspace` / `--all-targets` were never invoked here.

### Remote, PR #6 (runs 30226879644 Rust, 30226879657 TypeScript, 30226879673 audit)

| Check | Result |
|---|---|
| **TypeScript Test** — webview, osl-hub-ui, keyserver-cf, cipher-store-cf, keyserver-legacy, TypeScript gate | **all success**, 56 s |
| **Public release audit** | success |
| **Rust Test → quality checks** — boot.js, capability audit, actionlint, shellcheck, verifier self-test, promotion gate proof | **all steps success** |
| **Rust Test → workspace-test / hub-core-test / hub-desktop-check / lint** | see *Rust CI outcome* below |

This is the **first green TypeScript Test run since 2026-07-19**.

## Rust CI outcome

<!-- RUST-CI-RESULT -->

## Negative controls / failure injection

The promotion gate is the only thing between a draft and a published release, so it was tested
for refusal, not only acceptance. `prove-promotion-gate.sh` builds a synthetic candidate that
passes, then mutates one field at a time and requires a refusal **for the stated reason** —
a rejection with the wrong message counts as a failure. Covered:

substituted binary · mismatched tag · malformed hash · unknown schema version · single-VM run ·
**same golden snapshot restored twice** · reused VM name · `cleanRestore: false` ·
`cleanRestore: "true"` as a string · omitted required case · unknown extra case · failed case ·
**automated CAPTCHA** · blank operator · non-UTC timestamp · missing updater manifest ·
two ambiguous installers · no installer at all.

Plus two positive controls, one before and one after the battery, so a gate that had started
rejecting everything would itself be caught: **20 passed, 0 failed.**

`prove-rollback-guards.sh` covers rolling onto a draft, rolling onto the live version, malformed
and metacharacter-bearing tags, and unreadable feed state: **9 passed, 0 failed.**

`node-suite.sh` was also failure-injected: a missing npm script and a missing package both
produce a non-zero exit with a precise `::error::` line, so a renamed script cannot silently
stop being checked.

## Remaining failures and unknowns — handed off, with exact patches

These are real and are **not** fixed by this lane, which does not own the files.

1. **`crates/ipc/src/commands.rs:8901`** — owner: crypto/Rust source lane.
   ```rust
   // replace
   if let Err(e) = commit_staged_account_import(dir, &stage, &files) {
       return Err(e);
   }
   // with
   commit_staged_account_import(dir, &stage, &files)?;
   ```
   Note the existing comment above that block explains *why* the stage is kept on failure; keep
   it. Once this lands, `-A clippy::style` can be dropped from `rust-test.yml` and full
   `-D warnings` restored as blocking. The advisory step already reports what that would cost.

2. **`apps/osl-hub-ui/src/latest.test.ts:65`** — owner: the osl-hub-ui lane.
   `expect(runs.length).toBeLessThan(24)` is a wall-clock budget. Moving to Linux made it pass
   20/20, but it remains sensitive to a loaded runner and **will flake again**. The behaviour
   worth asserting — that coalescing happens and the newest request always runs — is already
   covered by `runs.at(-1) === 39` and `runs.length > 2`. Either drive it with fake timers or
   delete the upper bound. **CI moving to Linux is not a fix for this; it is a change of venue.**

3. **`apps/osl-hub/Cargo.toml` has no `[profile.release-deterministic]`** — owner: the
   `apps/osl-hub` crate owner. `apps/osl-hub` declares its own `[workspace]` (line 71) and
   therefore does **not** inherit the root profile. The binary users install has no
   deterministic build profile. `reproducible-build.yml` now fails loudly on this rather than
   passing quietly against the legacy binary.

4. **`hub-vm-qa` environment does not exist.** `osl-hub-promote.yml` (and now the rollback
   workflow) name it. A workflow naming a missing environment **does not fail** — GitHub creates
   it on first use with zero protection rules. The "separate human approval" in front of
   promotion is therefore currently enforced by nothing. Owner decision required.

5. **No release has ever been produced**, so the signed-candidate path and a live rollback are
   both unexercised. Cutting the first candidate is a production action and was not taken here.

6. **Unknown until the Rust jobs finish:** whether `cargo test --workspace` passes at all. It
   has not executed on `main` since at least 2026-07-19.

## Repository governance (I6) — findings

- `main`: **unprotected**, no rulesets, public. Payload prepared at
  `.github/branch-protection.json`; applying it is sequenced *after* this PR merges, because the
  payload requires one approving review and would otherwise block its own landing.
- Environments: `hub-release` has `required_reviewers` + `branch_policy` (good).
  `hub-vm-qa` **missing** (see above). Two unrelated environments exist,
  `OSLDC / production` and `zooming-prosperity / production`, both with **no** protection rules —
  worth an owner look; this lane did not touch them.
- **Open PRs — all five conflict with `main`:**

  | PR | Age | Size | Recommendation |
  |---|---|---|---|
  | #5 Scrub onboarding + deletion engine | 2026-07-22 | +8680/−486, 63 files | **Highest priority.** This is hard-deadline Scrub work and it is `CONFLICTING`. The Scrub lane should be told today. |
  | #3 Upgrade keyserver Fastify deps | 2026-05-11 | +324/−195 | Security-relevant dependency bump; rebase or supersede. |
  | #1 Harden Tauri remote capabilities (draft) | 2026-05-11 | +15/−26 | Verify whether already superseded by current capability work. |
  | #2 Clean up stale docs and test warnings | 2026-05-11 | +15/−18 | Low value now; close or rebase. |
  | #4 Remove Discord password settings (draft) | 2026-05-11 | −998 | Confirm the surface is already gone before closing. |

  No PR was closed and no branch deleted — several carry the only copy of their work.
- **Release record: none.** Zero releases, zero `hub-v*` tags. The existing tags are legacy
  `v0.0.x-phase*` markers pointing at the old client.

## Integration line proposal (I2) — proposed, NOT executed

Executing this is a separately authorized task. Inventory from a read-only survey of 21
registered worktrees: **16 dirty**, ~586 dirty entries total; **10 worktrees independently edit
`apps/osl-hub/src/main.rs`**; **9 edit `apps/osl-hub-ui/src/main.ts`**; one stash; two worktrees
carry no unique work (`.cache/osl-bisect`, `osl-payment-theory-release`).

Authoritative line: **current GitHub `main`**. Not local `main` (stale), not
`osl-newest-integration`, not the eye branch.

| Wave | Content | Serialized central files | Parallel? |
|---|---|---|---|
| **0 · Freeze** | Tag and fingerprint every worktree HEAD, dirty diff hash, untracked list, stash. Land PR #6 first so every later wave has a working gate. | none | no |
| **1 · Shared contracts** | crypto/keystore/keyserver migrations, THREAT_MODEL, capability manifests | `capabilities/hub.json`, `permissions/hub.toml`, `keyserver-cf/migrations/**`, both `Cargo.toml` | no |
| **2 · `main.rs` wave** | Resolve 10 competing copies by interface, one owner at a time | `apps/osl-hub/src/main.rs`, `broker.rs`, `native_discord_overlay.rs` | no |
| **3 · `main.ts` wave** | Resolve 9 competing copies | `apps/osl-hub-ui/src/main.ts` | no |
| **4 · Narrow subsystems** | browser-import, creative, notes, WhatsApp/Signal QA adapters | none — disjoint `owns` sets | **yes** |
| **5 · Reconcile and gate** | manifests/migrations reconciled; full CI, reproducible build, Windows exact build, two-identity proof, claim gate, signed release path | all | no |

Rules carried from master §21–22: no force-push, no history rewrite, no branch deletion, and no
worktree removed until the integrated release is independently proven. Wave boundaries require
green CI **and** no new capability drift (the 115→118 permission drift recorded in
`baseline-2026-07-26-current-bytes.md` is exactly the class of thing to check at each boundary).

## Conflicts with the dispatch brief

- The brief states `tauri.conf.json` has "an empty updater pubkey placeholder". **It does not.**
  On `origin/main` and in the working tree, `apps/osl-hub/tauri.conf.json:50` carries a populated
  minisign public key (`untrusted comment: minisign public key: 3B6AE4739858E8D4`). The real
  updater risk is not the public key but whether `HUB_TAURI_SIGNING_PRIVATE_KEY` is actually
  present in the `hub-release` environment — unverifiable from here, and untested because no
  release has ever been cut.
- The brief describes the failing TypeScript test as being under `webview/`. It is
  `apps/osl-hub-ui/src/latest.test.ts`; `webview/src/latest.test.ts` does not exist.
- The brief implies `scripts/ci/**` and `scripts/release/**` may already exist. Neither did.

## Integration and rollback for *this* change

- All work is on `release-lane-2026-07-26` in PR #6. Nothing is merged, deployed, signed or
  published. No release, tag, branch or file was deleted.
- Rollback of this change is `gh pr close 6` — the previous CI configuration is untouched on
  `main` and is recoverable by not merging.
- The new worktree `<workspace>/osl-release-lane` is additive and holds no unique uncommitted
  work; everything in it is committed and pushed.

## Resume here

- **Current verified state:** TypeScript CI green on real runners for the first time since
  2026-07-19; both release gates proven to refuse bad input (20/20 and 9/9); frontend proven
  byte-reproducible; governance gaps identified with a prepared payload.
- **Exact worktree/branch:** `<workspace>/osl-release-lane` @ `release-lane-2026-07-26`,
  based on `38d0867`. Clean apart from gitignored `node_modules`/`dist`.
- **Next unblocked actions, in order:**
  1. Confirm the four Rust jobs on PR #6 (see *Rust CI outcome*); fix forward if any fail.
  2. Merge PR #6 so `main` is green.
  3. Apply branch protection:
     `gh api -X PUT repos/OSLPrivacy/discord-privacy-client/branches/main/protection --input .github/branch-protection.json`
     — **after** the merge, because it requires an approving review.
  4. Ask the owner to create the `hub-vm-qa` environment with required reviewers.
  5. Hand the three source patches above to their owning lanes.
- **Blocked, needs owner:** cutting the first `hub-v0.1.0` candidate (production action). Until
  that exists, "signed candidate" and "rollback exercised" cannot honestly be claimed.
- **Known risk:** merging to `main` during a period when ~20 worktrees hold unique uncommitted
  work. This change touches only `.github/**`, `scripts/ci/**`, `scripts/release/**` and
  `docs/testing/`, none of which appear in the serialized-central-file conflict set.

## Acceptance rows this earns

Proposed for the single writer of the build checklist to adjudicate; deliberately conservative.

| Row | Current | Proposed | Evidence |
|---|---|---|---|
| **I3** Green public Rust/TS/selector/security CI | `earned: 0` / 3 | **2** | TypeScript Test green (run 30226879657, all 5 packages + gate); Public release audit green; Selector CI already green; Rust quality-checks green. Held at 2 not 3 because it is green on PR #6, **not yet on `main`**, and two named source defects still sit behind a documented lint downgrade. Move to 3 once #6 merges and `main` runs green. |
| **I4** Signed candidate, VM promotion, reproducible release, rollback | `earned: 0` / 4 | **2** | Promotion gate proven to refuse 18 distinct bad candidates and accept a valid one (20/20, run in CI); rollback path built with guards proven 9/9; `reproducible-build.yml` retargeted onto the shipped artifact; frontend proven byte-reproducible. Held at 2 because **no signed candidate has ever been produced**, no rollback has been exercised live, and `apps/osl-hub` still lacks a deterministic profile. |
| **I6** Repo governance / branch protection / PR cleanup / releases | `earned: 1` / 2 | **1** (unchanged) | Findings and a reviewable protection payload exist, but `main` is still unprotected, no PR was cleaned up, and there is still no release record. **Claim 2 only after** the protection payload is applied and verified. |
| **I2** Authoritative integration line and central-file waves | `earned: 0` / 3 | **1** | Ordered six-wave plan with per-wave serialized central files, parallelism marked, and a 21-worktree conflict inventory (10 × `main.rs`, 9 × `main.ts`). Held at 1 because the row requires the line to be *established*, and execution was explicitly out of scope. |

No claim is made on I1, I5 or I7.
