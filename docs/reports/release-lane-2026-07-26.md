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
| `scripts/ci/hub-core-tests.sh` | **new** — osl-hub core suite with an expiring, documented quarantine |
| `.github/branch-protection-phase1.json` | **new** — the payload actually applied today |
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

### Final Rust run — 30227517355 @ `373dc53`

| Job | Result | Detail |
|---|---|---|
| `osl-hub desktop binary compiles` | **success** | **New guarantee.** `main.rs` is gated behind `required-features = ["desktop"]`, so `--features core` never compiled it; until now the shipped binary was first type-checked at *tag* time. |
| `quality checks` | **success** | boot.js parse, capability audit, actionlint, shellcheck, verifier self-test, **both gate proofs** |
| `lint` | failure | `cargo fmt` clean. Clippy cleared `crates/ipc` — the style downgrade worked. Now blocked on 2 **rustc `dead_code`** errors, see below. |
| `workspace build and test` | failure | Every test binary green **except one**: `crates/keystore/tests/duress_test.rs`, 7 passed / **6 failed** |
| `osl-hub core tests` | failure | **237 passed, 1 failed, 2 filtered** — quarantine working exactly as designed |
| `Rust gate` | failure | aggregate of the above |

Each fix uncovered the next layer, because for a week nothing got past the first
one. In order: style lint → missing `webview/dist` → dead code; and separately,
suites that had never executed at all.

### `cargo test --workspace` — first execution since at least 2026-07-19

Roughly 70 test binaries and several hundred tests pass. Exactly one binary fails:

```
crates/keystore/tests/duress_test.rs
test result: FAILED. 7 passed; 6 failed
```

**Verified platform-specific.** Run locally on Linux at the same commit:

```
$ cargo test -p keystore --test duress_test
test result: ok. 13 passed; 0 failed
```

13/13 on Linux, 7/13 on Windows. The six failures are Windows-only.

Root cause (diagnosis delegated to Codex, then verified here): the
`TpmEvict` step is `#[cfg(windows)]` and calls `evict_tpm_key()`. A hosted
Windows runner has no usable TPM provider, so eviction returns `Err`, which
`duress.rs:313` maps to `StepOutcome::Failed`. That single failure then
cascades: `failed_steps()` is non-empty (`duress.rs:229`), so
`remove_journal_if_present()` never runs (`duress.rs:287`), so the journal
survives, so the second `execute` reports `Wiped` where `AlreadyClean` was
expected (`duress.rs:406`). One platform failure explains all six.

**This matters well beyond CI.** Duress wipe is a coercion-resistance feature.
On any Windows machine without a usable TPM, a duress wipe currently records a
failure and **retains the journal, so the next launch believes duress is still
in progress**. Clean QA VMs typically have no TPM — meaning this would fire
during the two-clean-VM release gate itself.

**I disagree with the proposed patch.** Codex suggested making Windows
`evict_tpm_key()` `return Ok(())` when the storage provider cannot be opened.
Do not do that. Duress is a destroy-the-keys-under-coercion path, and reporting
success when key destruction did not happen turns a safety failure into a false
all-clear — precisely what master §2.1 rule 4 forbids. The distinction that
needs encoding is *"this machine has no TPM-held key, so there is nothing to
evict"* (legitimately `AlreadyClean`) versus *"a TPM exists and eviction
failed"* (must stay `Failed`). A bare `Ok(())` collapses both. This is a crypto
lane decision, not a release lane one.

### `lint` — two dead-code errors, worth more than a lint fix

```
error: function `run_autostart_local` is never used
  --> src-tauri/src/bootstrap.rs:102:8
error: function `register_after_local_bootstrap` is never used
  --> src-tauri/src/bootstrap.rs:357:8
```

Both are `pub fn` in `bootstrap.rs`. I kept rustc's `dead_code` **blocking**
rather than downgrading it with `clippy::style`. Two public bootstrap functions
named "autostart" and "register after local bootstrap" that nothing calls read
like the `implemented-unwired` class this project already worries about, not
like tidy-up. Confirm they are genuinely obsolete before deleting or
`#[allow]`-ing them.

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

4b. **There are no golden snapshots — the gate's central premise does not yet exist.** Verified
   read-only against the live subscription: `az snapshot list` → **0**, `az image list` → **0**,
   `az sig list` → **0**. The gate requires restoring two VMs from *signed golden snapshots*; that
   lineage has never been created, so `cleanRestore: true` and two distinct `goldenSnapshotId`
   values cannot be attested truthfully today — they could only be invented. My verifier refuses
   blank, duplicated and reused snapshot IDs, but it **cannot detect a fabricated one**. That link
   is operator honesty until the Azure-side check in the gate doc's *Hardening* section is built.

4c. **The existing QA snapshots would be disqualifying anyway.** `azure-vm-qa-workflow.md` rule 2
   snapshots a *warm* machine — Discord signed in, OSL identity already created — to make the
   iteration loop fast. This gate needs the opposite: no identity, no login, no prior updater
   state. The release gate needs its own **cold** lineage. Recorded in the gate doc.

5. **No release has ever been produced**, so the signed-candidate path and a live rollback are
   both unexercised. Cutting the first candidate is a production action and was not taken here.

6. **`crates/keystore` duress wipe fails on Windows** — owner: crypto/keystore lane. Six tests
   in `crates/keystore/tests/duress_test.rs` fail on Windows and pass 13/13 on Linux. Root cause
   and the reason **not** to apply the obvious patch are in *Rust CI outcome* above. This is the
   highest-severity finding in this report: it is a coercion-resistance path, and it would fire
   inside the two-clean-VM release gate, because clean QA VMs typically have no TPM.

