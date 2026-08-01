# Distribution topology

This is the single answer to T11's OQ-2: the canonical Windows installer is
an **NSIS `.exe` on GitHub Releases**. It is the source of truth for the
installer, its Minisign signature, checksums, and the signed updater manifest.

```json distribution-topology
{
  "windows": {
    "canonical_host": "github-releases",
    "canonical_repository": "OSLPrivacy/discord-privacy-client",
    "canonical_release": "hub-latest",
    "installer_format": "nsis-exe",
    "installer_extension": ".exe",
    "download_redirect_target": "https://github.com/OSLPrivacy/discord-privacy-client/releases/download/hub-latest/{installer}",
    "associated_artifacts": [
      "{installer}.sig",
      "SHA256SUMS.txt",
      "latest.json"
    ],
    "self_hosted_copy": "mirror-only",
    "msi": "retire-do-not-serve"
  }
}
```

## Website and release rules

- `/download/windows` redirects to the `download_redirect_target` above. The
  installer name is the NSIS release asset name; do not substitute an MSI.
- `installers.oslprivacy.com`, if retained, is an explicitly labelled mirror,
  not a redirect target or source of truth. It must publish the same installer
  hash as the matching GitHub release. Retire the stale MSI rather than serving
  it.
- The GitHub release asset and its verification artifacts are kept together.
  Splitting the installer from its signature, checksums, or updater metadata is
  not a supported distribution path.

## Why this is the canonical path

The updater and release pipeline are NSIS-based. Moving back to MSI would
require a different updater, reproducibility comparison, and QA verification
path. GitHub Releases also avoids making Cloudflare/R2 load-bearing, as
required by owner decision D11. A download through GitHub still discloses the
download IP to GitHub; it does not send that download event to OSL.

If GitHub becomes unusable over Tor, revisit this decision with a Tor-friendly
`.onion` mirror. Do not make an R2 bucket canonical.
