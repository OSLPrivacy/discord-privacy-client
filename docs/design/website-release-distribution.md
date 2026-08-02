# Website release distribution

## Decision

The Windows download advertised by the website is the current **NSIS `.exe`**
from the `hub-latest` GitHub Release.  The release asset, its Minisign
signature, `SHA256SUMS.txt`, and `latest.json` are one distribution set.

`installers.oslprivacy.com` is a mirror only.  It may serve the exact same
bytes, but it is never the authority for a version, checksum, or signature.
The old `osl-privacy-0.0.1.msi` must not be served or linked: it is a different
version and format from the release pipeline's
`osl-hub-0.1.0-x64-nsis.exe`-style asset.

## Why

The updater and release gates produce and verify NSIS assets on GitHub
Releases. Making R2 canonical would split an installer from the signed
release evidence and make Cloudflare load-bearing for a security-sensitive
download. GitHub receives the downloader's IP; OSL does not receive that
event through its own infrastructure.

## Mirror contract

`scripts/publish-installer.mjs` reads the release checksum file rather than
recomputing a second authority. It uploads only the named NSIS asset after the
downloaded bytes match that entry. The site metadata generator likewise reads
`latest.json` and `SHA256SUMS.txt` from the release evidence. A mirror cannot
turn an unavailable or unproven release into an availability claim.

If GitHub Releases becomes unsuitable for the intended audience, revisit this
decision with an independently verified, Tor-friendly mirror. Do not silently
switch the public site back to the stale MSI or make R2 canonical.