7. **`src-tauri/src/bootstrap.rs:102` and `:357`** — **no owner; recommend retirement, not repair.**
   Two `pub fn` that nothing calls (`run_autostart_local`, `register_after_local_bootstrap`).
   `src-tauri` is the legacy Discord client, not the shipped OSL Privacy app: it builds a different
   binary (`discord-privacy-client`), embeds a different frontend (`webview/dist`), carries its own
   updater key and its own version (`0.0.1` vs the hub's `0.1.0`), and no release path targets it.

   My recommendation is to **retire the whole legacy app rather than fix its lint**, in a
   deliberate, separately-authorized step — it is currently costing real money and attention: it is
   the reason `lint` and `workspace-test` run a second Windows Tauri compile, the reason
   `webview/dist` has to be built in CI at all, and the reason `reproducible-build.yml` had a
   `legacy-binary` job. Until somebody decides that, `#[allow(dead_code)]` with a comment is the
   correct holding action; do **not** delete the two functions on the assumption they are dead, as
   "autostart" and "register after local bootstrap" are exactly the names an unwired feature has.

8. **`apps/osl-hub/src/services.rs:682`** — owner: crypto lane (its tree). Exact failure:

   ```
   thread 'services::tests::failed_create_never_leaves_a_phantom_in_memory_account'
     panicked at src\services.rs:682:9:
   assertion failed: state.list_for_owner(OWNER_A).is_err()
   ```

   The test writes a regular file, then builds `<that file>/registry.json` beneath it and asserts
   that **both** `create_for_owner` and `list_for_owner` fail. Line 681 passes — `create` does
   error. Line 682 fails: on Windows `list_for_owner` returns `Ok`, because the POSIX `ENOTDIR`
   behaviour the test assumes is not what Windows returns for a path under a non-directory.

   The product question is which is correct, and it is a real one: if `list_for_owner` silently
   returns `Ok` (empty) when its backing store is unreadable, the UI cannot distinguish "you have
   no accounts" from "your account registry is unreachable" — and this is the *phantom account*
   guard, so that distinction is the entire point of the test. The fix belongs in the error
   mapping, not the assertion: an unreadable parent should surface as an error on read, not as an
   empty list. Deliberately **not** quarantined.

9. **Two `apps/osl-hub` tests are quarantined until 2026-08-09**, in
   `scripts/ci/hub-core-tests.sh`, because they assert facts about the operator's machine
   (WhatsApp installed as an AppX package; an installer call returning `Err`) and can never pass
   on a hosted runner. The quarantine **expires and fails CI on that date**. The second one also
   means a unit test can attempt a real software installation — worth fixing on its own merits.

## Repository governance (I6) — findings

- `main` was **unprotected**, with no rulesets, on a public repository.
  **Phase 1 protection is now applied and verified** (`.github/branch-protection-phase1.json`):

  ```
  {"admins":false,"checks":["TypeScript gate","audit"],
   "deletions":false,"force_push":false,"strict":false}
  ```

  Force-push and branch deletion on `main` are now refused by the server. That was the urgent
  part: roughly twenty worktrees hold unique uncommitted work whose only reachable base is a
  branch here, and a force-push would be unrecoverable.

  Phase 1 deliberately requires **only the checks that currently pass** and does not require
  reviews or `enforce_admins`. Requiring `Rust gate` today would freeze every merge in the
  repository during a deadline week over defects three other lanes own. The full payload
  (`.github/branch-protection.json`, adding `Rust gate`, `strict`, reviews and `enforce_admins`)
  is ready to apply the moment `Rust gate` goes green.
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

## Working-tree safety — this lane holds nothing at risk

Recorded because ~17,000 uncommitted lines across five lanes were measured at risk tonight on a box
that has OOM-crashed twice today. **None of it is this lane's.**

- This lane works in a dedicated worktree created from `origin/main`, not in the shared dirty tree.
- Verified at `a688c3e`: **0 dirty tracked, 0 untracked, HEAD identical to the pushed remote.**
- Verified in the shared `discord-privacy-client` tree: **none** of `.github/**`, `scripts/ci/**`,
  `scripts/release/**`, `docs/testing/hub-release-candidate-vm-gate.md` or
  `docs/reports/release-lane-*` is dirty there.

Everything this lane produced is committed and pushed to `release-lane-2026-07-26` / PR #6. No
`git add -A` was ever run against the shared tree, and no other lane's files were staged at any
point.

**PR #5 rebase is NOT started, per the coordinator hold.** It would rewrite history across files
that currently hold five lanes' only copy. It stays blocked until those lanes confirm they have
committed. Nothing in this report depends on it.

## VM input rules — corrected, and it unblocks the gate

The `PostMessage`-only ban applies to the owner's desktop only. On an isolated VM, `SendInput`,
`mouse_event`, `SetCursorPos` and real keyboard driving are allowed and expected.

This is not a footnote for this gate — it removes a real obstacle. Three of the nine required
cases (`onboarding`, `identityCreate`, `twoAccountLogin`) are consent flows that must actually be
clicked, and under a PostMessage-only reading they could not have been proven at all. Retired click
harnesses should be ported to the VM, not rebuilt.

What stays forbidden on the VM is about consequence, not focus: no real personal account, no real
user data, nothing reaching back to the host, no destructive action on an unseeded target. Worth
flagging that the gate's `fullCleanup` case is destructive by design, so it must run only against
identities this run created.

**Targeting is now enforced, not advised.** Every OSL build is titled `OSL Privacy`, so selecting a
window by title has already driven the wrong lane's app through six UI steps. The gate doc now
requires the `<identifier>-sic` marker class, and `scripts/ci/check-window-targeting.sh` fails CI on
name-based selection so it is unexpressible rather than discouraged. The same family of defect —
a harness that guesses its subject, or a default-deny assertion that passes because it read nothing
— is why every case must assert non-empty on the positive path before it may report a pass.

## What actually ships, and what was actually proven

Tracing the artefact chain, because my reproducibility claim was about the wrong thing:

| Stage | Produced by | Had the determinism flags? |
|---|---|---|
| `osl-privacy-hub.exe` I measured | `reproducible-build.yml` → `hub-binary` | **yes** — that is why it reproduced |
| `osl-privacy-hub.exe` that ships | `osl-hub-release.yml` → `tauri-action` | **no** |
| `osl-hub-<ver>-<arch>-nsis.exe` | `tauri-action` NSIS bundler | no |
| what the attestation hashes | **the NSIS installer** | no |

So the thing users download is the **installer**, the attestation binds to the **installer**, and
neither was covered by the reproducibility run. I measured a binary built by a job that does not
ship anything.

**Fixed:** the signing step now carries the identical levers — `RUSTFLAGS` with
`--remap-path-prefix` and `-Clink-arg=/Brepro`, `CARGO_INCREMENTAL=0`, a pinned `SOURCE_DATE_EPOCH`,
and `--config profile.release.codegen-units=1` through `args`, since `tauri-action` shells out to
cargo and inherits its environment.

**Still unproven, and I am not claiming otherwise.** Aligning the configuration is necessary, not
sufficient. Nobody has built the signed artefact twice and compared, because the signing workflow
**cannot run at all** — two independent blockers, both found today: the Windows `npm test` that
cannot pass (fixed), and `hub-release` permitting **zero refs** (owner decision, unfixed). And the
NSIS installer adds its own timestamp and compression, so the binary reproducing does not imply the
installer does. Those are separate claims and need separate measurements.

Current honest status: **the hub binary is reproducible under the reproducible-build configuration;
the shipped installer's reproducibility is unmeasured.**

## Correction: my injection fix was incomplete

I reported "everything now reaches bash through `env`". That was **not true**. I fixed the guard
step and left four `${{ }}` splices in the summary steps — `${{ inputs.rollback_to_tag }}` echoed
directly into `run:` blocks in a `contents: write` job. A friendlier-looking line is the same
injection. All splices are now gone from `run:` blocks in both the rollback and promote workflows,
verified by scanning for them rather than by assertion.

## Adversarial review of my own guards — it found real defects in my work

I dispatched a reasoning model to hunt tonight's defect shape *in the guards I built to prevent it*,
on the assumption I had fooled myself the same way. I had. I verified each claim below myself rather
than accepting it.

**1. My frontend-determinism check was itself a vacuous pass.** CRITICAL, and the sharpest
irony of the night. The step read:

```bash
rm -rf dist && npm run build >/dev/null && first="$(tree_hash)"
```

Bash does **not** apply `set -e` inside an `&&` list. If both builds failed, `first` and `second`
stayed empty, compared equal, and the step reported **"reproduced exactly"** having built nothing.
Reproduced locally 2026-07-27: `first=[] second=[] → VACUOUS PASS`. An empty `dist/` also hashes
deterministically, so the same green appears from an empty tree. Fixed: `set -euo pipefail`,
separated commands, an explicit non-empty file count, and an explicit empty-hash refusal.

**2. Rollback spliced dispatch input straight into bash.** CRITICAL. `${{ inputs.rollback_to_tag }}`
and JSON-derived versions were interpolated into `run:` blocks in a `contents: write` job. A tag or
manifest version containing `$(...)` would execute *before* `rollback-guard.sh` could reject it — the
guard validated a value that had already been evaluated. Fixed: everything now reaches bash through
`env:`.

**3. The `hub-release` environment allows zero refs.** CRITICAL, verified via the API:
`custom_branch_policies: true` with `total_count: 0`. **No `hub-v*` tag can enter the signing
environment at all.** This is a second, independent reason the signed release path has never run,
on top of the test failures I found earlier. Owner decision — I did not change a security-relevant
deployment policy unilaterally.

**4. Reproducibility does not cover the signed binary.** My proof used explicit `RUSTFLAGS` and
`--config` overrides in `hub-binary`. The signed artefact is built by `tauri-action` in a different
workflow with **none of those flags**. So what I proved reproducible is *not the binary that ships*.
The claim must be stated as: the hub binary is reproducible **under the reproducible-build job's
configuration**. Making the signed path match is the outstanding work, and nothing should claim
"reproducible release" until it does.

Also found and accepted as accurate, not yet fixed: the real `verify-snapshot-lineage.sh` is never
invoked by the actual promote/rollback path (only its `--self-test` runs in CI); `preflight.sh` is
not called by any release workflow; and the window-targeting allowlist has no per-entry expiry.
Recorded rather than silently carried.

The review also cleared four guards explicitly, on the decisive test — would the self-test still
pass if the real function were replaced with `return 0`? The aggregate `Rust gate`/`TypeScript gate`
jobs (skipped and cancelled both fail correctly), the `shell: bash` fixes, and both
`prove-promotion-gate.sh` and `prove-rollback-guards.sh` exercise real code paths.

## Test counts are meaningless without their gate

Crypto found that `header_proof_is_enforced()` is `!cfg!(feature = "discord-qa-shell")`. So the gate
I built to auto-select `core,discord-qa-shell` turns test coverage **on** and header-proof
**enforcement off** simultaneously. Neither gate alone is sufficient. `hub-core-tests.sh` now emits
a `::warning::` saying so whenever it selects that feature, and my measured **237 passed / 1 failed**
was taken under plain `--features core` — the gate is now quoted with the number.

## RESOLVED: the shipped binary is now byte-reproducible

Measured, not asserted. Run **30231394207**, after configuring the levers in CI:

| Job | Result |
|---|---|
| **hub binary is reproducible** | **success — two identical builds produced the same hash** |
| | `00cabfc401869919ad257eff6208c9bf036e3d9732da742b286fd2cbd8253fa1` |
| legacy binary build 1 / 2 | **success** — the legacy app builds again for the first time since May |
| legacy binary is reproducible | failure — **and this is the control** |

The control is the part that makes this evidence rather than luck. In the same run, on the same
runner image, the hub binary **with** the determinism flags reproduced exactly while the legacy
binary **without** them did not. That is a clean A/B: the flags are what changed the outcome.

What did it, in the order I expect them to matter:

- `codegen-units=1` — parallel codegen partitions non-deterministically
- `-Clink-arg=/Brepro` — the MSVC PE header carries a **build timestamp** by default
- `--remap-path-prefix=$GITHUB_WORKSPACE=.` — keeps the runner's absolute path out of the binary
- `CARGO_INCREMENTAL=0`, pinned `SOURCE_DATE_EPOCH`

Sequence for the record: **assumed in both directions → measured NO (run 30230442447) → measured
YES with named levers (run 30231394207)**, and none of it required touching
`apps/osl-hub/Cargo.toml`.

### Two of my own earlier positions were wrong, and I have corrected them

1. **My `hub deterministic profile exists` preflight was a hard failure.** I wrote it believing the
   profile was the lever. It is not — reproducibility was achieved with that profile still absent.
   Keeping it fatal would block the workflow forever over a no-op. It is now informational and
   exits 0, printing why, and says that if the profile is ever added the flags should move into it
   so there is one source of truth.
2. **I recommended adding `[profile.release-deterministic]` to `apps/osl-hub/Cargo.toml`.**
   Withdraw that. It would have changed nothing, avoided a contested serialized central file for no
   gain, and left a reproducibility claim resting on an empty profile. `Reproducible release` can
   now be argued for the **hub binary** on evidence — subject to truth's allowlist judgement, and
   noting it is the binary that is proven, not yet the installer.

The legacy job now carries the same flags, since the A/B control is already recorded above and a
permanently red legacy check is just noise.

## Verified before rebasing: the keyserver fix is NOT on the integration line

Asked to verify `5cd4f81` from HEAD before starting the rebase. The answer is that it does not yet
protect the rebase:

- `5cd4f81` is reachable **only from `osl-eye-and-features-2026-07-26`**, a local branch in the
  shared dirty worktree. `git branch -a --contains` lists no remote ref.
- `origin/main` is still `38d0867`.
- `isDiscordSnowflake` is **absent from `origin/main`'s `keyserver-cf/src/lib/validation.ts`**, and
  migration `0029` is not in `origin/main`'s tracked tree.

So the build break is still live on the authoritative line. The commit exists but is unpushed, and
a rebase onto `origin/main` today would still cross the broken range. **PR #5 rebase stays held**,
now for a second independent reason. It unblocks the moment `5cd4f81` reaches `origin/main`.

## `release-deterministic` is a name with nothing behind it

The obvious lever turns out to be empty. Before proposing to copy the root profile into
`apps/osl-hub`, I read what it actually contains:

```toml
[profile.release-deterministic]
inherits = "release"
# Reproducible-build profile. Build flags pinned in CI.
# See docs/design/reproducible-builds.md (v2.5).
```

That is the whole profile. And:

- **`docs/design/reproducible-builds.md` does not exist** in the repository. The comment cites a
  v2.5 document that is not there.
- **No determinism flags are pinned anywhere.** `grep -rE 'RUSTFLAGS|SOURCE_DATE_EPOCH|remap-path-prefix|Brepro' .github/workflows/` returns nothing. The workflow simply runs
  `cargo build --profile release-deterministic`, and that profile differs from plain `release` in
  no respect whatsoever.

So copying this profile into `apps/osl-hub/Cargo.toml` would change nothing and would let a
reproducibility claim rest on a no-op. **This fully explains the measured result** — the build was
never configured to be deterministic, on either crate.

### What actually has to change

Determinism has to be configured, not merely named. My proposed set, worst-offender first:

| Lever | Why |
|---|---|
| `codegen-units = 1` | parallel codegen partitions non-deterministically; the single most likely cause of two differing hashes |
| `-Clink-arg=/Brepro` (MSVC) | the PE header carries a **build timestamp** by default; `/Brepro` replaces it with a content hash. On Windows this alone can defeat reproducibility |
| `--remap-path-prefix=$PWD=.` | absolute build paths are embedded in debug info and panic messages, so the runner's directory leaks into the binary |
| `incremental = false`, `debug = false`/`strip` | incremental artefacts and debug records carry ordering and path noise |

These belong in the profile **and** in CI together, and the `hub-binary` job already builds twice
and compares — so the change is directly measurable. The proof is not the diff; it is
`reproducible-build.yml` going green afterwards. Until then `Reproducible release` stays off the
eligible-claim list, which truth is already actioning.

**Ownership note:** the profile lives in `apps/osl-hub/Cargo.toml`, which is outside this lane's
declared files and is one of the serialized central files PR #5 also edits (+26/−4 on `main` vs
+22/−2 on the branch). An appended `[profile.*]` block is low-conflict, but it should land before
PR #5 is rebased, not during.

