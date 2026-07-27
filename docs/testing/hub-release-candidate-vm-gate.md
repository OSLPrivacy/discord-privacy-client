# OSL Privacy release-candidate VM gate

A signed candidate is **not a release**. The tag workflow creates a draft and
cannot move `hub-latest`. Promotion requires a separate `hub-vm-qa` protected
environment approval and a `hub-vm-qa-attestation.json` asset on that draft.

## The three workflows

| Workflow | Trigger | Environment | Effect |
|---|---|---|---|
| `osl-hub-release.yml` | push of a `hub-v*` tag | `hub-release` (signing secrets) | builds and signs a **draft**; cannot publish |
| `osl-hub-promote.yml` | manual dispatch | `hub-vm-qa` (human approval) | publishes the draft and moves `hub-latest` |
| `osl-hub-rollback.yml` | manual dispatch | `hub-vm-qa` (human approval) | repoints `hub-latest` at an earlier attested release |

The signing identity and the publishing identity are deliberately different
environments. Whoever can produce a signed binary must not also be able to
decide, alone, that it is fit to ship.

> **Blocking prerequisite (open as of 2026-07-26):** the `hub-vm-qa`
> environment **does not exist** on the repository. A workflow that names a
> missing environment does not fail — GitHub creates it on first use with no
> protection rules — so promotion and rollback would run with *no* human
> approval at all. Create it with required reviewers before the first
> promotion, or this entire gate is advisory. See `.github/BRANCH_PROTECTION.md`.

## Running the gate

Before attaching the attestation, restore two distinct Windows VMs from signed
golden snapshots with no OSL identity, service profile, login, or prior updater
state. Install the exact draft `.exe` on both VMs and record its SHA-256.

Both VMs must cover onboarding, identity creation/recovery, two disposable
service-account logins, persistence across restart, signed updating, one-sided
and two-sided encryption, and full cleanup. Pause for the operator at CAPTCHA,
2FA, or provider security checks; never automate around them or put credentials
in the attestation, logs, screenshots, or repository.

Use this bounded attestation shape:

```json
{
  "schemaVersion": 1,
  "candidateTag": "hub-v0.1.0",
  "candidateSha256": "<64 lowercase hex characters>",
  "completedAtUtc": "2026-07-17T23:00:00Z",
  "operator": "<reviewer identity>",
  "captchaHandling": "paused_for_manual_completion",
  "vms": [
    {"name": "OSL-QA-A", "goldenSnapshotId": "<signed snapshot A>", "cleanRestore": true},
    {"name": "OSL-QA-B", "goldenSnapshotId": "<signed snapshot B>", "cleanRestore": true}
  ],
  "cases": {
    "onboarding": true,
    "identityCreate": true,
    "identityRecover": true,
    "twoAccountLogin": true,
    "persistenceRestart": true,
    "signedUpdate": true,
    "oneSidedEncryption": true,
    "twoSidedEncryption": true,
    "fullCleanup": true
  }
}
```

Validate locally before upload:

```powershell
python scripts/verify_hub_vm_qa_attestation.py `
  --tag hub-v0.1.0 `
  --candidate-dir .\candidate `
  --attestation .\candidate\hub-vm-qa-attestation.json
```

Then upload the JSON to the draft release and manually run **Promote VM-tested
OSL Privacy candidate**. Do not approve the `hub-vm-qa` environment unless
the verifier is targeting the exact installer tested on both clean VMs.

## Proving the gate actually refuses

A gate that has only ever been run against input designed to pass cannot be
distinguished from one that always returns success. Two proof scripts run on
every pull request via `rust-test.yml`'s quality-checks job, and can be run by
hand at any time:

```bash
bash scripts/release/prove-promotion-gate.sh
bash scripts/release/prove-rollback-guards.sh
```

`prove-promotion-gate.sh` builds a synthetic candidate that passes, then mutates
one field at a time and requires a refusal for the stated reason. Measured
2026-07-26: **20 passed, 0 failed**, covering a substituted binary, a mismatched
tag, a malformed hash, a single-VM run, the same golden snapshot restored twice,
a reused VM name, `cleanRestore` attested as the string `"true"`, an omitted
case, an unknown extra case, a failed case, an automated CAPTCHA, a blank
operator, a non-UTC timestamp, a missing updater manifest, two ambiguous
installers, and no installer at all.

`prove-rollback-guards.sh` does the same for rollback. Measured 2026-07-26:
**9 passed, 0 failed**, including the two that matter — refusing to point the
update feed at a draft that was never published, and refusing a "rollback" onto
the version already being served.

## Rollback

Rollback repoints the `hub-latest` update feed at an earlier release. It does
**not** delete, unpublish, or rewrite anything; the withdrawn release stays
exactly where it is. Clients that already installed follow the feed.

A rollback target must be a published `hub-v*` release that **itself** passed
the two-clean-VM gate: `osl-hub-rollback.yml` re-runs the same attestation
verifier against the target's own installer before moving the feed. You
therefore cannot roll back onto a build that skipped QA, and you cannot roll
back onto a draft.

> **Not yet exercised end to end.** As of 2026-07-26 the repository has **zero
> releases and zero `hub-v*` tags**, so no live rollback rehearsal is possible:
> there is nothing to roll back to. The decision logic is proven offline by
> `prove-rollback-guards.sh`, and the first genuine rehearsal is blocked on
> cutting the first candidate. Do not record this gate as exercised until a
> real feed move has been performed and observed from an installed client.

## Reproducibility

`reproducible-build.yml` builds the shipped binary twice and compares SHA-256.
Note that `apps/osl-hub-ui/dist` is embedded at **compile** time, so the
frontend is hashed too — a non-deterministic UI makes a deterministic binary
impossible. The frontend was verified byte-reproducible on 2026-07-26.

The shipped crate `apps/osl-hub` declares its own `[workspace]` and therefore
does not inherit the root `[profile.release-deterministic]`. Until that profile
is added to `apps/osl-hub/Cargo.toml`, the binary users install has no
deterministic build profile, and `reproducible-build.yml` reports that as a
hard failure rather than passing quietly.
