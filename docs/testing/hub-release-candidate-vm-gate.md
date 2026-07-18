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
