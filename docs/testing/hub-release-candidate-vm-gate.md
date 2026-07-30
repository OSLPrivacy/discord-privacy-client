# OSL Privacy release-candidate VM gate

A signed candidate is **not a release**. The tag workflow creates a draft and
cannot move `hub-latest`. Promotion requires a separate `hub-vm-qa` protected
environment approval and a `hub-vm-qa-attestation.json` asset on that draft.

Before attaching the attestation, restore two distinct Windows VMs from signed
golden snapshots with no OSL identity, service profile, login, or prior updater
state. Install the exact draft `.exe` on both VMs and record its SHA-256.

Both VMs must cover onboarding, identity creation/recovery, two disposable
service-account logins, persistence across restart, signed updating, one-sided
and two-sided encryption, and full cleanup. Pause for the operator at CAPTCHA,
2FA, or provider security checks; never automate around them or put credentials
in the attestation, logs, screenshots, or repository.

## final_owner_gated_signoff_reproduces_package_from_second_session

Final promotion is a two-person/session gate. The release owner may prepare the
candidate and first VM evidence, but the final approver must use a second
login/session to download the draft release assets, reproduce the package hash,
run the verifier against that reproduced candidate directory, and record the
reproduction in the attestation before approving `hub-vm-qa`.

This test refuses promotion unless all of these are true:

- the second session observes the same `candidateTag` and exactly one Windows
  installer in the candidate directory;
- the second session computes the installer SHA-256 and it exactly matches
  `candidateSha256`;
- the second session runs `scripts/verify_hub_vm_qa_attestation.py` against the
  downloaded candidate directory, not a local build tree;
- the attestation names the second reviewer/session and records
  `packageReproducedBySecondSession: true`;
- the release owner and final approver are not the same session.

Any mismatch, missing second-session reviewer, missing downloaded candidate
directory, missing `latest.json`, changed installer bytes, or locally rebuilt
package is a refusal. Do not approve the environment and do not attach a
replacement attestation until the second session reproduces the package from the
draft release assets.

The verifier treats `finalApprover` and
`packageReproducedBySecondSession: true` as release blockers, not notes. A blank
final approver, a reused owner session, or a false reproduction flag means the
draft remains unpromoted.

Use this bounded attestation shape:

```json
{
  "schemaVersion": 1,
  "candidateTag": "hub-v0.1.0",
  "candidateSha256": "<64 lowercase hex characters>",
  "completedAtUtc": "2026-07-17T23:00:00Z",
  "operator": "<reviewer identity>",
  "finalApprover": "<second reviewer/session identity>",
  "packageReproducedBySecondSession": true,
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