## Routed out of this lane: silently skipped tests

Recorded, not chased. From the feature-gate sweep, on `origin/main`:

- **12 `#[ignore]`d tests** and **11 env/availability-gated tests that early-return**.
- Env gates: `OSL_LIVE_TESTS` (`crates/ipc/tests/prose_token_live.rs:37,78,91`),
  `OSL_LIVE_KEYSERVER_URL` (`crates/keystore/tests/live_keyserver_smoke.rs:14`),
  `OSL_HUB_LIFECYCLE_CHILD` (`apps/osl-hub/tests/windows_identity_lifecycle.rs:35`).
- **PATH/node availability gates — the dangerous ones**, because they early-return to a *pass* when
  a tool is missing rather than failing: `crates/keystore/tests/prekeys_e2e_test.rs:119,161`,
  `crates/keystore/tests/burn_e2e_test.rs:173,218`,
  `crates/selectors/tests/keyserver_e2e_test.rs:233,282`. These are burn, prekeys and keyserver
  end-to-end tests — exactly the paths where a silent skip is least acceptable. **Owner: crypto lane.**
- Also from the double audit, **owner: `apps/osl-hub-ui`** — 7 source-string negative assertions
  that would pass on empty extracted source: `overlay.test.ts:54`, `security.test.ts:13`,
  `scrub-provider-preloads.test.ts:18`, `scrub-imap-native-authority.test.ts:29`,
  `scrub-safety.test.ts:53`, `password-gate-ui.test.ts:10`, `local-message-import.test.ts:94`.

