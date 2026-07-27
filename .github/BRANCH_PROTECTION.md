# Branch protection and release environments

As of 2026-07-26 `main` on this **public** repository had no branch protection
and no rulesets at all (`gh api .../branches/main/protection` returned 404), so
every check described below was advisory and anyone with write access could push
straight to `main`. This document is the applied configuration, not a wish list.

## Required status checks

Two aggregate checks gate `main`. They exist specifically so that adding,
renaming or re-sharding a CI job never silently un-enrols a required check —
GitHub matches required checks **by name**, and a matrix job's name changes
whenever its matrix does.

| Required check | Workflow | What it aggregates |
|---|---|---|
| `Rust gate` | `rust-test.yml` | workspace build+test, osl-hub core tests, osl-hub desktop binary compile, lint, quality checks |
| `TypeScript gate` | `ts-test.yml` | webview, osl-hub-ui, keyserver-cf, cipher-store-cf, legacy keyserver |
| `Public release audit` | `public-release-audit.yml` | `scripts/audit_public_release.py` |

Do **not** enrol the individual jobs (`lint`, `osl-hub core tests`,
`osl-hub-ui`, …). They are already required transitively through the two gate
jobs, and enrolling them directly reintroduces the rename problem.

## Applying it with the CLI — in two phases, on purpose

Requiring `Rust gate` today would freeze every merge, because `Rust gate` is
genuinely red on defects in `crates/keystore`, `src-tauri` and `apps/osl-hub`
that this lane does not own. Locking the repository during a deadline week to
enforce a gate nobody can currently pass is worse than the problem. So the
protection is applied in two phases.

**Phase 1 — applied 2026-07-26.** Protects against the unrecoverable, blocks
nothing routine:

```bash
gh api -X PUT repos/OSLPrivacy/discord-privacy-client/branches/main/protection \
  --input .github/branch-protection-phase1.json
```

This requires only the checks that actually pass (`TypeScript gate`, `audit`)
and, most importantly, sets `allow_force_pushes: false` and
`allow_deletions: false`. It deliberately does **not** require reviews or
`enforce_admins`, so no in-flight work is blocked.

**Phase 2 — apply once `Rust gate` is green.** The full payload:

```bash
gh api -X PUT repos/OSLPrivacy/discord-privacy-client/branches/main/protection \
  --input .github/branch-protection.json
```

Both payloads live in the repository so the configuration is reviewable in a
diff rather than existing only inside the GitHub UI.

Verify with:

```bash
gh api repos/OSLPrivacy/discord-privacy-client/branches/main/protection \
  --jq '{checks: .required_status_checks.contexts,
         strict: .required_status_checks.strict,
         reviews: .required_pull_request_reviews.required_approving_review_count,
         admins: .enforce_admins.enabled,
         force_push: .allow_force_pushes.enabled,
         deletions: .allow_deletions.enabled}'
```

## Deliberate settings

- **`allow_force_pushes: false` and `allow_deletions: false`.** Roughly twenty
  worktrees currently hold unique uncommitted work whose only reachable base is
  a branch in this repository. A force-push or branch deletion is unrecoverable
  for that work, so both are refused at the server rather than by convention.
- **`enforce_admins: true`.** A red pipeline under deadline pressure is exactly
  when someone reaches for the bypass.
- **`strict: true`** (branches must be up to date before merging), because the
  integration line is `main` and several long-lived branches are 9+ commits
  behind it.
- **One approving review, with stale reviews dismissed on new commits.**

## Release environments

Two protected environments carry the release path. They are separate on
purpose: the identity that can *sign* a build must not be the identity that
decides a build is *fit to publish*.

| Environment | Used by | Must have |
|---|---|---|
| `hub-release` | `osl-hub-release.yml` | signing secrets; required reviewers |
| `hub-vm-qa` | `osl-hub-promote.yml`, `osl-hub-rollback.yml` | required reviewers; **no signing secrets** |

> **Open finding (2026-07-26):** `hub-release` exists with `required_reviewers`
> and `branch_policy`. **`hub-vm-qa` did not exist.** A `workflow_dispatch` job
> naming a non-existent environment does not fail — GitHub creates it on first
> use with *no* protection rules — so the "separate human approval" in front of
> promotion was not being enforced by anything. It must be created with required
> reviewers before the first promotion, or the two-VM gate is advisory.

## When to update

Add every new aggregate gate here. If you add a CI job, add it to the existing
gate job's `needs:` list rather than enrolling it as a new required check.
