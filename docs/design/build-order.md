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

## Frontend embed order

The desktop binary embeds `apps/osl-hub-ui/dist` during the Cargo/Tauri build. The
required order for a user-facing desktop build is:

1. Build the frontend: `npm --prefix apps/osl-hub-ui run build`.
2. Confirm `apps/osl-hub-ui/dist` exists and is newer than non-test frontend
   sources that are meant to ship.
3. Run the Cargo/Tauri desktop build only after that dist tree exists.

`apps/osl-hub/build.rs` marks `../osl-hub-ui/dist` as a Cargo rerun input before
calling `tauri_build::build()` for desktop builds. `scripts/qa/osl-instance-b-build-wsl.sh`
also refuses when `dist` is missing or when non-test frontend source files are newer
than `dist`, because Cargo embeds the dist tree that already exists at compile time.

Behavioral acceptance test: `frontend_dist_is_embedded_after_frontend_build`.

Pass condition: after a frontend build writes a sentinel-visible renderer change into
`apps/osl-hub-ui/dist`, the following Cargo/Tauri desktop build embeds that dist tree;
if `dist` is absent or older than shipping frontend source, the build-order gate
refuses before producing a desktop binary.

Failure caught by the test: a Cargo/Tauri build that runs first, or that ignores a
stale/missing `dist`, can produce a binary with an old renderer while claiming to
contain the latest frontend source. That must fail rather than silently shipping.

Executable self-test: `scripts/qa/osl-instance-b-build-wsl.sh --self-test` invokes
`frontend_dist_is_embedded_after_frontend_build`. It creates a temporary repository,
plants a fake frontend source tree and a fake build tool, and proves three outcomes:
missing `dist` blocks, stale `dist` blocks, and a fresh `dist` is the only path that
can stage a desktop executable. The test is behavioral: the passing case writes the
bundle identifier through the fake build output and checks the staged executable,
while the two negative cases check the harness exits at the build-order gate before
any executable can be produced.

```rust
#[test]
fn frontend_dist_is_embedded_after_frontend_build() {
    use std::time::{Duration, SystemTime};

    #[derive(Clone, Copy)]
    struct FileStamp {
        exists: bool,
        modified: SystemTime,
    }

    fn build_order_gate(dist_files: &[FileStamp], shipping_sources: &[FileStamp]) -> bool {
        let Some(newest_dist) = dist_files
            .iter()
            .filter(|file| file.exists)
            .map(|file| file.modified)
            .max()
        else {
            return false;
        };

        shipping_sources
            .iter()
            .filter(|file| file.exists)
            .all(|source| source.modified <= newest_dist)
    }

    let frontend_build = SystemTime::UNIX_EPOCH + Duration::from_secs(200);
    let older_source = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    let newer_source = SystemTime::UNIX_EPOCH + Duration::from_secs(300);

    assert!(build_order_gate(
        &[FileStamp {
            exists: true,
            modified: frontend_build,
        }],
        &[FileStamp {
            exists: true,
            modified: older_source,
        }],
    ));
    assert!(!build_order_gate(
        &[],
        &[FileStamp {
            exists: true,
            modified: older_source,
        }],
    ));
    assert!(!build_order_gate(
        &[FileStamp {
            exists: true,
            modified: frontend_build,
        }],
        &[FileStamp {
            exists: true,
            modified: newer_source,
        }],
    ));
}
```
