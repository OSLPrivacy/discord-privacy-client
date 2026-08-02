# OSL Privacy release gates

This is the release-owner inventory. A failed gate means **do not promote** the
candidate. A draft release is not a release, and it must not be treated as
one. The commands and refusal controls below are the current contract, not a
claim that a release has passed them.

`redControl` records an executed refusal test or a failed hosted run. Test
controls deliberately give the gate invalid input and pass only when the gate
rejects it; their `result` is therefore `rejected`, rather than an ambiguous
"green test" label.

```json
[
  {
    "id": "version-and-tag",
    "phase": "candidate",
    "blocks": "Building a signed draft",
    "command": "python scripts/check_version_consistency.py; require tag = hub-v<tauri version>",
    "redControl": "scripts/test_check_version_consistency.py::VersionConsistencyTests.test_rejects_a_ui_only_version_bump",
    "result": "rejected"
  },
  {
    "id": "trusted-frontend",
    "phase": "candidate",
    "blocks": "Building a signed draft",
    "command": "cd apps/osl-hub-ui && npm ci && npm test -- --run && npm run build",
    "redControl": "release workflow step stops before signing when npm test or npm run build exits non-zero",
    "result": "rejected"
  },
  {
    "id": "core-library",
    "phase": "candidate",
    "blocks": "Building a signed draft",
    "command": "cargo test --manifest-path apps/osl-hub/Cargo.toml --features core --lib",
    "redControl": "release workflow step stops before signing when the core test command exits non-zero",
    "result": "rejected"
  },
  {
    "id": "windows-identity-lifecycle",
    "phase": "candidate",
    "blocks": "Building a signed draft",
    "command": "cargo test --manifest-path apps/osl-hub/Cargo.toml --features core --test windows_identity_lifecycle",
    "redControl": "release workflow step stops before signing when the lifecycle test command exits non-zero",
    "result": "rejected"
  },
  {
    "id": "supply-chain-policy",
    "phase": "candidate",
    "blocks": "Signing a candidate",
    "command": "python scripts/audit_hub_release.py && python -m unittest scripts/audit_hub_release.py",
    "redControl": "scripts/audit_hub_release.py::Refuse release supply-chain drift before any candidate is signed. (commit 2872228691db3e3f6605de5683fb42d0c604a20e)",
    "result": "rejected"
  },
  {
    "id": "release-notes",
    "phase": "candidate",
    "blocks": "Signing a candidate",
    "command": "python scripts/extract_changelog_section.py --tag hub-v<version> --github-output <output>",
    "redControl": "scripts/extract_changelog_section.py refuses a tag without a matching changelog section",
    "result": "rejected"
  },
  {
    "id": "public-claims",
    "phase": "candidate",
    "blocks": "Signing a candidate",
    "command": "python scripts/check_release_notes_claims.py --tag hub-v<version>",
    "redControl": "the release-notes claims checker refuses unvetted public claims before signing",
    "result": "rejected"
  },
  {
    "id": "candidate-update-manifest",
    "phase": "candidate",
    "blocks": "A promotable draft",
    "command": "python scripts/normalize_updater_manifest.py … && python scripts/verify_update_feed_acceptance.py --manifest candidate/latest.json --installer candidate/<installer>.exe",
    "redControl": "scripts/test_update_feed_acceptance.py::UpdateFeedAcceptanceTests.test_rejects_a_single_flipped_byte_in_the_artifact",
    "result": "rejected"
  },
  {
    "id": "checksums-hashes-and-provenance",
    "phase": "candidate",
    "blocks": "Completing the candidate workflow",
    "command": "python scripts/build_hash_manifest.py --sign …; python scripts/publish_release_checksums.py --sign …; actions/attest-build-provenance",
    "redControl": "scripts/test_build_hash_manifest.py::BuildHashManifestTests.test_signed_manifest_binds_installer_and_embedded_executable_and_rejects_tampering",
    "result": "rejected"
  },
  {
    "id": "protected-approval-and-two-clean-vms",
    "phase": "promotion",
    "blocks": "Publishing the draft and hub-latest",
    "command": "protected environment hub-vm-qa; python scripts/verify_hub_vm_qa_attestation.py --tag hub-v<version> --candidate-dir candidate --attestation candidate/hub-vm-qa-attestation.json",
    "redControl": "scripts/test_verify_hub_vm_qa_attestation.py::HubVmQaAttestationTests.test_rejects_incomplete_test_matrix (commit 74b466616b6f68f6227c04d4bf5dae03512bd904)",
    "result": "rejected"
  },
  {
    "id": "bootstrap-waiver-expiry",
    "phase": "promotion",
    "blocks": "Promoting a candidate that waives signedUpdate after a feed exists",
    "command": "python scripts/check_bootstrap_waiver_allowed.py --attestation candidate/hub-vm-qa-attestation.json --feed-assets feed-assets.json",
    "redControl": "the promotion workflow fails closed if feed lookup fails, and rejects the waiver once hub-latest has assets",
    "result": "rejected"
  },
  {
    "id": "promoted-client-acceptance",
    "phase": "promotion",
    "blocks": "Publishing the draft and hub-latest",
    "command": "python scripts/verify_update_feed_acceptance.py --manifest candidate/latest.json --installer candidate/<installer>.exe",
    "redControl": "scripts/test_update_feed_acceptance.py::UpdateFeedAcceptanceTests.test_rejects_an_http_platform_url_before_downloading",
    "result": "rejected"
  },
  {
    "id": "reproducible-build-evidence",
    "phase": "post-tag operational hold",
    "blocks": "Manual release-owner sign-off; it is not currently a workflow dependency of promotion",
    "command": "Reproducible Build workflow for the exact hub-v<version> tag",
    "redControl": "hosted run 30658453427 failed for commit 2edf98c96efe8f524d785d0cdd22a5edd3708125 on 2026-07-31",
    "result": "rejected"
  }
]
```

## How the workflows enforce the list

`Build signed OSL Privacy candidate` runs the candidate gates in order. A
non-zero command stops the job, so no later signing step runs. It creates a
draft only; it does not publish `hub-latest`.

`Promote VM-tested OSL Privacy candidate` is separately dispatched and requires
the `hub-vm-qa` protected environment. It verifies the exact draft installer,
the two-VM attestation, any bootstrap-waiver expiry, and installed-client
acceptance before it changes the draft or update feed.

The Reproducible Build workflow was changed by T8-A1 to run for `hub-v*` tags
and on a schedule. It supplies required release evidence, but GitHub does not
currently make its conclusion a dependency of the promotion workflow. The
release owner must therefore hold promotion manually until the exact-tag run is
successful. Treating it as an automated blocker would be inaccurate; making it
one is separate workflow work.

## Re-running a red control

Use the named test or hosted run above rather than inventing a new release
candidate. These controls deliberately reject a malformed version, tampered
artifact, missing VM result, invalid update URL, or invalid workflow order.
They are evidence that a gate is capable of refusing, not evidence that a
release has passed.
