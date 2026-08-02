# Release-artifact retention

Every published `hub-v*` release remains a verifiable historical record. Its
NSIS installer, Minisign signature, checksum list and signature, updater
manifest, release notes, and provenance/build-verification materials stay
together on GitHub Releases, the canonical host set by
[`distribution-topology.md`](distribution-topology.md). The matching source
tag remains available as well.

```json artifact-retention-policy
{
  "published_release_retention": "indefinite",
  "vulnerable_installer": "keep-downloadable-with-prominent-advisory",
  "ordinary_unpublish": "forbidden",
  "advisory_requirements": [
    "affected versions",
    "impact and practical mitigation",
    "first fixed version",
    "link to the current safe installer"
  ],
  "exceptional_removal": {
    "allowed_for": [
      "confirmed malicious or unauthorized release artifact",
      "binding legal requirement"
    ],
    "required_record": [
      "version and release tag",
      "artifact names and SHA-256 digests",
      "removal time and reason",
      "replacement or incident advisory"
    ],
    "post_removal": "retain the source tag and a public tombstone; do not continue serving the removed installer"
  }
}
```

## Vulnerable releases

A known vulnerability is not a reason to erase an old installer. OSL keeps it
downloadable for reproducible-build verification and for a user who must
reinstall a matching historical version. The release receives a prominent
advisory that names the affected versions, explains the impact and practical
mitigation, identifies the first fixed version, and links to the current safe
installer. The current download path must never select a vulnerable release
when a fixed release exists.

## Un-publishing

Ordinary un-publishing is forbidden. Deleting a release because it is old,
superseded, embarrassing, or vulnerable destroys evidence and makes the
project's verification claims weaker.

Removal is allowed only for a confirmed malicious or unauthorized artifact, or
when a binding legal requirement makes continued distribution impossible. In
either case, stop serving the installer and leave a public tombstone recording
the release tag, artifact names and SHA-256 digests, removal time and reason,
and a replacement or incident advisory. Keep the source tag available unless
the legal requirement also prohibits it. A tombstone is not a substitute for
an advisory on a still-downloadable vulnerable release.