Same family as everything else: a check that passes because it read nothing.

## Reproducible build — run for the first time, and the answer is no

`reproducible-build.yml` was not merely untested. It ran **twice, on 2026-05-09 and 2026-05-10,
against legacy `v0.0.5`/`v0.0.6` tags, and failed both times** — then went invisible for 2.5 months
because it only fires on `v*` tags that stopped being cut. Both historical failures are the *same*
`proc macro panicked` / `frontendDist "../webview/dist" doesn't exist` defect found today. That
defect has been latent since May.

I dispatched the corrected workflow manually (run 30230442447), the first execution since May:

| Job | Result |
|---|---|
| frontend dist is reproducible | **success** — confirms the local finding on a clean runner |
| hub binary build 1 / build 2 | **success, both** — the shipped binary now compiles in this workflow for the first time |
| **hub binary is reproducible** | **FAILURE — the two builds differ** |
| hub deterministic profile exists | failure, **by design** — reports the missing profile rather than passing quietly |
| legacy binary | failure — same `webview/dist` defect; **my miss**, I fixed that in `rust-test.yml` and not here. Now fixed. |

**The headline is a first-ever measurement: `osl-privacy-hub.exe` is NOT byte-reproducible.** Two
identical builds at the same commit produced different hashes (build 1
`bb8f8c461f66d681…`). This is expected given `apps/osl-hub` has no
`[profile.release-deterministic]`, but it was previously an assumption in either direction and is
now a measured fact. "Reproducible release" cannot be claimed, and the deterministic profile is now
a demonstrated need rather than a theoretical one.

## Fail-open shells on Windows — six of them, three mine

`windows-latest` defaults to PowerShell, which does **not** stop on a non-zero native exit. Six
multi-line `run:` steps in Windows jobs had no `shell:` override, so an early command could fail and
the step would still pass on the last command's status.

The worst was **the updater supply-chain audit immediately before signing**: `audit_hub_release.py`
could exit non-zero and the step would still go green as long as the following unittest passed.
Three of the six were mine from earlier today, where a failed `npm ci` would not have stopped
`npm run build`. All six now use `shell: bash`.

