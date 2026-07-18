# OSL Privacy bundled UI

This directory is the bundled frontend for the standalone OSL Privacy app. It is
not a hosted web application. The Tauri shell should load its production build
with:

```json
"frontendDist": "../osl-hub-ui/dist"
```

## Local development

```sh
npm install
npm run typecheck
npm test
npm run build
```

`npm run build` produces a relative-path bundle in `dist/` for packaging inside
the desktop application.

Build the native Windows preview with the desktop feature enabled:

```sh
cargo build --release --features desktop --manifest-path apps/osl-hub/Cargo.toml
```

## Current scope

The app opens allowlisted first-party service websites in isolated native
WebView2 profiles. Users enter service credentials only on those services' own
pages. Each remote child view has no Tauri capability and is separate from the
privileged bundled OSL Privacy shell. Its cookies and cache persist locally in
that account's WebView2 profile so login can survive restart; they are not OSL
message-store ciphertext and should be protected with OS full-disk encryption.

The trusted UI may render a draft, decrypted message, attachment preview, or
Scrub excerpt in local process/DOM memory. OSL does not send those plaintext
previews to its servers, analytics, or logs, but local screen capture,
accessibility tools, malware, crash dumps, and cameras remain outside the E2EE
boundary. Windows capture resistance is best effort only.

The trusted composer is a separate local child WebView with a zero-authority
capability. Its draft/IME/UTF-8 foundation is packaged, but it remains hidden
until a service adapter can prove the exact native composer, conversation, and
recipient set. Automatic placement is fail-closed for the same reason. The app
must not describe its current self-loopback crypto fixture as person-to-person
E2EE.

OSL identity, People, recovery, password, retention, burn, updater, and
isolated-profile foundations are native and persistent. Platform history
cleanup, attachments, calls, service-specific notifications, and cross-device
two-person transport are not release-ready yet. Anti-detection and ban-evasion
behavior remain out of scope.
