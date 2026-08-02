# Cutting an OSL Privacy release

This runbook turns one committed version bump into a signed **draft** release,
then promotes only the installer that two clean VMs tested. Do not substitute a
GitHub web-release, a manually uploaded installer, or a hand-written
`hub-latest` manifest: those paths bypass the candidate and update-feed gates.

```json release-runbook
{
  "version_files": [
    "apps/osl-hub/tauri.conf.json",
    "apps/osl-hub/Cargo.toml",
    "apps/osl-hub-ui/package.json"
  ],
  "tag_prefix": "hub-v",
  "tag_must_equal_tauri_version": true,
  "release_starts_as": "draft",
  "promotion_workflow": "Promote VM-tested OSL Privacy candidate",
  "update_feed_release": "hub-latest"
}
```

## 1. Prepare one release commit

Choose a SemVer version without a leading `v`. Change that exact value in all
three files listed above, then add a non-empty `## [<version>] - YYYY-MM-DD`
section to `CHANGELOG.md`. The release workflow uses that section as the
user-visible release notes and refuses a missing or empty section.

Run the ordinary review and release checks for the commit. The hosted candidate
workflow runs the frontend, core-library, Windows identity-lifecycle,
supply-chain, release-note, and public-claim gates; do not create a release
tag until the commit is ready to be built on Windows.

Commit the bump and changelog together. Do not amend those files after tagging.

## 2. Create the tag, then prove it names this exact version

Create an annotated tag from the release commit. Replace only `$version`; do
not type a tag name separately.

```powershell
$version = (Get-Content apps/osl-hub/tauri.conf.json -Raw | ConvertFrom-Json).version
git tag -a "hub-v$version" -m "OSL Privacy $version"
```

Before pushing, run this check while `HEAD` is the tagged commit. It is
deliberately after tag creation: a hand-created `hub-v1.2.4` tag on a `1.2.3`
config must stop here, before it can start a release workflow.

```powershell
python scripts/check_version_consistency.py
$version = (Get-Content apps/osl-hub/tauri.conf.json -Raw | ConvertFrom-Json).version
$tag = git describe --tags --exact-match
if ($tag -cne "hub-v$version") {
  throw "Tag $tag does not match OSL Privacy version tag hub-v$version"
}
python scripts/extract_changelog_section.py --tag $tag
python scripts/check_release_notes_claims.py --tag $tag
```

Push the commit and only that verified tag:

```powershell
git push origin HEAD
git push origin "hub-v$version"
```

## 3. Build the signed draft candidate

The tag starts **Build signed OSL Privacy candidate**. Wait for it to pass. It
must leave `hub-v<version>` as a GitHub *draft* release containing exactly the
signed installer, normalized `latest.json`, checksum and build-hash assets,
and provenance attestation. The workflow does not publish `hub-latest`.

If it fails, fix with a new version-bump commit and new matching tag; never
retag or replace candidate assets under an existing release tag.

## 4. Test the exact draft on two clean VMs

Follow [the candidate VM gate](../testing/hub-release-candidate-vm-gate.md).
Restore two distinct clean Windows snapshots, download the draft installer,
and test the downloaded bytes — not a local rebuild. Record the installer
SHA-256, all required cases, and a second-session final approval in
`hub-vm-qa-attestation.json`.

The `signedUpdate` case may use the documented bootstrap waiver only while no
live feed exists. Upload the completed attestation to the same draft release.
Do not approve the protected environment if the local verifier rejects it.

## 5. Hold for reproducibility, then promote

Wait for the exact-tag Reproducible Build workflow to succeed. It is a manual
release-owner hold today, not a GitHub workflow dependency; a failed or absent
run means do not promote.

Dispatch **Promote VM-tested OSL Privacy candidate** with
`candidate_tag = hub-v<version>`, then approve its `hub-vm-qa` protected
environment only after reviewing the two-VM evidence. The workflow re-downloads
the draft, verifies the exact installer and updater manifest, publishes the
draft, and moves `hub-latest/latest.json`. It is the only authorized promotion
path.

## 6. Verify the live feed and publish checksums

After promotion, fetch the configured public feed and verify that it names the
released version and a downloadable signed installer. Use an unauthenticated
client path; a token-bearing GitHub API request is not evidence an installed
client can update.

```powershell
$feed = "https://github.com/OSLPrivacy/discord-privacy-client/releases/download/hub-latest/latest.json"
Invoke-WebRequest $feed -OutFile latest.json
$manifest = Get-Content latest.json -Raw | ConvertFrom-Json
if ($manifest.version -cne $version) { throw "Live feed version does not match $version" }
Invoke-WebRequest $manifest.platforms.'windows-x86_64'.url -OutFile installer.exe
python scripts/verify_update_feed_acceptance.py --manifest latest.json --installer installer.exe
```

Finally, verify the published `SHA256SUMS.txt` and its minisign signature
against the release assets, then announce the release with the checksum and
build-hash links. The candidate workflow creates and signs those assets; this
step publishes their locations, not a replacement hand-made checksum.

If the live-feed check fails, stop the announcement and investigate the
promotion workflow. Do not claim that the release is available for update.
