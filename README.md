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

## Known limitations (v1 alpha)

Surfaced here so users running closed-beta dogfood builds know what
to expect. Full rationale + v2 mitigation path in
[`docs/design/layer-10-discord-internals.md`](docs/design/layer-10-discord-internals.md)
§14.6.

- **Brief flash of cover string on incoming messages.** The receive
  hook uses a DOM `MutationObserver` to swap `DPC0::<base64>` for
  the decrypted plaintext after Discord renders. Typical flash
  window: tens to a few hundred ms. v2's overlay-window
  architecture eliminates it.
- **DOM-mutation fragility.** Major Discord refactors of the
  message-renderer DOM shape (the `chat-messages___…` list-item
  structure, the avatar URL pattern) can break the receive
  observer. Regressions surface loudly as cover strings staying
  visible.
- **Best-effort sender extraction.** When the rendered DOM
  doesn't expose a parseable Discord user_id (rare — system
  messages, peculiar non-user authors), the cover stays visible
  rather than the client guessing.
- **Sender's own messages flash too.** The sender is auto-included
  as a recipient slot so the sender CAN decrypt their own bounced
  message, but the cover renders briefly first. Optimistic-render
  fix is a separate UX layer (deferred).
- **Receive-side rewriting is detectable from outside.** A
  page-level scan that compares wire-level message content with
  rendered DOM text knows something rewrote it. v1 anti-detection
  (Proxy-wrapped `fetch` + `XMLHttpRequest.prototype.{open,send}`,
  `Function.prototype.toString` spoofing, compile-time DEBUG
  strip) covers the **send** path only.
- **Keyserver privacy linkage.** v1 uses your Discord user_id as
  your OSL identity, so the keyserver sees a graph of "Discord
  users registered to OSL." Acceptable for closed-beta dogfood
  with a known peer set; v2 moves to client-generated UUIDs with
  per-peer Discord-ID → UUID mapping kept locally.
- **No edits, deletes, or attachments.** Phase 6.
- **No PQXDH handshake or Double Ratchet yet.** Phase 7+. Current
  wire format is X25519 ECDH + HKDF-SHA256 + XChaCha20-Poly1305
  per-recipient slots.

## License

AGPL-3.0-only.
