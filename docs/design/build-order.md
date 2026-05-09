# Build order for v1 alpha prototype

Layers in dependency order. Stop at CHECKPOINT markers for review.

## Crypto layers
1. Sender keys construction (next)
2. Attachment streaming AEAD
3. Wire-format serialization (length-prefixed framing for EncryptedMessage, Header, InitiatorHandshake)
4. Constant-time review pass

[CHECKPOINT — review crypto crate completion]

## Prototype scaffolding (aggressive cuts — see CHANGELOG)
5. Stego Mode 0 (base64 placeholder, no template fluency)
6. Minimum key server scaffold (single endpoint, sqlite, no auth, plain HTTP)
7. Identity gen + key registration glue
8. Rust ↔ JS bridge for encrypt/decrypt calls

[CHECKPOINT — review scaffolding before Discord integration]

## Discord integration (requires human-in-loop after this point)
9. Tauri shell loading discord.com webview
10. Discord injection hooks (reference Vencord patterns)
11. End-to-end integration: send encrypted message, decrypt on other side

[CHECKPOINT — first working prototype, human verification required]

## Notes
Prototype mode cuts: NO TPM, NO password feature, NO duress flow, NO screenshot resistance, NO prekey infrastructure (live PQXDH only), NO sender-key rotation triggers, NO threshold sharing, NO anonymous credentials, NO manifest signing, NO code signing, NO installer. Plain file storage for keys with loud "INSECURE, dev only" comments. Both users assumed online during testing. All cuts revisited and properly implemented before any expert review.