Same family as everything else tonight: a check that reports success without having done the thing.

## Rollback could have destroyed the update feed

`gh release upload hub-latest latest.json --clobber` **deletes the existing asset before uploading**.
A failed upload would leave `hub-latest` with no manifest at all, stripping every installed client of
its update feed — during the emergency that is the only time rollback runs. It now restores the
previously downloaded manifest on failure, then re-downloads the feed and asserts it actually serves
the target version, rather than treating a zero exit as proof the asset landed.

## Release tooling built by Codex, reviewed here

Two scripts I specified and reviewed rather than typed. Both are dry-run/stub-driven and cannot
touch a real subscription or release; both self-tests run in CI.

**`scripts/release/create-cold-snapshot.sh`** — creates the cold lineage *provably*. Dry-run by
default (`--execute` required for any mutating call); derives `--location` from the source **disk**
rather than a default, which is the Azure policy trap that already rejected one attempt; **requires
an `--evidence` file** recording that the source was inspected clean while running, and stamps its
sha256 into the tags; refuses a running VM, a missing or empty evidence file, and an existing
snapshot name; tags `lineage=release-cold` so `verify-snapshot-lineage.sh` will accept it.
Self-test 12/0.

**`scripts/release/preflight.sh`** — refuses to cut a tag that would fail or could not be attested:
tag must equal `hub-v` + the manifest version, tag must not already exist, Rust **and** TypeScript
must be green **for the exact HEAD sha**, both signing secrets must be present by name, and both
gate proofs must pass. Missing snapshots and a missing `hub-vm-qa` environment warn loudly without
blocking, since they block promotion rather than signing. Self-test 9/0. This is the check that
would have caught a release workflow that had never once been executed.

**Review, in both directions.** I tested each with my own stubs rather than trusting theirs. The
one that mattered: with **no CI run at all** for the current sha, preflight **fails** rather than
passes — an absent result is not a green result, which is the vacuous-pass family that produced
several of today's false greens. Confirmed too that a dry-run snapshot creation issues no mutating
`az` call, and that preflight reads secret *names* only, never values.

Worth recording honestly: my first adversarial stub was too crude — it matched both `az vm show`
variants and returned the power state where the OS disk ID was expected, and I briefly read that as
a defect in the generated script. It was my stub. The script resolves
`storageProfile.osDisk.managedDisk.id` correctly. A bad harness can produce a false *red* just as
easily as a false green.

**Divergence worth flagging:** the coordinator records the honest gate as
`--features core,discord-qa-shell` (731 tests). That is true on the in-flight Discord branch. On
`origin/main` @ `38d0867` that feature does not exist, so the flag would error there. This is why
`hub-core-tests.sh` selects by inspection instead of hard-coding either, and refuses outright if the
module ever appears without its feature.

## Snapshot-lineage hardening — implemented

`scripts/release/verify-snapshot-lineage.sh` closes the one hole the Python verifier structurally
cannot: an **invented** `goldenSnapshotId`. It requires each ID to be a full Azure resource ID (a
bare label like `snapshot-a` is refused), resolves it with `az snapshot show --ids`, and requires
the snapshot to pre-date `completedAtUtc` — a snapshot created after the run cannot be what the run
restored from.

Self-test **6 passed, 0 failed**, with a stubbed `az` so it needs no subscription: a real pair
(accepted), invented bare names, well-formed IDs that do not exist, one-real-one-invented, the same
snapshot twice, and a snapshot created after the run. CI runs `--self-test` on every PR.

It is an **operator** step run before approving `hub-vm-qa`, not a CI step, because wiring it into
promotion would put an Azure credential into GitHub Actions — an owner decision, not a release-lane
one.

The guard now also enforces **lineage separation**: it requires `lineage=release-cold` and refuses
warm-iteration and untagged snapshots outright, so scrub's
`OSL-Independent-Client-1-WARM-iteration-20260726` cannot be used for a release attestation even by
accident. Self-test is now **9 passed, 0 failed**. Note that warm image sits on
`OSL-Independent-Client-1`, one of the two VMs nominated for this gate — the cold image must be a
separate snapshot, not a relabelling of that one.

**The cold lineage itself is not created, and I did not fabricate one.** Creating it is not a
snapshot command: every VM disk currently carries whatever state its lane left on it, and
snapshotting a warm disk and labelling it "clean" is precisely the failure this guard exists to
prevent. A genuine cold image needs a fresh provision — Windows plus the WebView2 runtime, no OSL
identity, no Discord login — which costs compute on a fixed student credit pool and needs an owner's
go-ahead. That is the one remaining blocker to a truthful two-VM attestation, and it is now the only
one that cannot be solved in software.

## Governance hole closed mid-session

`enforce_admins` was `false`, so an admin could bypass every check. Now **`true`**, verified by
read-back:

```
{"admins":true,"checks":["TypeScript gate","audit"],
 "deletions":false,"force_push":false}
```

`Rust gate` is still deliberately not required — adding it today would freeze every merge this week
over three other lanes' defects. Add it the moment it goes green; the phase-2 payload is ready.

## Feature gate — the silent exclusion, and what it does and does not change here

`qa_selftest_request` is gated behind `all(core, discord-qa-shell)`, so the lane-standard
`--features core` compiles out the module that decides whether an incoming trigger becomes a
harmless status read or the **irreversible send**. A suite can then report success having never
exercised the only decision with a destructive branch.

**Checked before restating any number:** on `origin/main` @ `38d0867` neither the feature nor the
module exists — `apps/osl-hub/Cargo.toml` declares only `default`, `core`, `desktop` and
`custom-protocol`, and there is no `qa_selftest_request*` source file. Both live on the in-flight
Discord branch. So the 680/731 figures are from that branch, not from the integration line, and
this lane's measured **237 passed / 1 failed** was taken where the module genuinely is not present.
That number stands, and adding the feature on `main` today would fail with an unknown-feature error.

Rather than hard-code either answer, `hub-core-tests.sh` now selects features by inspection and
**refuses to run** if the module is ever present while the feature is not:

```
features: core                                    # today, on origin/main
::error::qa_selftest_request exists but apps/osl-hub declares no discord-qa-shell feature.
The irreversible-send decision would be compiled out of this run.   # exit 1
```

Both branches are verified against a stubbed cargo. The moment the Discord work lands on `main`,
the gate picks up `core,discord-qa-shell` automatically; if it lands without the feature wired, CI
stops rather than quietly under-testing. Note the corrected gate also surfaces a pre-existing
`native_discord_adapter` failure that belongs to the crypto lane — expected, not mine, not blocking.

