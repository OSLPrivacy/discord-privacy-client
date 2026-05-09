# discord-privacy-client

Privacy-focused Discord client modification for Windows 10+. End-to-end
encryption with post-quantum security, revocable message access, screenshot
resistance, and metadata protection. Companion repo: `discord-privacy-keyserver`.

## Status

**Pre-alpha. No implementation code is in this repository yet.**

This is repository scaffolding. Crypto and injection design docs are under
[`docs/design/`](docs/design/) and must be reviewed before any implementation
begins. A paid third-party cryptographic review is a hard prerequisite for the
v1 ship.

## Threat model

See [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md). Short version: this tool
substantially raises the cost of mass surveillance and casual capture of
Discord content. It does **not** hide that you are communicating, when, or
with whom. Users with targeted-investigation threat models should use Signal,
Briar, or Cwtch.

## Platform

Windows 10+. macOS and Linux are not supported. Onboarding exits on
unsupported OS.

## Discord Terms of Service

Using this tool may violate Discord's Terms of Service and may result in
account suspension. The user assumes that risk. The developer of this tool
should obtain legal review before public distribution.

## Repository layout

```
src-tauri/        Tauri shell (Rust)
crates/
  crypto/         PQXDH hybrid + Double Ratchet + AEAD
  stego/          Text steganography (Mode 1 v1; Modes 2/3 in v2.6)
  transport/      Mullvad WireGuard (v1) + arti Tor (v2.2)
  keystore/       TPM 2.0 seal + Windows Credential Manager fallback
  selectors/      Discord webpack selector resolution
  ipc/            Tauri command surface
webview/          TypeScript injection layer
selector-ci/      Headless WebView2 selector regression check
docs/
  THREAT_MODEL.md
  design/         Per-feature design docs (review gate)
```

## License

AGPL-3.0-only.
