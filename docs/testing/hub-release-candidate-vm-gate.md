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

> **Two different things, both required.** The GitHub `hub-vm-qa` environment is the *approval*
> mechanism — it is what forces a human to click before the feed moves. The Azure fleet below is
> the *execution* mechanism — it is where the two clean VMs actually come from. Neither replaces
> the other, and both are currently incomplete.

> **Blocking prerequisite A (open as of 2026-07-26):** the `hub-vm-qa` environment **does not
> exist** on the repository. A workflow that names a missing environment does not fail — GitHub
> creates it on first use with *no* protection rules — so the "separate human approval" in front of
> promotion is currently enforced by nothing. Create it with required reviewers before the first
> promotion. See `.github/BRANCH_PROTECTION.md`.

> **Blocking prerequisite B (open as of 2026-07-26):** **there are no golden snapshots.** Verified
> read-only against the live subscription: `az snapshot list` → 0, `az image list` → 0,
> `az sig list` → 0. This gate's central premise is restoring two VMs from signed golden
> snapshots, and that lineage has never been created. Until it exists, `cleanRestore: true` and two
> distinct `goldenSnapshotId` values cannot be attested truthfully — they could only be invented.
> The verifier rejects blank, duplicated and reused snapshot IDs, but it **cannot detect a
> fabricated one**; that link in the chain is operator honesty. See *Hardening* below.

## The VM fleet

Testing runs on the Azure fleet documented in `docs/testing/azure-vm-qa-workflow.md`, not on the
owner's desktop. A virtual monitor is visual separation only — Windows scopes the input queue to a
desktop object, so a process on a second monitor can still steal the owner's foreground window.

Verified live 2026-07-26: **10** Windows VMs across 5 resource groups, all `VM deallocated`
(the workflow doc says 8; `OSL-SIGNAL-QA-SCUS` with `OSL-Signal-Client-1/-2` is not listed there).
Subscription is **Azure for Students** — a fixed credit pool, not a billing account.

For this gate use two VMs **in different resource groups**, so "two distinct VMs" is structurally
true rather than a naming convention:

| Role | VM | Resource group |
|---|---|---|
| `OSL-QA-A` | `OSL-Azure-Client-1` | `OSL-TWO-CLIENT-LAB` |
| `OSL-QA-B` | `OSL-Independent-Client-1` | `OSL-TWO-CLIENT-LAB-INDEPENDENT` |

```bash
az vm start      -g OSL-TWO-CLIENT-LAB -n OSL-Azure-Client-1
# ... run the gate ...
az vm deallocate -g OSL-TWO-CLIENT-LAB -n OSL-Azure-Client-1
az vm list -d --query "[?powerState=='VM running'].name" -o tsv   # leak check, must be empty
```

**Deallocate when finished.** A `D2s_v3` left running is the only way this costs real money.

### Creating the cold lineage — it must be *provably* cold

Two lineages share this subscription and must never be confused. Scrub's first snapshot,
`OSL-Independent-Client-1-WARM-iteration-20260726`, is tagged `lineage=warm-iteration` with a
purpose string saying it is not a release-gate image. Follow that model exactly:

| | Warm (scrub) | Cold (this gate) |
|---|---|---|
| tag | `lineage=warm-iteration` | `lineage=release-cold` |
| name | `<vm>-WARM-iteration-<date>` | `<vm>-COLD-releasegate-<date>` |
| contents | Discord signed in, OSL identity present | Windows + WebView2 runtime only |

`verify-snapshot-lineage.sh` **requires `lineage=release-cold`** and refuses anything else,
including an untagged snapshot — absence of evidence is not a clean restore. A warm image therefore
cannot be used for a release attestation even by mistake.

**Pin the region.** Azure policy rejects a snapshot that inherits a default region instead of the
disk's; scrub hit this on the first attempt:

```bash
az snapshot create -g OSL-TWO-CLIENT-LAB -n OSL-Azure-Client-1-COLD-releasegate-20260726 \
  --source <disk-id> --incremental true --location northcentralus \
  --tags lineage=release-cold owner=release-lane \
        purpose="Release-gate cold image, no OSL identity or service login"
```

**The part that actually matters.** Scrub's warm snapshot was taken from a *deallocated* disk, so
its contents are unverified. That is fine for an iteration base. **It is not fine here.** A cold
lineage whose cleanliness was never checked cannot back a `cleanRestore: true` attestation — "we
did not look" is precisely the fabrication the verifier cannot detect. So the cold image must be
created by a path that *proves* it is cold, one of:

1. **Fresh provision** from a clean Windows Marketplace image, WebView2 runtime added, never logged
   into any service, snapshotted before OSL is ever installed. Strongest, and the default choice.
2. **Verified wipe**, if reusing an existing VM: start it, prove absence *while it is running* —
   no `%APPDATA%` OSL identity directory, no Discord profile, no prior updater state — record that
   output as the evidence artefact, then deallocate and snapshot.

Either way the emptiness evidence is recorded and referenced from the attestation. A snapshot of a
deallocated disk nobody inspected is not a cold image; it is an assumption with a tag on it.

### Clean snapshots are NOT the QA snapshots

`azure-vm-qa-workflow.md` rule 2 snapshots a **warm** machine — Discord installed and signed in,
WebView2 present, an OSL identity already created — because that makes the iteration loop fast.
**That image is disqualifying for this gate**, which requires no OSL identity, no service profile,
no login and no prior updater state.

The release gate therefore needs its own **cold** snapshot lineage, taken *before* any OSL identity
or Discord login exists: Windows + WebView2 runtime only. Name them distinguishably, e.g.
`osl-release-clean-A-<date>`, and record the full Azure resource ID as `goldenSnapshotId`.
Two clean restores from the *same* snapshot are not two VMs; the verifier refuses that.