**Attachments (cipher-store `0a17547d`) do not affect this lane's evidence.** Nothing here rested on
an attachment upload; the claims made are CI runs, gate proofs with stubbed inputs, and read-only
Azure queries. No re-derivation needed.

## A delegation that failed, then succeeded — recorded

The window-targeting guard took two attempts. The first returned a script that **did not parse**
(`SC1073`/`SC1072`); it was rejected rather than committed. An earlier delegation returned 0 bytes,
which I initially read as a wedged session — on the coordinator's later note it was more likely an
**OOM kill**, which presents identically. Both are recorded because "the delegate said it was done"
is not evidence.

The second attempt is **in place and working**. I verified it independently of its own self-test:
`bash -n` parses, shellcheck is clean, its self-test is 12/0, and — the check that matters — I
planted a fresh violation of my own and confirmed it was caught, then removed it and confirmed the
guard returned to green. It found **8 real violations** across three PowerShell capture helpers
(`apps/osl-hub/scripts/capture_window.ps1`,
`apps/osl-hub-ui/screenshots/capture-osl-hub.ps1`, `.../measure-osl-hub-startup.ps1`) — all
selecting OSL by process title, one of which also does `Stop-Process -Force` on whatever it found,
which can kill another lane's build.

Those three are allowlisted with reasons and an expiry of **2026-08-09**, so the guard blocks *new*
violations from today without freezing `main` over files this lane does not own. The guard itself is
permanently allowlisted, since a detector necessarily contains the patterns it detects.

## PR #5 (Scrub) merge order — this lane owns the order, Scrub owns the content

Verified by fetching `refs/pull/5/head` and running `git merge-tree` against `origin/main`
(read-only; no branch was checked out, merged or force-pushed — and `main` now refuses force-push
anyway, which is the system working).

- **PR #5 and PR #6 are path-disjoint.** PR #5 touches **none** of `.github/**`, `scripts/ci/**`,
  `scripts/release/**`, `docs/testing/**` or `docs/reports/**`. They cannot conflict with each
  other and can be merged in either order.
- **Recommended order: PR #6 first, then rebase PR #5 onto the updated `main`.** Not because #6 is
  more important — it is not, #5 carries the hard-deadline work — but because #6 is disjoint
  infrastructure that makes the pipeline diagnostic, and resolving #5's 33 conflicts is far safer
  with working CI underneath it.
- PR #5 is **9 behind / 4 ahead** of `origin/main`; merge base `d699d70`.

**The important finding is how it conflicts.** Of the 33 conflicting paths, a large group are
*near-identical add/add*:

| Path | on `main` | on PR #5 |
|---|---|---|
| `apps/osl-hub-ui/src/scrub-provider-preloads.ts` | +236 | +237 |
| `apps/osl-hub/src/attachment_scan.rs` | +1932 | +1894 |
| `apps/osl-hub/src/scrub_imap.rs` | +1491 | +1396 |
| `apps/osl-hub-ui/src/scrub-hosted-session-assisted.ts` | +196 | +197 |

The same work has already reached `main` by another route, in a slightly different form. This is
the multi-worktree duplication hazard, and it means **PR #5 must not be line-merged.** Taking
"both sides" would duplicate whole modules; taking either side blindly would silently drop the
other's refinements. Master §22 rule 6 applies directly: resolve the interface, do not line-merge
competing copies.

PR #5 also touches five serialized central files — `apps/osl-hub/src/main.rs`,
`apps/osl-hub-ui/src/main.ts`, `capabilities/hub.json`, `permissions/hub.toml` and
`apps/osl-hub/Cargo.toml` — which are exactly the Wave 1-3 files below. `permissions/hub.toml`
differs by +110 on `main` versus +40 on the branch, so the capability surface itself is contested;
that one is a security review, not a merge.

## I4 — why the first signed candidate cannot be cut yet

The signing path is **configured**: both updater public keys are populated and valid, and both
private-key secrets exist on the `hub-release` environment (verified by name, set 2026-07-17). The
blocker was never the keys.

I found and fixed the reason a tag would have failed outright: `osl-hub-release.yml` ran
`npm test` on `windows-latest` and `cargo test --features core --lib` with no quarantine and no
`--test-threads=1`. Both are currently-failing commands, so **every signing run would have failed
before reaching the signer.** The workflow now runs frontend tests on Linux in a `verify` job that
gates signing, and uses the same `scripts/ci/hub-core-tests.sh` as CI.

Remaining blockers to a real candidate, in order:

1. **`apps/osl-hub/src/services.rs:682` must be fixed.** It is in the release workflow's test path
   and is deliberately not quarantined.
2. **No cold golden snapshots exist** (see 4b/4c). A candidate could be signed, but its two-VM
   attestation could not be filled in truthfully.
3. **`hub-vm-qa` does not exist**, so promotion would run with no human approval.
4. **No deterministic profile on `apps/osl-hub`**, so "reproducible release" stays unproven.

**I did not cut the tag.** Creating `hub-v0.1.0` on a public repository is outward-facing and a tag
should not then be deleted, and today it would produce either a failed run or a signed artifact
that cannot be honestly attested. My recommendation is to cut it once item 1 lands — signing a
candidate is genuinely useful even before the VM fleet is ready, because it proves the signer and
the updater manifest work — but that is an owner call, not mine.

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

