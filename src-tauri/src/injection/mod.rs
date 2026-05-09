//! Tauri WebView injection harness for Layer 10 Discord hooks.
//!
//! `BOOT_SCRIPT` is the JavaScript source that runs as the WebView's
//! `initialization_script` — Tauri 2 executes this **before** the
//! page's own scripts, which is the timing window the Vencord-style
//! source-rewrite approach requires. See `boot.js` for the full
//! design rationale and the verified anchor strings used to identify
//! the chat-input module.
//!
//! The script is `include_str!`'d at compile time so it ships embedded
//! in the binary — no runtime file reads, no extra loose files in the
//! installer payload. Edits to `boot.js` are caught at compile time
//! (the file must exist, must be valid UTF-8) but are otherwise
//! treated as opaque text by Rust.
//!
//! Phase boundaries (see `docs/design/layer-10-discord-internals.md`):
//!   - **Phase 3** (this commit): hook installs and routes outbound
//!     messages through a stub `osl_encrypt_message` IPC command that
//!     returns `"[OSL-STUB] " + plaintext`. End-to-end smoke test of
//!     the injection mechanism.
//!   - **Phase 4** (next): replace the stub with a real call into
//!     `crypto::*` + `stego::*` so outbound messages actually carry
//!     encrypted content. Requires no JS-side changes — the IPC
//!     command shape stays the same.

pub const BOOT_SCRIPT: &str = include_str!("boot.js");