## Running the gate

Before attaching the attestation, restore both VMs from the **cold** lineage above, with no OSL
identity, service profile, login, or prior updater state. Install the exact draft `.exe` on both
VMs and record its SHA-256.

Operational rules carried from `azure-vm-qa-workflow.md`, each of which has already cost time:

- **Stage `WebView2Loader.dll` beside the exe.** Without it the process hangs before `main` with no
  trace file at all and looks exactly like a corrupt build.
- **Never build on the VM.** These are 2-vCPU boxes. Build on the host and ship the artifact.
- **Identify the build by `sha256`, never mtime** — cargo hardlinks its output, so mtime lies.
- **Drive by file rendezvous over an Azure Files share, not RDP.** `az vm run-command` executes as
  SYSTEM in session 0 where nothing renders, so it cannot verify anything the user would see; it is
  fine for liveness checks and log collection only.
- **Reject any artifact older than run start.** A stale receipt will happily impersonate the current
  run; grade it `unmeasurable`, never pass or fail.
- **Drive the UI for real. On the VM, input injection is allowed and expected.** `SendInput`,
  `mouse_event`, `SetCursorPos` and real keyboard driving are all fine here — nobody is sitting at
  that desktop. The `PostMessage`-only rule is about protecting the *owner's* cursor and applies to
  his desktop only. This matters for this gate specifically: `onboarding`, `identityCreate` and
  `twoAccountLogin` are consent flows that must actually be clicked, and under a PostMessage-only
  reading they were effectively unprovable. Port retired click harnesses to the VM rather than
  rebuilding them.
- **Still forbidden on the VM**, because these bans are about consequence, not focus: no real
  personal account, no real user data, nothing that reaches back to the host, and never a
  destructive action against a target you did not seed yourself. Note the gate's `fullCleanup` case
  is destructive by design — it must run only against identities and conversations this run created.
- **Target the instance by its single-instance marker window class `<identifier>-sic`. Never by
  window title, never by "first process with a window".** Every OSL build is titled `OSL Privacy`.
  Selecting by name has already driven the wrong lane's application through six UI steps and graded
  a stale instance. This is a hard requirement of this gate. An
  automated CI guard was attempted and is **not** in place — a delegated implementation did not
  parse and was rejected — so today this is enforced by review, not by tooling.
- **Assert non-empty on the positive path.** A harness that guesses its subject confirms whatever it
  happened to find, and a default-deny assertion that passes because it read nothing is the same
  defect. Every case below must prove it observed something real before it may report a pass;
  otherwise "correctly denied" and "found nothing" are indistinguishable and the gate grades
  `unmeasurable`, never pass.
- **Credentials come from Key Vault just in time on the VM** — vault `osl-test-secrets-a7d5d9`
  (resource group `osl-two-client-lab`), per `docs/testing/test-account-secrets.md`. They never
  enter a prompt, report, event, screenshot, log or the attestation.

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

## Updater signing — resolved, not a placeholder

Dispatch briefs have repeatedly stated that `tauri.conf.json` carries an empty updater pubkey
placeholder alongside `createUpdaterArtifacts: true`. Checked on `origin/main` @ `38d0867` and
again in the working tree, this is **not the case for either app**. Both keys are populated and
both base64-decode to well-formed minisign public keys:

| File | `createUpdaterArtifacts` | Public key |
|---|---|---|
| `apps/osl-hub/tauri.conf.json:50` (**ships**) | `true` | `minisign public key: 3B6AE4739858E8D4` |
| `src-tauri/tauri.conf.json:39` (legacy) | `true` | `minisign public key: 44AD89E36BC119F8` |

They are different keys, which is correct — the two apps have separate update feeds.

The corresponding private keys are present as **environment** secrets on `hub-release`, verified
by name (values never read): `HUB_TAURI_SIGNING_PRIVATE_KEY` and
`HUB_TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, both set 2026-07-17.

So the signing path is configured end to end. What has **never** been done is running it: there are
zero releases and zero `hub-v*` tags, so no signed artifact has ever been produced and no client has
ever verified one. "Configured" is not "proven"; treat the updater as `designed-only` until a real
candidate is built and a signed update is installed on a clean VM.

## Hardening still owed

1. ~~Verify `goldenSnapshotId` against Azure at promotion time.~~ **Implemented** as
   `scripts/release/verify-snapshot-lineage.sh`. It requires each `goldenSnapshotId` to be a full
   Azure resource ID (a bare label like `snapshot-a` is refused), resolves it with
   `az snapshot show --ids`, and requires the snapshot to pre-date `completedAtUtc` — a snapshot
   created after the run cannot be what the run restored from. Self-test: **6 passed, 0 failed**,
   including an invented bare name, a well-formed ID that does not exist, one-real-one-invented,
   the same snapshot twice, and a snapshot created after the run.

   **Run it before approving the `hub-vm-qa` environment:**

   ```bash
   scripts/release/verify-snapshot-lineage.sh candidate/hub-vm-qa-attestation.json
   ```

   It is deliberately an operator step rather than a CI step: wiring it into promotion would put an
   Azure credential into GitHub Actions, which is an owner decision, not a release-lane one. CI runs
   its `--self-test` on every PR so the logic itself cannot rot.
2. **Bind the attestation to the run, not just the file.** A candidate hash proves *which* binary
   was tested, not *that* it was tested. Recording the VM agent's run id and verdict artifact hashes
   would make the claim checkable after the fact.
3. **Create the cold snapshot lineage** (blocking prerequisite B) before the first promotion.