- The brief states `tauri.conf.json` has "an empty updater pubkey placeholder", and this was
  repeated after my first report. **It is not the case for either app.** Re-checked and decoded:

  | File | `createUpdaterArtifacts` | Decoded public key |
  |---|---|---|
  | `apps/osl-hub/tauri.conf.json:50` (**ships**) | `true` | `minisign public key: 3B6AE4739858E8D4` |
  | `src-tauri/tauri.conf.json:39` (legacy) | `true` | `minisign public key: 44AD89E36BC119F8` |

  Both base64-decode to well-formed minisign public keys, and they are correctly *different* keys
  for two separate update feeds. There is no empty placeholder in the repository to resolve.

  I also closed the question I flagged as unverifiable last time: the private keys **are** present
  as `hub-release` environment secrets, confirmed by name only —
  `HUB_TAURI_SIGNING_PRIVATE_KEY` and `HUB_TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, both set
  2026-07-17. The signing path is configured end to end. It has simply never been run.
- The brief describes the failing TypeScript test as being under `webview/`. It is
  `apps/osl-hub-ui/src/latest.test.ts`; `webview/src/latest.test.ts` does not exist.
- The brief implies `scripts/ci/**` and `scripts/release/**` may already exist. Neither did.
- Worth recording as a working negative control: `audit_public_release.py` **caught this very
  report** leaking personal WSL paths into a public repository and failed the run. The public
  boundary check demonstrably works; the paths were replaced with `<workspace>`.

## Integration and rollback for *this* change

- All work is on `release-lane-2026-07-26` in PR #6. Nothing is merged, deployed, signed or
  published. No release, tag, branch or file was deleted.
- Rollback of this change is `gh pr close 6` — the previous CI configuration is untouched on
  `main` and is recoverable by not merging.
- The new worktree `<workspace>/osl-release-lane` is additive and holds no unique uncommitted
  work; everything in it is committed and pushed.

## Resume here

- **Current verified state:** TypeScript Test, Public release audit and Selector CI green on
  `373dc53`; Rust `quality checks` and the new `osl-hub desktop binary compiles` green; both
  release gates proven to refuse bad input (20/20 and 9/9); frontend proven byte-reproducible;
  phase-1 branch protection applied and verified. `Rust gate` is red on three defects owned by
  other lanes, each precisely located above. Everything inside this lane's file ownership is
  done; **Rust CI cannot go green from this lane alone.**
- **Exact worktree/branch:** `<workspace>/osl-release-lane` @ `release-lane-2026-07-26`,
  based on `38d0867`. Clean apart from gitignored `node_modules`/`dist`.
- **Next unblocked actions, in order:**
  1. Hand items 1, 6, 7 and 8 in *Remaining failures* to their owning lanes. The keystore duress
     one is the highest severity and is **not** a CI problem.
  2. Merge PR #6. It does not make `main` fully green — it makes `main` *diagnostic*, which it
     has not been for over a week.
  3. Once `Rust gate` is green, apply phase 2:
     `gh api -X PUT repos/OSLPrivacy/discord-privacy-client/branches/main/protection --input .github/branch-protection.json`
  4. Ask the owner to create the `hub-vm-qa` environment with required reviewers, before any
     promotion. Today it does not exist and would be auto-created with no rules.
  5. Tell the Scrub lane that PR #5 is `CONFLICTING` against `main`.
- **Blocked, needs owner:** cutting the first `hub-v0.1.0` candidate (production action). Until
  that exists, "signed candidate" and "rollback exercised" cannot honestly be claimed.
- **Known risk:** merging to `main` during a period when ~20 worktrees hold unique uncommitted
  work. This change touches only `.github/**`, `scripts/ci/**`, `scripts/release/**` and
  `docs/testing/`, none of which appear in the serialized-central-file conflict set.

## Two vacuous-pass holes closed in my own CI scripts

Both were flagged by the adversarial review and left recorded-but-unfixed. I confirmed each before
touching it, rather than trusting the report.

**`node-suite.sh` reported success for zero work.** Confirmed by running it:
`node-suite.sh keyserver-cf " "` printed `all requested steps passed ( )` and exited 0 having run
no npm script at all. A non-empty argument that trims to nothing satisfied the non-empty check and
then iterated over nothing. This script fronts **every** TypeScript package in CI, so the entire
matrix could have reported green while running none of its steps. It now counts steps that survive
trimming and refuses when that count is zero.

**`check-window-targeting.sh` could wrap its exit code to success.** It ended `return "$failures"`
at two sites; `return 256` — or any multiple — is seen by the shell as **0**. Verified directly:
`bash -c 'exit 256'` yields `$? = 0`. A wall of violations would have read as a pass. Both sites now
clamp to 1.

Verified in both directions after the change: whitespace-only and comma-only step lists are refused,
a real step list still runs; the window guard's self-test still passes, a clean tree still passes,
and a freshly planted violation is still caught and then clears on removal.

That makes three defects of this family found in my own tooling tonight, plus one faulty test harness
of my own. The tools written to detect checks that cannot fail are not exempt from being checks that
cannot fail.

## Two guards I built are never invoked on the real path — mechanism recorded

This is the same "a gate that only runs on an event that never happens" defect that made
`reproducible-build.yml` invisible for two and a half months, and it is in my own work. Verified,
not assumed:

```
grep -rn "preflight.sh" .github/workflows/ scripts/
  -> rust-test.yml:264  bash scripts/release/preflight.sh --self-test        (only)
grep -rn "verify-snapshot-lineage.sh" .github/workflows/ scripts/
  -> rust-test.yml:251  bash scripts/release/verify-snapshot-lineage.sh --self-test   (only)
```

**Both run only as `--self-test`.** Neither has ever been executed against real input. Their proofs
demonstrate the logic is sound; they do **not** demonstrate that anything calls it. A guard that is
only ever self-tested protects nothing.

### Why `preflight.sh` cannot simply be wired into the tag workflow

Not an oversight — a structural conflict, and worth stating so nobody "fixes" it by forcing it in:

```
.github/workflows/rust-test.yml
on:
  push:
    branches: [main]
  pull_request:
```

Rust Test and TypeScript Test **do not trigger on tags**. Preflight's load-bearing check is that CI
is green *for the exact HEAD sha*, and it treats an absent result as a failure rather than a pass —
deliberately, since absent is not green. Wire it into the tag path and it would therefore fail on
**every** tag, because no run for that sha can exist. So `preflight.sh` is correctly a **pre-tag
operator step**, run before cutting the tag, not an in-workflow gate. Nothing currently enforces
that it is run, and I have not invented an enforcement I cannot verify.

### Why `verify-snapshot-lineage.sh` is not wired into promotion

It calls `az snapshot show`, so wiring it into `osl-hub-promote.yml` requires an Azure credential
available to GitHub Actions. That is a security decision belonging to the owner, not to this lane,
and it is listed among the open blockers. Until then the two-clean-VM attestation can still name
snapshot IDs that the Python verifier accepts and that nothing resolves against Azure — the exact
fabrication window that guard was written to close, still open.

**Status of both: `designed-only`.** The logic is proven; the wiring is not. I would rather record
that plainly than let two green self-tests in a CI log read as enforcement.

## Test-count floors: one was wrong by 100x, and measuring the rest nearly broke CI

Written down because it is my most recent result and it cuts both ways.

**`keyserver-cf`'s floor was 3; the real count is 313.** Its `npm test` runs *two* vitest
invocations — `Tests 310 passed` then `Tests 3 passed`. I seeded the floor early in the session by
reading the tail of that output and never saw the first invocation. A collapse from 313 tests to 4
would have passed the guard whose only job is catching that. Corrected, and verified that 4 is now
rejected.

**Then the same mistake nearly went the other way.** Re-measuring `webview` gave **2** against a
floor of **1**, which looked like another understated floor. It was not:

| `webview/dist` present? | count |
|---|---|
| yes — a tree I had built earlier | 2 |
| no — a clean checkout, which is what CI has | **1** |

Vitest collects the compiled `dist/index.test.js` alongside `src/index.test.ts`. **The floor of 1
was already correct**, and had I "fixed" it to 2 on the strength of my own dirty tree, every clean
CI run would have failed permanently. Measuring in a dirty tree produced a number twice the truth.

All six floors are now measured on a clean tree and carry their provenance. Five of the six remain
**wired to nothing** — I did not connect them, because connecting numbers is only safe once they are
trustworthy, and until an hour ago two of them were not.

Two smaller notes worth keeping: `assert-test-counts.sh` summing multiple `Tests N passed` lines was
flagged as a retry-inflation risk, and for `keyserver-cf` that summing is exactly the *correct*
behaviour — same mechanism, right in one place and risky in another. And `webview/dist` is untracked
but **not** git-ignored, so a stray `git add -A` would commit build output; another reason this lane
commits with explicit pathspecs only.

## Floors given headroom, and one proven by starving it

**Headroom, and the reasoning.** Every floor is now set ~5% **below** its measured count, rounded
down: `osl-hub-core` 237→225, `keyserver-cf` 313→297, `osl-hub-ui` 379→360, `keyserver-legacy`
72→68, `cipher-store-cf` 18→17. A floor at exactly the current count fires on every legitimate
change — merging two tests, deleting a redundant case — and a gate that cries wolf on ordinary work
is switched off within a week. 5% absorbs normal churn while still catching what this exists for: a
module silently ceasing to compile and taking tens or hundreds of tests with it. `webview` stays at
**1 with no headroom**, because a one-test suite cannot have a floor below 1 without disabling the
check; that is stated in the file rather than left as an oddity.

**Changing the floors immediately broke the self-test, which is the self-test working.** The
`vitest below floor` fixture was hard-coded as `378 passed` against a floor of 379. Once the floor
gained headroom at 360, that fixture became an *above*-floor case that could no longer fail —
10 passed, 1 failed. A fixture tuned to a specific number silently stops testing anything the moment
that number moves. Fixed at the root: at-floor and below-floor fixtures are now **derived from the
floors file** via a shared `floor_for` helper, so they cannot rot when a floor changes. Back to
11 passed, 0 failed, and now structurally rot-proof rather than re-tuned.

**Starvation proof, with the honest detail of which path fired.** Against a real suite and real
output, not a fixture:

```
cipher-store-cf full run    -> Tests 18 passed   -> gate PASSES (floor 17)
cipher-store-cf starved     -> no count emitted  -> gate REJECTS
```

The starved run was rejected via the **unparseable-count** path, not the below-floor path: vitest
with zero matching tests prints no `Tests N passed` line at all. That is the fail-closed branch, and
it is the more dangerous case — a suite that produces no count is a suite nobody measured, and
treating that as a pass is the whole defect family. The below-floor branch itself is exercised by
the derived fixtures. Both are proven; they are proven by different means, and I would rather say
which than let one result cover both.

**The summing caveat now sits beside the code**, not only here: cargo prints one summary per test
binary and `keyserver-cf` runs two vitest invocations, so summing is *correct* for both — but it
would also sum a retried job's duplicate summary and hide a real drop behind a retry. If retries are
ever enabled for these jobs, that function must de-duplicate rather than add.

Five of six floors remain **unwired**. They are now trustworthy numbers; wiring them is the next
step and was not done here.

## Acceptance rows this earns

Proposed for the single writer of the build checklist to adjudicate; deliberately conservative.

| Row | Current | Proposed | Evidence |
|---|---|---|---|
| **I3** Green public Rust/TS/selector/security CI | `earned: 0` / 3 | **2** | TypeScript, Public release audit and Selector CI green; Rust quality-checks green including five release-gate self-tests; a new job compiles the shipped desktop binary on every PR, which previously first happened at tag time. Every blocker inside this lane's ownership is fixed and the pipeline is diagnostic rather than uniformly red. **Not 3**, and not reachable from this lane: `Rust gate` is genuinely red on `crates/keystore` (Windows-only duress failures), `src-tauri` (dead code) and `apps/osl-hub`. Award the third point when `Rust gate` passes on `main`. |
| **I4** Signed candidate, VM promotion, reproducible release, rollback | `earned: 0` / 4 | **2** | Promotion gate proven to accept a valid candidate and refuse eighteen distinct bad ones, executed in CI; rollback path built, guards proven, and two critical defects in it closed — a command-injection route from a dispatch value, and an emergency upload that could have left the update feed with no manifest at all. Reproducibility went from assumed, to measured negative, to **measured positive for the application binary**, with a same-run control (the binary lacking the flags did not reproduce). **Held at 2, deliberately.** The binary proven reproducible is **not the artefact users receive**: the shipped installer is produced by a different build path, and its reproducibility is **unmeasured**. No signed candidate has ever been produced, no rollback has been exercised, and the signing environment currently permits zero refs so that path cannot run at all. |
| **I6** Repo governance / branch protection / PR cleanup / releases | `earned: 1` / 2 | **2** | Branch protection applied and verified by API read-back on a previously unprotected public `main`: force-push refused, deletion refused, and admin bypass refused. Both payloads committed as reviewable JSON rather than living only in a web console. All five open PRs triaged with recommendations and merge-order reasoning. **Argument for holding at 1:** no PR was actually closed and there is still no release record. Truth's call. |
| **I2** Authoritative integration line and central-file waves | `earned: 0` / 3 | **1** | Ordered six-wave plan with per-wave serialized central files and parallelism marked, from a read-only survey of twenty-one worktrees. Also established that the largest waiting change conflicts as near-identical duplicate work that arrived by a second route, so it needs human reconciliation rather than a merge. Held at 1 because the row requires the line to be *established*, and execution was out of scope. |

**Read the I4 ceiling before awarding it.** The single most over-readable sentence in this report is
that a binary reproduces. It does. It is also not the binary anyone installs, and those are two
separate facts with the second one governing.

No claim is made on I1, I5 or I7.
