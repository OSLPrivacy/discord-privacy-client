# Changelog

All notable changes to `discord-privacy-client` are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project follows semver.

## [Unreleased] — v1 alpha foundation

### Added

- `crates/crypto/` primitives:
    - `aead.rs` — XChaCha20-Poly1305 wrapper (RustCrypto
      `chacha20poly1305`), `seal()` / `open()` API, `Key` (32 B,
      `ZeroizeOnDrop`) and `Nonce` (24 B) typed wrappers.
    - `hkdf.rs` — HKDF-SHA256 wrapper (RustCrypto `hkdf` + `sha2`),
      `derive()` and `derive_32()` helpers.
    - `padding.rs` — text-bucket padding (64 / 128 / 256 / 512 / 1024)
      with `u32` BE length prefix; pre-AEAD application so padding is
      authenticated by the AEAD tag.
    - `random.rs` — `OsRng`-backed key/nonce/byte generation.
    - `x25519.rs` — X25519 key exchange (RustCrypto `x25519-dalek` 2.0).
      `SecretKey` / `PublicKey` / `SharedSecret` types (zeroize on
      drop for secret + shared); `generate_keypair`, `derive_public`,
      `diffie_hellman` with manual all-zero (low-order) rejection.
    - `ml_kem_768.rs` — ML-KEM-768 KEM (RustCrypto `ml-kem` 0.2).
      `EncapsulationKey` / `DecapsulationKey` / `Ciphertext` /
      `SharedSecret` types with FIPS 203 byte serialization;
      `generate_keypair`, `encapsulate`, `decapsulate`, plus
      `_with_rng` variants for deterministic / KAT-style testing.
      Includes [`pqxdh_combine_stub`] — domain-separated HKDF-SHA256
      combiner over `(dh_concat || ss_pq)` with the
      `"discord-privacy-client/pqxdh/v1"` info label, as a stub for
      eventual full PQXDH handshake integration.
    - `error.rs` — typed `Error` enum.
    - `lib.rs` — module re-exports.
- Test suites against published vectors:
    - IETF XChaCha20-Poly1305 KAT (CFRG draft Appendix A.3.1).
    - RFC 5869 HKDF-SHA256 test cases 1 and 3.
    - RFC 7748 §6.1 X25519 Alice/Bob known-answer.
    - AEAD round-trip + AD/nonce/key/tag tampering rejection.
    - Padding round-trip across all five buckets, boundary promotion,
      oversized rejection, malformed-length rejection.
    - X25519 round-trip + DH symmetry + identity-point and known
      low-order-point rejection.
    - ML-KEM-768 FIPS 203 size invariants (ek 1184, dk 2400, ct 1088,
      ss 32), encaps/decaps round-trip, determinism with fixed-bytes
      RNG, distinct seeds → distinct keys, byte-serialization round-
      trip (incl. `Ciphertext` round-trip preserving decapsulation),
      implicit-rejection behaviour on wrong-dk decaps, PQXDH combiner
      stub determinism + sensitivity to `dh_concat` and `ss_pq`.
      FIPS 203 / ACVP **published** KAT integration is deferred to a
      future commit; the framework (`_with_rng` variants + `FixedRng`
      helper) is in place.

### Changed

- **Toolchain bumped from 1.78.0 → 1.82.0** (`rust-toolchain.toml`,
  workspace `Cargo.toml` `rust-version`, both GitHub Actions
  workflows). Reason: `ml-kem` 0.2 transitively requires
  `hybrid-array` 0.2 which needs `rustc ≥ 1.81`. 1.82 leaves a small
  headroom while remaining widely supported.
- **AEAD library switched from `dryoc` to RustCrypto
  `chacha20poly1305`** (`crates/crypto/Cargo.toml` adds
  `chacha20poly1305 = "0.10"`; `crates/crypto/src/aead.rs` rewritten).
  Reason: `dryoc` 0.7 does not expose `crypto_aead_xchacha20poly1305_ietf`
  at all. Its only XChaCha20-Poly1305 surfaces are
  `crypto_secretbox_xchacha20poly1305` (no AAD support) and
  `crypto_secretstream_xchacha20poly1305` (auto-generated header,
  no caller-supplied nonce). Neither satisfies the protocol
  requirement of AEAD with **both** caller-supplied nonce and AAD.
  RustCrypto's `chacha20poly1305` implements the same algorithm
  (CFRG XChaCha20-Poly1305 IETF draft), is pure-Rust and reproducible-
  build friendly, and offers a clean caller-supplied-nonce + AAD
  API. **No protocol-level cryptographic change** — the cipher is
  identical, only the implementation library differs.

- **X25519 library switched from `dryoc` to RustCrypto
  `x25519-dalek`** (`crates/crypto/Cargo.toml` adds
  `x25519-dalek = { version = "2.0", features = ["static_secrets"] }`;
  `crates/crypto/src/x25519.rs` rewritten). Reason: `dryoc` 0.7's
  `classic` module does not contain `crypto_scalarmult` (confirmed
  via Windows cargo check). Same situation as the AEAD swap above.
  RustCrypto's `x25519-dalek` 2.0 implements RFC 7748 X25519 with a
  clean `StaticSecret` / `PublicKey` / `SharedSecret` API. **No
  protocol-level cryptographic change** — same algorithm.

  **Behaviour note**: `x25519-dalek` 2.0 returns an all-zero shared
  secret when the peer's public point is low-order, rather than
  erroring (the design called for an explicit reject). The
  contributory-behaviour check is restored in `diffie_hellman` by
  rejecting all-zero output via `subtle::ConstantTimeEq`; documented
  inline in `x25519.rs`.

- **`dryoc` removed from the workspace.** With AEAD on
  `chacha20poly1305` and X25519 on `x25519-dalek`, `dryoc` has no
  remaining consumers in the v1 alpha foundation. The "pure-Rust
  libsodium reimplementation" rationale stands, but `dryoc` 0.7
  turned out to lack both `crypto_aead_xchacha20poly1305_ietf` and
  `crypto_scalarmult` — the two primitives we needed. Removed from
  workspace `Cargo.toml` `workspace.dependencies` and from
  `crates/crypto/Cargo.toml`. Comment block in workspace `Cargo.toml`
  retains the swap history for future readers.

  These deviations from the design doc's library table will be
  reflected in `docs/design/pqxdh-double-ratchet.md` in a separate
  doc-update pass once the implementation phase has stabilised.

- `dryoc` workspace dependency was briefly switched from
  `default-features = false` to default features (initial check,
  before discovering dryoc lacks the IETF AEAD module entirely).
  Moot now that `dryoc` is removed.

- **Test fix in `crates/crypto/tests/x25519_test.rs`**: the
  `dh_rejects_known_low_order_point` test originally used hex
  `"00...0001"` which decodes to the bytes `[0x00, ..., 0x00, 0x01]`
  — the little-endian wire representation of u = 2^248, **not** a
  low-order point. Updated to use `e0eb7a7c3b41b8ae...49b800`, a
  real curve25519 order-8 point from libsodium's known-low-order
  list.

### Deferred to v2.2 (removed from v1 alpha workspace)

- **`boringtun` (Mullvad WireGuard) and `arti-client` (Tor) removed**
  from workspace dependencies and from `crates/transport/Cargo.toml`.
  The `transport` crate retains its scaffolding `lib.rs` with a
  placeholder note pointing here.

  **Reason**: dependency-version conflicts on the curve25519/x25519
  Dalek libraries:

  | Dependency      | Pin                                       |
  | ---             | ---                                        |
  | boringtun 0.6   | `x25519-dalek =2.0.0-rc.3`                 |
  | x25519-dalek 2.0.0-rc.3 | `curve25519-dalek =4.0.0-rc.3`     |
  | arti-client 0.21| `x25519-dalek ^2.0.0` (incompatible with rc) |
  | dryoc 0.7       | `curve25519-dalek ^4.1.3`                  |

  No single set of versions satisfies both boringtun and dryoc, nor
  both boringtun and arti-client, simultaneously. Workspace cargo
  resolution fails before any compilation begins.

  **Plan for v2.2**: pick newer compatible versions of boringtun and
  arti-client, or isolate the WireGuard / Tor stack into a separate
  mini-workspace with a self-contained crypto build (no shared
  curve25519 dependency with the main workspace). The transport
  crate's `lib.rs` placeholder calls this out for future readers.

  **Impact on v1 alpha threat model**: bundled VPN and Tor protections
  are deferred. v1 alpha users are directed to run Mullvad's official
  app (or another VPN) externally. See `docs/THREAT_MODEL.md`
  "Network-layer protection (v1 alpha vs v2.2)".

### Layer 2 — Attachment streaming AEAD

- `crates/crypto/src/attachment.rs`:
    - `wrap_attachment_key(MK_n, content_id, attachment_index)` —
      HKDF-SHA256 wrap per the design doc's "Attachment integration"
      subsection (`salt = MK_n`, `ikm = "attachment-key-wrap"`,
      `info = content_id || u32_be(attachment_index)`).
    - `ATTACHMENT_BUCKETS` = 256 KB / 1 MB / 5 MB / 10 MB / 25 MB,
      `ATTACHMENT_CHUNK_SIZE` = 16 KB; bucket sizes are exact
      multiples of chunk size.
    - `StreamHeader` (versioned, length-prefixed `content_id`,
      bucket / total-chunk / base-nonce-prefix metadata) with a
      `"DPCATT\x01"` magic prefix and structural-invariant checks on
      deserialize.
    - `StreamEncryptor` / `StreamDecryptor` push-style streaming API
      that holds at most one chunk (≤ 16 KB) of plaintext at a time;
      per-chunk nonce = `base_nonce_prefix(20 B) || u32_be(chunk_index)`,
      per-chunk AAD binds the serialized header bytes, the chunk
      index, and an `is_final` flag (so reordering, header tampering,
      and last-chunk truncation all break AEAD).
    - `encrypt_attachment` / `decrypt_attachment` whole-buffer
      conveniences that exercise the streaming path internally.
- 26 attachment tests covering: HKDF wrap determinism + input
  separation, round-trip across all 5 buckets at max capacity, empty
  payload, bucket promotion across boundaries, oversized rejection,
  byte-at-a-time streaming for both encryptor and decryptor, header
  serialization round-trip, wrong-key / tampered-header / tampered-
  ciphertext / swapped-chunks / truncated-tail / dangling-tail
  rejection, and bad magic / version / bucket header rejection.

### Layer 3 — Wire-format serialization

- `crates/crypto/src/wire.rs`:
    - `encode_ratchet_message` / `decode_ratchet_message` for
      pairwise [`ratchet::EncryptedMessage`] (magic `"DPCRDM\x01"`).
    - `encode_sender_keys_message` / `decode_sender_keys_message` for
      group [`sender_keys::EncryptedMessage`] (magic `"DPCSKG\x01"` —
      distinct from the pairwise envelope so receivers cannot
      conflate them).
    - `encode_initiator_handshake` / `decode_initiator_handshake` for
      PQXDH [`pqxdh::InitiatorHandshake`] (magic `"DPCPQX\x01"`),
      including consistency enforcement between the `no_opk` flag
      and `opk_id` presence on both encode and decode.
    - All envelopes carry a 7-byte magic + version-byte prefix and
      length-prefix every variable-length field (`u32` BE);
      receivers reject bad magic, unknown version, truncation,
      oversized declared lengths, and trailing bytes.
- `crates/crypto/src/ratchet.rs`, `sender_keys.rs`:
    - Inner plaintext `Header` types promoted to `pub` with public
      `to_bytes` / `from_bytes` (44 B for ratchet, 16 B for sender
      keys); `HEADER_BYTES` constants exposed. The serialized bytes
      live inside `enc_header` after AEAD-decryption — this commit
      just makes the layout part of the public surface for
      diagnostic / interop tooling.
- 22 wire tests covering: round-trip through encode/decode for all
  three envelope types (with-OPK + no-OPK PQXDH variants), magic /
  version / truncation / trailing-garbage / oversized-length-prefix
  rejection, distinct-magic separation between ratchet and
  sender-keys, ML-KEM ciphertext-length validation, `no_opk` /
  `opk_id` consistency rejection, inner header byte round-trip and
  wrong-length rejection, plus stress with synthetic
  random-byte-payload `EncryptedMessage` and an empty-fields edge
  case to confirm the codec is a perfect inverse independent of
  protocol-layer validation.

### Layer 4 — Constant-time review pass

Audit of all `crates/crypto/src/*.rs` for non-constant-time
comparisons of secret-derived data. Findings:

- **No code changes needed.** The crate already routes every
  secret-dependent equality through a constant-time primitive:
    - AEAD tag verification flows through RustCrypto
      `chacha20poly1305`, which uses `subtle::CtOption` /
      `ConstantTimeEq` internally.
    - ML-KEM-768 decapsulation uses *implicit rejection* per
      FIPS 203 §6.3 — wrong-key / tampered-ciphertext inputs return
      a deterministic non-secret-revealing 32 B value.
    - `x25519::diffie_hellman` rejects all-zero shared secrets via
      `subtle::ConstantTimeEq::ct_eq` (small-subgroup contributory
      behaviour, since `x25519-dalek` 2.0 does not error on
      low-order peer points).
- **No secret-typed struct derives `PartialEq`/`Eq`** — verified by
  grep across the crate. Public-data types
  ([`aead::Nonce`], [`x25519::PublicKey`],
  [`ratchet::Header`], [`sender_keys::Header`],
  [`attachment::StreamHeader`]) do, but their contents are
  transmitted in the clear and admit no CT-relevant attack.
- **Skipped-message-key cache** in `ratchet` / `sender_keys` uses a
  linear early-exit AEAD-trial scan; matched-slot index leaks
  through iteration count. This matches Signal's reference
  implementation; the cap (1000) + 30-day TTL bound the leak.
  Always-scan (1000×) constant-time variant rejected as a
  perf regression for a leak the design accepts.
- Findings recorded inline in `crates/crypto/src/lib.rs`'s
  module-level docs so the reviewing cryptographer (paid engagement,
  v1 stable prerequisite) sees them in one place.

[CHECKPOINT — crypto crate complete; awaiting review before
proceeding to Layer 5 (stego Mode 0).]

### Layer 5 — Stego Mode 0 (base64 placeholder)

- `crates/stego/src/lib.rs`, `mode0.rs`:
    - `encode_mode0` / `decode_mode0` / `is_mode0` — base64-standard
      body with a verbatim `DPC0::` magic prefix.
    - `MODE0_MAX_RAW_LEN = 1400` raw bytes (≈ 1874 chars on the
      wire after base64 + prefix), comfortably under Discord's
      2000-char per-message limit.
    - Per-message-independence requirement documented in module
      docs and locked in by a test that decodes payloads in reverse
      order to confirm no inter-message state.
- 11 Mode 0 tests: empty / arbitrary-byte / max-length round-trip,
  oversized rejection, prefix detection (case-sensitive, no
  whitespace tolerance), invalid-base64 rejection, Discord-safe
  charset verification, 2000-char-limit fit, per-message
  independence, encoding determinism.
- Stego dep added: `base64 = "0.22"`.
- Mode 0 is a **placeholder**, not fluent stego: a human reading
  the channel will see "DPC0::<base64>" and immediately recognise
  an encrypted message. Acceptable for prototype mode (dev-to-dev
  testing in private channels); v1 stable replaces with Mode 1
  template-based fluency per the design doc.

### Layer 6 — Key server scaffold

- New sibling `keyserver/` directory: Node 22+, Fastify 4,
  better-sqlite3 11. Plain HTTP, no auth, sqlite. **INSECURE BY
  DESIGN** — prototype scaffold, dev-to-dev only.
- Endpoints:
    - `GET /v1/healthz` — liveness.
    - `POST /v1/register` — upsert identity-key record (initial =
      201, re-registration = 200 with `key_rotation_recorded`).
    - `GET /v1/pubkeys/:user_id` — look up identity keys; 404 on
      unknown user; surfaces `last_rotated_at` for client-side
      verification UI.
    - `POST /v1/wrapped-keys` — upload one wrapped-share blob.
      Validates shape (allow-listed `content_type`,
      `system_message_kind` allow-list with `burn-alert`,
      `single_use` ↔ `display_duration_seconds` consistency,
      base64 + ISO-8601). 409 on duplicate `content_id`.
    - `GET /v1/wrapped-keys/:content_id` — fetch + (single_use)
      consume. Atomic transaction: single_use rows are deleted in
      the same statement that returns them. 404 unknown / burned;
      410 Gone on past `expires_at` (lazy-tombstoned on read).
- 21 tests pass via `npm test` (Node built-in test runner).
- Deferred (per design doc, not in v1 alpha scope): Discord OAuth
  registration gate, rate limiting, prekey-bundle / replenish, burn
  endpoint, session rotation, anonymous-credential token issuance,
  TLS, threshold-share fan-out across 5 jurisdictions.

### Layer 7 — Identity gen + key registration glue

- `crates/keystore/`:
    - `identity.rs`: `Identity` (X25519 + ML-KEM-768 keypair +
      `user_id`); `generate_identity` uses `OsRng`; ML-KEM
      decapsulation key stored as `Zeroizing<[u8; 2400]>` because
      RustCrypto `ml-kem` 0.2's `DecapsulationKey` is not `Clone`.
      Reconstructed on demand via `mlkem_decapsulation_key()`.
    - `storage.rs`: plain-JSON file with base64-encoded key bytes,
      versioned blob (`IDENTITY_BLOB_VERSION = 1`), explicit
      `insecure_banner` field on disk. Loader rejects version
      mismatches and any field that decodes to the wrong byte length.
    - `client.rs`: minimal hand-rolled HTTP/1.1 client over
      `std::net::TcpStream`. Plain HTTP only (rejects `https://`).
      Endpoints: `register` (POST /v1/register) and `fetch_pubkeys`
      (GET /v1/pubkeys/:user_id, with `:user_id` percent-encoded).
      Sync; Layer 8 will spawn it via `tokio::task::spawn_blocking`.
- Dep churn: tempfile pinned to `=3.13` because 3.27 transitively
  pulls in `getrandom 0.4.2` which requires edition-2024 / rustc
  1.85+ (workspace MSRV is 1.82). Same reason no ureq / reqwest /
  attohttpc — hand-rolled HTTP client avoids the deps-tree drift.
- Old `keyring` and `windows` crate deps removed from
  `crates/keystore/Cargo.toml` for prototype mode; reinstated when
  TPM-sealed identity blobs land in v1 stable.
- 16 keystore tests pass:
    - 3 identity tests (generation, distinctness, byte-round-trip).
    - 6 storage tests (round-trip, parent-dir creation, overwrite,
      version-mismatch rejection, short-field rejection, INSECURE
      banner present on disk).
    - 7 client tests (request shape, https rejection, URL parsing
      across forms, in-process mock-server round-trips for
      register + fetch_pubkeys, percent-encoding of `user_id`,
      HTTP error propagation).
- Standing INSECURE notes documented in module-level docs and the
  on-disk `insecure_banner` field. v1 stable replaces with TPM
  seal + Argon2id passphrase + Discord OAuth + TLS.

### Layer 8 — Rust ↔ JS bridge

- `crates/ipc/`:
    - `commands.rs`: pure functions for the bridge surface —
      `cmd_generate_identity`, `cmd_load_identity`, `cmd_save_identity`,
      `cmd_init_keyserver`, `cmd_register`, `cmd_fetch_pubkeys`,
      `cmd_aead_seal`, `cmd_aead_open`, `cmd_stego_encode`,
      `cmd_stego_decode`, `cmd_status`, `cmd_x25519_diffie_hellman`.
      All testable without a Tauri runtime.
    - `state.rs`: `AppState` holds `Mutex<Option<Identity>>` and
      `Mutex<Option<KeyServerClient>>` — installed once via
      Tauri's `manage` slot.
    - `lib.rs`: `IpcError` is `Serialize` (tagged enum:
      `{ kind, message }`) so JS sees a stable shape; `From` impls
      collapse crypto / stego / keystore / base64 errors into the
      IPC variants.
    - **No Tauri dep in this crate.** Tauri's Wry runtime pulls in
      gtk/webkit2gtk on Linux which would prevent `cargo check
      -p ipc` from running on dev environments without the system
      libs. The `#[tauri::command]` wrappers therefore live in
      `src-tauri/src/main.rs` instead.
- `src-tauri/src/main.rs`: 12 `#[tauri::command]` wrappers around
  the `ipc::cmd_*` pure functions; `tauri::Builder::default()
  .manage(AppState::new()) .invoke_handler(generate_handler![…])
  .run(generate_context!())`. Sync HTTP work is driven through
  `tauri::async_runtime::spawn_blocking` so the tokio runtime that
  hosts Tauri commands never blocks on the keystore HTTP client.
- 17 ipc tests pass (state seeding, save/load round-trip,
  init-keyserver URL parsing + https rejection, register / fetch
  prerequisite checks, AEAD seal/open round-trip + tampering
  rejection + wrong-key-length rejection, stego round-trip + non-
  Mode-0 rejection, status reflecting state, X25519 DH round-trip,
  IPC error JSON shape).
- Toolchain bump: rustc **1.82 → 1.88** because tauri 2.11 + its
  transitive deps (uuid 1.23, indexmap 2.14, time 0.3.47,
  serde_with macros 3.19, wasip3 0.4) require edition-2024 / 1.88.
  rust-toolchain.toml + workspace `rust-version` updated. All
  prior layers re-tested green on 1.88.
- Build state: `cargo check -p src-tauri` requires gtk-3 +
  webkit2gtk-4.1 + libsoup-3.0 system packages on Linux (or a
  Windows / macOS host). Verification of the Tauri attribute glue
  is deferred to the Windows host the user already verifies on; the
  IPC pure-function surface (which is the part with logic) is
  fully exercised by `cargo test -p ipc`.

[CHECKPOINT — scaffolding complete; awaiting review before
proceeding to Layer 9 (Tauri shell loading discord.com webview).]

### Layer A1 — Sender-key rotation triggers

- New `crates/runtime/` workspace member.
    - `clock.rs`: `Clock` trait + `SystemClock` (production) +
      `MockClock` (test-only, advance-by-Duration).
    - `rotation.rs`: `RotationController` per-(sender, group) state
      machine with `RotationConfig` (defaults from
      `docs/design/sender-keys.md`: time 1h, message-count 500,
      idle 6h, suspicious-cap 5min). `RotationReason` /
      `SuspiciousEventKind` public enums.
    - Caller-driven API: `note_message_sent`,
      `note_rotation_completed`, `note_suspicious_event`,
      `note_membership_change`, `note_recipient_request` +
      `check_for_rotation` (returns `Option<RotationReason>`).
    - Precedence per design doc: forced (membership / recipient) →
      time → message-count → suspicious (with idle synthesis +
      5-min DoS cap; cap exempts time / count / membership /
      recipient).
    - Idle is synthesised in `check_for_rotation` when
      `idle_trigger` has elapsed since the last `note_message_sent`
      and at least one message was sent on the chain. Emits at
      most once per chain (re-arms on each `note_message_sent`).
- 20 runtime tests pass:
    - Time / message-count / idle each fire at configured threshold.
    - Suspicious cap bounds rotations at 1 per 5 min; queued
      events collapse to a single rotation.
    - Time + message-count + membership are cap-exempt: fire even
      while the suspicious cap window is active.
    - Forced rotations (membership / recipient) take precedence
      over all timer-driven triggers.
    - End-to-end: controller drives a real
      `crypto::sender_keys::SenderChain::rotate` after 500
      messages.
    - `RotationReason::is_cap_exempt` classification matches the
      design doc.
- Note: with the default config (time 1h, idle 6h), idle is
  structurally redundant — the time trigger fires before idle ever
  could. Tests use `time_trigger=100h` to isolate idle's
  observable behaviour. v1 stable may invert the relationship if
  cryptographer review prefers idle-first precedence.

### Layer A2 — Screenshot resistance

- `crates/runtime/src/screenshot.rs`: cross-platform interface
  `apply_to_hwnd(hwnd_isize, ScreenshotProtection::{On, Off})`.
    - `cfg(windows)` impl: `SetWindowDisplayAffinity` with
      `WDA_EXCLUDEFROMCAPTURE` (or `WDA_NONE`).
    - `cfg(not(windows))` stub: always `Ok(())`. Lets the rest of
      the binary compile on Linux / macOS dev hosts.
- `src-tauri/src/screenshot.rs`: thin wrapper that pulls the HWND
  out of `tauri::WebviewWindow` and delegates to
  `runtime::apply_to_hwnd`. Tauri-window unwrap kept out of the
  cross-platform crate so `runtime`'s tests still build on Linux.
- `src-tauri/src/main.rs`:
    - On `app.setup`, fetches the main webview window and applies
      `ScreenshotProtection::On`. Logs a tracing warning if it
      fails (non-Windows targets always succeed via the stub).
    - New `set_screenshot_protection(enabled)` Tauri command lets
      the webview JS toggle protection at runtime — e.g. relaxing
      it for a deliberately-public conversation.
- `windows = "0.56"` dep added to both `crates/runtime/Cargo.toml`
  and `src-tauri/Cargo.toml`, gated on `cfg(windows)`, with the
  `Win32_Foundation` + `Win32_UI_WindowsAndMessaging` features
  needed for `HWND` + `SetWindowDisplayAffinity` /
  `WDA_EXCLUDEFROMCAPTURE` / `WDA_NONE`.
- 3 runtime screenshot tests on Linux exercise the no-op stub
  (both states, arbitrary HWND values) plus enum invariants
  (`Copy`, `Eq`, `Debug`). Per the standing instructions and the
  design doc, the actual capture-blocking on Windows is OS-level
  and verified visually by the user — no automated test possible.
- Caveats documented inline in `runtime/src/screenshot.rs`:
  WDA_EXCLUDEFROMCAPTURE blocks OS-level capture only; cameras,
  kernel-mode capture drivers, and HDMI capture cards downstream
  of the GPU still work. Threat model already labels this a
  deterrent rather than a guarantee.
- Build state: `cargo check -p discord-privacy-client` still
  fails on Linux because of unrelated gtk/webkit2gtk system-deps
  (gio-sys / gobject-sys / glib-sys build scripts). Verification
  of the src-tauri integration is deferred to the user's Windows
  host as documented for Layer 8.

### Layer A3 — USB device monitoring

- `crates/runtime/src/usb.rs`:
    - `UsbDeviceDescriptor` (base_class, video_streaming_present,
      input_terminal_types) and pure `is_capture_device` filter
      per design doc table: USB-IF base class `0x0E`, subclass
      `0x02` (`SC_VIDEOSTREAMING`), with at least one Input
      Terminal in `0x0200..=0x02FF` (camera / media transport) or
      External Terminal in `0x0400..=0x04FF` (composite / S-video
      / component connectors).
    - `UsbMonitor::start(callback)` with platform-specific `imp`:
        - **Windows**: hidden message-only window on a dedicated
          `dpc-usb-monitor` thread. Registers
          `RegisterDeviceNotificationW` against
          `KSCATEGORY_CAPTURE`
          (`{65E8773D-8F56-11D0-A3B9-00A0C9223196}`); WndProc
          handles `WM_DEVICECHANGE` / `DBT_DEVICEARRIVAL` and
          forwards to the callback. RAII cleanup on drop:
          `PostThreadMessageW(WM_QUIT)`,
          `UnregisterDeviceNotification`, `DestroyWindow`,
          `UnregisterClassW`. **The Win32 monitor compiles only
          under `cfg(windows)` and is unverified on the WSL
          test host** — same caveat as A2's Win32 path; needs
          Windows-host verification.
        - **Non-Windows**: stub that holds the callback and never
          fires it. `cargo test -p runtime` exercises this path.
    - `windows = "0.56"` features extended with
      `Win32_System_LibraryLoader`,
      `Win32_System_Diagnostics_ToolHelp` (the latter for A4).
- 21 USB tests pass:
    - 5 capture-class positive cases (camera, media transport,
      HDMI capture composite, S-video / component, multiple
      terminals with one input).
    - 7 non-capture USB-class negatives (HID, mass storage, audio,
      hub, smart card, comms, printer).
    - 5 video-class-but-not-capture negatives (no streaming, only
      output terminals, no terminals, vendor input outside range,
      just-outside-external-range).
    - 1 boundary test for terminal-range edges
      (`0x0200`/`0x02FF`/`0x0400`/`0x04FF`).
    - 2 monitor lifecycle tests (construction succeeds on all
      targets; non-Windows stub never fires callback).
    - 1 integration test demonstrating the
      `callback → RotationController::note_suspicious_event(UsbCaptureDevice)`
      wiring shape.
- src-tauri integration of the monitor is **deferred to A4**, where
  all four Group-A trigger sources will wire into a shared
  `RotationController` registry at app startup.

### Layer A4 — Process scanning for screen recorders

- `crates/runtime/src/recorder.rs`:
    - `RECORDER_PROCESS_NAMES`: 35 lowercase basenames covering
      OBS / OBS legacy, Bandicam, Camtasia + helpers, ShareX,
      NVIDIA ShadowPlay / Share / Broadcast, Windows Game Bar +
      helpers, Snipping Tool / Snip & Sketch, Snagit, XSplit,
      Loom, vokoscreenNG, Screenpresso, Movavi, Mirillis Action!,
      Fraps, Dxtory, Lightshot, Greenshot, FlashBack Express,
      Ezvid, FastCap, Icecream Screen Recorder. Provenance comments
      next to each entry.
    - `match_recorders<S: AsRef<str>>(&[S]) -> Vec<&'static str>`:
      pure case-insensitive match returning entries in
      match-list order.
    - `snapshot_running_processes()`: Win32
      `CreateToolhelp32Snapshot` + `Process32FirstW/NextW`
      enumeration on Windows; non-Windows stub returns
      `Err(RecorderScanError::Win32("unsupported"))` so callers
      degrade gracefully.
    - `RecorderScanner::start(config, on_detect)`: dedicated
      `dpc-recorder-scan` thread that periodically calls `scan()`
      every `config.interval` (default 1 h) and fires
      `on_detect(&matches)` only when at least one recorder is
      found. RAII teardown via shared atomic stop-flag + bounded
      tick sleeps. Errors logged via `tracing::debug!`; the loop
      continues so transient enumeration failures don't disable
      the trigger.
- `windows = "0.56"` features extended with
  `Win32_System_Diagnostics_ToolHelp` (already added in A3).
- 15 recorder tests pass:
    - 7 match-logic tests: positive / multi / case-insensitive /
      innocuous-process empty / empty-input / full-path strings
      not stripped / dedup-by-list-order.
    - 4 list-invariant tests: lowercase, unique, .exe-suffixed,
      design-doc examples present.
    - 1 snapshot-platform test: returns Win32 error on non-Windows.
    - 2 scanner-lifecycle tests: starts + stops cleanly,
      `Drop` terminates thread within tick budget (asserted
      `< 500 ms`).
    - 1 integration test: detected recorders drive
      `RotationController::note_suspicious_event(ScreenRecorder)`,
      controller emits the matching `RotationReason`.
- src-tauri integration: deliberately not yet wired into
  `src-tauri/src/main.rs`. The unified Group-A startup hookup
  (UsbMonitor + RecorderScanner + idle-tick driver feeding a
  `RotationController` registry held in Tauri State) is a small,
  bounded follow-up — but it can't be cargo-checked on the WSL
  host (no GTK system libs), so leaving it for the user's
  Windows-host integration pass keeps the verification surface
  honest. Both monitors expose `*::start(callback)` constructors
  ready to be glued into one `app.setup`-time block.

[CHECKPOINT A — Group A complete (rotation triggers, screenshot
resistance, USB monitoring, recorder scanning). Awaiting review
before proceeding to Group B (TPM-sealed keys, password / duress,
prekey infrastructure).]

### Layer B1 — TPM-sealed identity keys

- New `crates/keystore/src/sealer.rs` with the `Sealer` trait
  (`method_label`, `is_tpm_backed`, `requires_insecure_banner`,
  `seal`, `unseal`) and four implementations:
    - **`TpmSealer`** (Windows-only, `cfg(windows)`): NCrypt with
      `Microsoft Platform Crypto Provider`. Persistent RSA-2048
      key (`DiscordPrivacyClientIdentityKeyV1`) created on first
      use; per-seal hybrid wrap — fresh XChaCha20-Poly1305 data
      key, RSA-PKCS#1 wraps the data key via the TPM-resident RSA
      key, AEAD encrypts the payload. Wire format:
      `u32 BE wrapped_len || wrapped || nonce || ciphertext+tag`.
      `evict_tpm_key()` for the duress / B3 strip path. Win32 path
      is unverified on WSL — same caveat as Group A's Win32 paths.
    - **`KeyringSealer`**: keyring crate (Windows Credential
      Manager / macOS Keychain / Linux Secret Service). 32-byte
      XChaCha20-Poly1305 key persisted as base64 under
      `discord-privacy-client / identity-data-key.v1`. Self-probe
      on `new()` reads the stored key back and rejects if the
      backend is non-persistent (e.g. WSL with a transient DBus
      session).
    - **`NoOpSealer`**: passthrough for the absolute-last-resort
      fallback. `requires_insecure_banner == true` so storage
      writes the loud `INSECURE prototype storage` banner field.
    - **`MemorySealer`**: test-only, in-process random key. Used
      by Linux test paths to exercise the sealed code path without
      requiring TPM / DBus.
- `select_best_sealer()` factory: TPM → keyring → NoOp.
- `crates/keystore/src/storage.rs` rewrite: two-layer JSON
  (`{ version, method, sealed_b64, insecure_banner? }` outer,
  `{ user_id, x25519_*, mlkem_* }` inner). On-disk version bumped
  `1 → 2`; v1 blobs are explicitly rejected with
  `Error::BlobVersionMismatch`. New
  `Error::BlobMethodMismatch { got, expected }` distinguishes
  cross-sealer load attempts.
- `crates/keystore/Cargo.toml`:
    - `keyring = { workspace = true }` reinstated.
    - Windows-gated `windows = "0.56"` extended with
      `Win32_Security_Cryptography` for NCrypt / BCrypt.
- `crates/ipc/src/commands.rs`: `cmd_save_identity` /
  `cmd_load_identity` now call `select_best_sealer()` and thread
  the sealer through.
- 22 keystore tests pass:
    - 10 sealer unit tests: NoOp / Memory round-trip, fresh-nonce
      uniqueness, truncation + tamper rejection, cross-sealer
      reject, empty plaintext, factory returns a working sealer.
    - 12 storage tests: round-trip per sealer, version mismatch,
      method mismatch, short inner field rejection, banner
      present-for-NoOp / absent-for-Memory, opaque-on-disk for
      Memory vs visible-for-NoOp, parent-dir creation,
      overwrite, marker-trait `Send + Sync` for `dyn Sealer`.
    - 32 total in keystore (10 sealer + 12 storage + 3 identity +
      7 client).
- The TPM path on Windows verifies via the user's Windows host
  (same protocol as Group A Win32 paths). Linux WSL runs:
  `select_best_sealer()` falls through to NoOp on this host
  (keyring's secret-service backend errors against WSL's transient
  DBus or the self-probe rejects); production Windows hosts will
  pick TPM.

### Layer B2 — Argon2id password feature

- New `crates/keystore/src/password.rs`:
    - `Argon2Params` with `production()` (m=64 MiB / t=3 / p=1 /
      out=32) and `fast_for_tests()` (m=8 KiB / t=1) — production
      meets the design doc's GPU-resistance floor.
    - `PasswordHash::create(plaintext, params)` (random 16-byte
      salt) and `PasswordHash::verify` (constant-time via
      `subtle::ConstantTimeEq`).
    - `validate_password` (>= 6 chars per design doc) and
      `validate_setup_pair(unlock, duress)` (rejects identical
      passwords).
    - `PasswordRecord` (unlock_hash + optional duress_hash +
      `failed_attempts` + threshold + inactivity_seconds);
      defaults match design doc (10-attempt threshold, 900s /
      15min inactivity).
    - `verify_against_record` returns `VerifyOutcome` —
      `Unlock` / `Duress` / `Wrong { attempts }` /
      `DuressByThreshold`. Always checks both hashes (no
      time-distinguishable difference between unlock and duress
      paths). Successful unlock resets the counter; duress paths
      do not (irreversible).
    - `InactivityTimer` (in-memory, OS-idle source per design
      doc): `mark_activity_at` / `should_reprompt_at` for
      deterministic testing.
    - `save_password_record` / `load_password_record` go through
      the active `Sealer` — TPM-sealed when available, INSECURE
      banner emitted otherwise. Mirrors identity on-disk shape:
      version + method tag + sealed inner JSON. Method-mismatch
      and version-mismatch rejected with the same `Error`
      variants as identity load.
- `keystore::Argon2Params`, `PasswordHash`, `PasswordRecord`,
  `VerifyOutcome`, `InactivityTimer`, etc. re-exported at crate
  root.
- `argon2 = "0.5"` and `subtle = { workspace = true }` added to
  `crates/keystore/Cargo.toml`.
- 29 password tests pass:
    - 7 policy tests (length floor, six-digit accept, alphanumeric
      accept, identical-pair reject, unlock-only allowed, design
      constants).
    - 6 hash tests (round-trip, wrong / empty rejection,
      random-salt-per-create, production params meet floor, test
      params distinct from production).
    - 8 record / verify tests (unlock match, duress match, wrong
      increments, success resets, threshold triggers
      `DuressByThreshold`, defaults match design, identical-pair
      rejected at construction, short unlock rejected at
      construction).
    - 3 inactivity-timer tests (does not fire within window, fires
      at threshold, resets on activity).
    - 5 persistence tests (round-trip per sealer, banner present
      for NoOp, method + version mismatch rejection,
      failed-attempt count survives save/load).

### Layer B3 — Duress flow execution

- New `crates/keystore/src/duress.rs`:
    - `WipeStep` enum (12 variants) covers every Phase-2 list item
      in `docs/design/unlock-and-duress.md` plus Phase-3 strip
      OPSEC; `WipeStep::ordered()` is the canonical execution
      order. Step 4 (anonymous credentials, v2.3+) and steps 5–8
      (prekeys, ratchet, sender keys, peer ratchets) are explicit
      enum variants — **not** `unimplemented!()` / `todo!()` —
      and run as `Skipped { reason }` when no handler is wired.
    - `StepOutcome::{Wiped, AlreadyClean, Skipped { reason },
      Failed { error }}` reports what each step actually did. A
      failing step does NOT abort the run; the engine keeps going
      and surfaces the failure in `DuressReport::failed_steps()`.
    - `DuressEngine::execute` is the synchronous Phase 2 + 3
      driver. `resume_if_pending()` reads the on-disk journal and
      re-runs uncompleted steps so a crash mid-strip resumes
      cleanly on relaunch.
    - On-disk JSON journal at a caller-supplied path. Each step
      writes its outcome AFTER it runs so even a panic between
      step N and N+1 leaves N recorded. Successful clean runs
      delete the journal; runs with any `Failed` step retain it
      so future relaunches retry.
    - Handlers wired in v1 alpha (no callback needed):
        - `TpmEvict` — `evict_tpm_key()` (no-op on non-Windows /
          if no key was created).
        - `KeyringPurge` — `KeyringSealer::purge_keyring_entry()`.
        - `IdentityFile` / `PasswordHashes` — idempotent file
          deletion. Missing file → `AlreadyClean`, not failure.
    - Handlers reserved for future layers via `DuressHandlers`:
        - `wipe_local_cache_dir`, `wipe_anonymous_credentials`
          (v2.3+), `wipe_prekeys` (B4), `wipe_double_ratchet`,
          `wipe_sender_keys`, `wipe_peer_ratchets`,
          `zeroize_in_memory`, `strip_opsec_files`. Each `None`
          becomes a documented `Skipped` step at run time.
- 11 duress tests pass:
    - Walks every step in canonical order with no handlers wired
      (deferred steps land as `Skipped` with non-empty reasons
      that don't look like silent stubs).
    - File-deletion idempotency: missing files yield
      `AlreadyClean`; running twice doesn't break.
    - Handlers run in declared order (recorded via shared `Vec`).
    - A failing handler records `Failed { error }` and the engine
      continues to subsequent steps.
    - `resume_if_pending` returns `None` when no journal exists.
    - Resuming from a hand-crafted partial journal skips the
      already-completed steps (verified by leaving the identity
      file on disk after the journal pre-records its deletion).
    - Successful run removes the journal; failing run retains it.
    - `DuressReport::skipped_steps()` / `failed_steps()` classify
      correctly.
    - `WipeStep::ordered()` covers every enum variant.

### Layer B4 — Prekey infrastructure

- `crates/crypto/src/ed25519.rs` (new): RFC 8032 Ed25519 sign /
  verify wrapping `ed25519-dalek` 2.x. **Deviation from design
  doc**: design specifies `IK_X25519`-signed SPKs (xeddsa); v1
  alpha adds a separate `IK_Ed25519` identity-signing key
  alongside `IK_X25519`. v1 stable migrates to xeddsa per the
  design. 9 ed25519 tests pass including RFC 8032 §7.1 KAT vector
  1.
- `crates/keystore/src/identity.rs`:
    - `Identity` gains `ed25519_secret` / `ed25519_public`.
    - `IDENTITY_BLOB_VERSION` bumped 2 → 3 (inner blob shape).
    - `from_bytes` extended; `generate_identity` generates both
      keypairs.
- `crates/keystore/src/storage.rs`: inner JSON gains
  `ed25519_secret_b64` / `ed25519_public_b64` fields.
- `crates/keystore/src/client.rs`:
    - `RegisterRequest` + `PubkeysResponse` carry
      `ik_ed25519_pub`.
    - New `fetch_prekey_bundle(user_id)` (GET).
    - New `replenish_prekeys(identity, spk?, opks)` (POST). Signs
      the canonical batch bytes with `identity.ed25519_secret` and
      ships them with the SPK section + OPK list.
    - New `replenish_using_state(identity, &mut state,
      server_remaining, now)` convenience: rotates SPK if due,
      generates fresh OPKs to top up to target, signs, uploads,
      mutates the local state.
- `crates/keystore/src/prekeys.rs` (new):
    - `PrekeyConfig { opk_pool_target = 100,
      opk_replenish_threshold = 25, spk_rotation_seconds = 7 days }`
      matches design doc defaults.
    - `PrekeyState`: current SPK + previous SPK (retained one
      rotation period) + OPK pool with monotonic ids.
    - `PrekeyState::should_rotate_spk` / `rotate_spk` / `should_replenish`
      / `replenish_count_to_target` / `add_opk_batch` / `consume_opk`.
    - `canonical_replenish_bytes(user_id, spk?, opks)` — exact
      byte-level mirror of the Node server's
      `canonicalReplenishBytes`. Domain-separation label
      `discord-privacy-client/prekey-replenish/v1` + length-prefixed
      string fields. `sign_replenish_batch` Ed25519-signs the bytes
      with the identity key.
    - `iso_8601_from_unix_seconds` for SPK rotation timestamps
      (matches Node `Date(t).toISOString()` for whole seconds:
      `YYYY-MM-DDTHH:MM:SS.000Z`).
    - `save_prekey_state` / `load_prekey_state` — sealed via the
      active `Sealer`; same INSECURE-banner-conditional pattern as
      identity / password storage.
- `keyserver/`:
    - `src/db.js`: `users` table gains `ik_ed25519_pub` column.
      New `prekey_bundles` (per-user current + previous SPK) and
      `opk_pool` (per-user OPK pool with `(user_id, opk_id)` PK)
      tables. New `upsertPrekeyBundle` / `popPrekeyBundle`
      transactional helpers; `popPrekeyBundle` does the atomic OPK
      single-row delete.
    - `src/canonical.js` (new): `canonicalReplenishBytes` matches
      Rust's encoding byte-for-byte;
      `verifyEd25519(rawPubKey32, message, signature64)` wraps raw
      key in SPKI DER prefix and uses Node's built-in
      `crypto.verify(null, ...)`.
    - `src/server.js`: `POST /v1/register` requires
      `ik_ed25519_pub`. New `GET /v1/prekey-bundle/:user_id`
      (atomic OPK pop, returns identity + SPK + OPK +
      `remaining_opk_count`; `opk: null` when pool empty per OPK
      exhaustion fallback). New `POST /v1/prekey-bundle/replenish`:
      verifies `batch_signature_b64` against the user's stored
      `IK_Ed25519`, then transactionally replaces SPK
      (current → previous) and appends OPKs. 401 on signature
      mismatch; 404 before register; 409 on duplicate OPK id;
      400 on shape errors.
- 60 keystore tests pass (was 32; +9 ed25519, +19 prekeys, +2
  prekey e2e through real keyserver subprocess; existing
  identity/storage/client tests updated for v3 schema).
- 31 keyserver tests pass (was 21; +10 prekey-bundle: 404 before
  replenish, 401 on bad/tampered signature, 200 happy path,
  atomic OPK pop until exhausted, SPK rotation moves current to
  previous, 404 before register, 400 on missing fields, 409 on
  duplicate id, `verifyEd25519` size-rejection).
- The `prekeys_e2e_test` integration test spawns the real Node
  keyserver as a subprocess on an ephemeral port, registers an
  identity, replenishes, fetches — proves the Rust
  `canonical_replenish_bytes` produces bytes that the Node
  server's `canonicalReplenishBytes` reconstructs verbatim
  (Ed25519 verification would fail otherwise). Skips automatically
  when `node` isn't on PATH or the keyserver hasn't been
  `npm install`-ed.
- Identity blob v2 → v3: existing on-disk identity files from B1
  fail to load with `Error::BlobVersionMismatch { got: 2,
  expected: 3 }` — caller regenerates. Acceptable for v1 alpha;
  no production data exists yet.

[CHECKPOINT B — Group B complete (TPM-sealed identity, Argon2id
password, duress flow, prekey infrastructure). Awaiting review
before proceeding to Group C (burn flows, selector manifest, Stego
Mode 1).]

### Layer B-fixup — duress prekey wiring

- `WipeStep::PrekeyFile` (new) added to the duress engine. Wired
  automatically when the caller supplies
  `DuressPaths::prekey_file: Option<PathBuf>` — engine deletes the
  sealed `prekeys.json` idempotently (missing file →
  `AlreadyClean`, present file → `Wiped`). When `None`, reports
  `Skipped` with a reason pointing at the path field.
- The existing in-memory `WipeStep::Prekeys` step's deferred
  message updated. Was: "handler wired by Layer B4 once prekey
  infrastructure exists" (B4 has now landed); now: "in-memory
  PrekeyState wipe handler not wired — caller sets
  DuressHandlers::wipe_prekeys to drop the live state (the on-disk
  file is handled by the PrekeyFile step)".
- Added test
  `execute_with_no_handlers_walks_every_step::skip_reasons` check:
  any `Skipped` reason that contains "Layer B4" (case-insensitive)
  fails the suite — locks in the invariant that landed-layer
  references stay accurate.
- 13 duress tests pass (+2 new: skipped-when-path-not-supplied,
  already-clean-when-file-missing).
- Total keystore: 62 tests (was 60; +2 duress).

### Layer C1 — Burn flows (server endpoint + client primitives + signed alerts + revalidation cycle)

- `keyserver/src/db.js` `burnWrappedKeys(db, burning_user_id, scope, target)`:
  always filters `sender_id = burning_user_id` so a request can only
  ever delete the caller's own rows. `scope = 'single' | 'to_user'
  | 'all'` map to `(content_id, sender)`, `(sender, recipient)`,
  and `(sender)` filters respectively. Returns `{ deleted_count }`.
- `keyserver/src/canonical.js` `canonicalBurnBytes({ user_id,
  scope, target })`: domain-prefixed length-prefixed encoding —
  `domain (LP) || user_id (LP) || scope (LP) || target_kind (u8:
  0 none, 1 content_id, 2 user_id) || [target_value (LP) if
  target_kind != 0]`. Server signs nothing; this is what the
  client signs and the server verifies.
- `DELETE /v1/wrapped-keys` endpoint: required body `{ scope,
  user_id, burn_signature_b64, target_content_id?,
  target_user_id? }`. Validates shape, requires the caller to be
  registered (404 before register), then `verifyEd25519` against
  `users.ik_ed25519_pub`. Bad signature → 401, target shape
  mismatch → 400, scope=`all` with target field → 400. On verify
  success calls `burnWrappedKeys`.
- Rust `keystore::burn` module: `BurnScope` enum (`Single { content_id }
  | ToUser { user_id } | All`), `canonical_burn_bytes` (matches
  Node byte-for-byte), `sign_burn` helper.
- Rust `KeyServerClient::burn(identity, &BurnScope) -> Result<BurnResponse>`
  signs canonical bytes with `identity.ed25519_secret` and issues
  `DELETE /v1/wrapped-keys` over the existing send_request helper.
  Body shape mirrors the server schema (skips absent target fields).
- `keystore::burn_alert` module: signed payload for the
  `system_message_kind = "burn-alert"` system-message path (no new
  endpoint — reuses POST `/v1/wrapped-keys`). Domain
  `discord-privacy-client/burn-alert/v1`; signs over `(sender_id,
  recipient_id, scope_label, alert_text, issued_at_unix_seconds)`.
  Recipients verify against the sender's `IK_Ed25519` so the
  server can't forge alerts.
- Re-validation cycle in new `runtime::revalidation` module:
  `RevalidationLoop` is caller-driven (no internal task — tests
  use `MockClock`, integration layer wires `poll()` to a tokio
  interval). Tracks per-`content_id` state, fires
  `TransitionCallback` on transitions
  (`Present → Burned`/`Tombstoned`). Default 5-minute timer
  (`docs/design/key-server-api.md` § "Re-validation"); also
  exposes `probe_now()` for window-focus / interaction triggers.
  Probe HTTP coupling left to a `Probe` trait so the integration
  layer (in `src-tauri`) supplies the `KeyServerClient`-backed
  implementation; runtime crate stays HTTP-free.
- Cover-text fallback semantics implemented as
  `runtime::revalidation::render_decision(WrappedKeyState,
  ContentKind) -> Option<RenderDecision>`. Hard-coded per design
  doc table: `text` → `KeepCoverText` (NO "[deleted]" marker;
  observer indistinguishability), `attachment` →
  `AttachmentUnavailable`, `system` → `SystemUnavailable`.
- 12 keyserver burn tests (43 total, was 31): single deletes own +
  leaves others, can't burn another user's content (deleted_count
  = 0), to_user filters sender+recipient, all deletes every row
  alice sent, 401 on bad sig, 400 on shape errors, 404 before
  register, burn-and-alert via existing system-message path.
- 8 Rust burn / burn-alert lib tests, 2 burn end-to-end tests
  (round-trip + to_user) that spawn the real Node keyserver and
  drive `KeyServerClient::burn` end-to-end — proves Rust
  `canonical_burn_bytes` produces the same bytes Node's
  `canonicalBurnBytes` reconstructs (Ed25519 would fail
  otherwise).
- 7 `runtime::revalidation` lib tests: render-decision table
  matches design doc, poll below interval is no-op, poll at
  interval fires callback on transition, `probe_now` fires
  immediately + sticks-no-double-fire, `untrack` stops probing,
  `last_state` query, `track` is idempotent on re-call.
- Keystore now: 105 tests across lib + 9 integration files
  (`+8` burn / burn_alert lib tests, `+2` burn e2e, others
  unchanged from B-fixup). Runtime now: 66 tests across lib + 4
  integration files (`+7` revalidation lib tests). Keyserver now:
  43 tests (`+12` burn).

### Layer C2 — Selector manifest (signing + fetch + fail-closed + CDN mirror)

- `selectors::manifest`:
  - `SelectorManifest { version, issued_at_unix_seconds,
    client_min_version, selectors: BTreeMap<String, String> }`. The
    `BTreeMap` keeps the on-wire order deterministic.
  - `canonical_manifest_bytes` — length-prefixed encoding mirroring
    the prekey / burn pattern: domain LP, version u32 BE,
    issued_at u64 BE, client_min_version LP, selector_count u32 BE,
    then each (key LP, value LP) sorted lexicographically by key.
    Domain string `discord-privacy-client/selector-manifest/v1`.
  - `sign_manifest(secret, public, &SelectorManifest) ->
    SignedManifest` envelope: `{ version: 1, manifest_b64,
    signature_b64, signing_key_b64 }`. The envelope carries the
    canonical bytes directly (not the parsed JSON) so JSON
    pretty-print quirks can never desynchronise the signed payload.
  - `verify_manifest(&SignedManifest, trusted_pub_b64,
    now_unix_seconds)` — five gates in order: envelope version == 1,
    signing-key strict equality with the *trusted* constant
    (server-presented key is never trusted on its own), Ed25519
    signature, canonical-bytes round-trip equivalence (parsed
    manifest → re-encoded must equal the signed bytes), 24-h
    staleness gate (`MAX_MANIFEST_AGE_SECONDS` constant), future-
    issued rejection.
- `selectors::fetcher`:
  - `ManifestSource` trait — supplies raw envelope JSON bytes; tests
    use a queue-driven `MockSource`, integration layer wires
    HTTP-backed sources for primary + CDN. Crate stays HTTP-free.
  - `ManifestFetcher::refresh(now)` — try primary; on any failure
    (transport / parse / signature / staleness) try CDN; on both
    failures transition to `ManifestState::FailClosed { reason }`.
    Reason string includes both source errors so the integration
    layer can surface diagnostic detail.
  - `ManifestState::{NotYetFetched, Loaded { manifest, from },
    FailClosed { reason }}`. Initial state is `NotYetFetched`
    (banner suppressed — we haven't *failed*, we just haven't
    fetched yet).
  - `reconsider_staleness(now)` — re-evaluate freshness without
    fetching. Caller invokes on clock advance to flip `Loaded` →
    `FailClosed` once age > 24 h, without waiting for the next
    refresh tick.
- Server-side: `keyserver/src/canonical.js` adds
  `canonicalManifestBytes(manifest)` mirroring the Rust encoder.
  `GET /v1/selector-manifest` endpoint serves the configured
  envelope verbatim or 503 when unconfigured (so clients fall
  through to the CDN mirror per design). `buildServer` accepts
  `selectorManifest`; the script entrypoint loads from
  `SELECTOR_MANIFEST_PATH` env var.
- 20 selectors lib tests: round-trip sign+verify, reject
  wrong-key (mismatched trusted constant), reject tampered
  manifest_b64, reject when signed bytes ≠ provided bytes
  (canonical-round-trip check), reject stale, accept at exact 24-h
  boundary, reject future-issued, reject envelope version != 1,
  canonical bytes deterministic under selector insert order, parse
  envelope round-trip, empty selectors map round-trips, fetcher
  primary success / fallback to CDN / both-fail FailClosed /
  signature-fail-on-primary fallback / signature-fail-both
  FailClosed / stale-on-both FailClosed / `reconsider_staleness`
  flips Loaded→FailClosed at 25h, keeps Loaded at 12h, initial
  state is `NotYetFetched`.
- 2 selectors e2e tests (`crates/selectors/tests/keyserver_e2e_test.rs`):
  spawn the real Node keyserver with a manifest envelope signed
  *by Node* using `canonicalManifestBytes`, fetch via an
  HTTP `ManifestSource` adapter, then run Rust
  `verify_manifest` — proves Rust `canonical_manifest_bytes`
  byte-equals Node's. Second test spawns two keyservers (primary
  unconfigured returning 503, CDN serving the envelope) to
  exercise the CDN-mirror fallback path against real HTTP. Skipped
  when `node` isn't on PATH.
- 4 keyserver manifest tests (47 total, was 43): 503 when
  unconfigured, returns envelope verbatim when configured,
  `canonicalManifestBytes` is stable under selector insert
  order, exact byte layout matches the spec.
- Selectors crate: 22 tests total (was 0 — the lib was a
  scaffolding placeholder).

### Layer C3 — Stego Mode 1 (templates + bit-packing + per-conversation salt)

- `stego::mode1` module — fluent template-based stego encoder
  layered alongside the existing Mode 0 base64 placeholder. Wire
  prefix `DPC1::`. Each emitted sentence carries 20 bits of
  payload: 4 bits select one of 16 templates, plus 2 × 8-bit slot
  fills from per-conversation-permuted wordlists.
- Per-message independence preserved (the design's hard
  architectural invariant): every Mode 1 message decodes from
  itself + the conversation cipher with no reference to any
  other message — exercised by `mode1_test.rs ::
  per_message_independence_each_message_decodes_alone`, which
  shuffles encoded messages and decodes in reverse order.
- `ConversationCipher::from_salt(&[u8])` derives three
  permutations (16 templates / 256 nouns / 256 adjectives) via
  `HKDF-SHA256` with domain
  `discord-privacy-client/stego-mode1/permutation/v1`. Same salt
  → identical permutations on encode + decode; different salts →
  uncorrelated permutations so two parallel conversations
  encoding the same payload produce different surface text.
  Permutation built via Fisher-Yates with 16-bit random words
  per swap (widens modulo-bias surface vs. naive `% (i+1)` on
  single bytes).
- Wire format: `DPC1::` prefix || rendered sentences. Bit stream
  begins with a `u16 BE payload_len_bytes`; trailing bits in the
  final sentence are zero-padded for stable test output. The
  decoder reads the length prefix and stops after that many bytes.
- `MODE1_MAX_RAW_LEN = 80` bytes (cap fits comfortably under
  Discord's 2000-char limit; 80-byte payload encodes to ~70
  sentences and ~1700 chars in worst case). Verified by
  `fits_under_discord_2000_char_limit_at_max_raw`.
- Static template pool: 16 templates each with exactly 2 slots,
  unique skeletons (compile-time invariant
  `template_skeletons_are_unique`). Wordlists are disjoint from
  every template skeleton token (invariant
  `wordlists_disjoint_from_template_skeletons`) — would create
  decode ambiguity otherwise.
- Decoder: tokenises the body via `split_whitespace`, walks each
  template skeleton against the token stream, first match wins;
  unknown tokens → `Mode1ParseError`, which is the parser-error
  branch the integration layer routes back to "not stego, render
  as plain text".
- `stego::Error` gains `NotMode1`, `Mode1TooLong`,
  `Mode1ParseError`. `decode_mode1` returning `Mode1ParseError`
  is the cover-text-fallback signal — caller hands the message
  through unchanged.
- 15 mode1 lib tests + 6 mode1 integration tests:
  - Round-trip across 0-byte / 1-byte / multi-length / max-length
    payloads, several byte distributions (all-zero, all-FF,
    alternating, 0..32, 0..MAX).
  - `same_salt_is_deterministic`,
    `different_salts_produce_different_cover_text`,
    `cross_cipher_decode_does_not_recover_payload`.
  - `permutation_is_a_bijection_for_templates` /
    `_for_nouns` (sanity check on Fisher-Yates output).
  - `cover_text_uses_only_known_words_and_skeleton_tokens`
    (every emitted token round-trips back to either a wordlist
    entry or a template fixed token).
  - `template_pool_size_matches_expected_bit_budget`
    (16 templates, 2 slots, 4 + 16 = 20 bits/sentence).
  - `wordlists_disjoint_from_template_skeletons`,
    `template_skeletons_are_unique` (compile-time invariants).
  - `rejects_oversize_payload`, `rejects_missing_prefix`,
    `rejects_empty_after_prefix`, `rejects_corrupted_token_in_middle`.
  - `fits_under_discord_2000_char_limit_at_max_raw`.
  - `per_message_independence_each_message_decodes_alone` (the
    hard design invariant).
  - `cross_cipher_decode_does_not_recover_payload` (per-
    conversation salt actually changes output).
  - `detects_mode1_vs_mode0_prefix`.
- Stego crate: 32 tests total (was 11 — `+21` Mode 1 tests
  across lib + integration). Mode 0 unchanged.

[CHECKPOINT C — Group C complete (burn flows, selector manifest,
Stego Mode 1). Awaiting review before proceeding to Discord
integration (Layers 9-11) per docs/design/build-order.md.]

### Layer 9 — Tauri shell loading discord.com webview

- `src-tauri/tauri.conf.json` `windows[0]`:
    - `url = "https://discord.com/app"` — the main window now opens
      directly on Discord's web app (`WebviewUrl::External`) rather
      than a localhost stub. Width / height / resize chrome
      unchanged from the Layer 8 placeholder.
    - `userAgent =
      "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36
       (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36
       Edg/130.0.0.0"` — pinned to a recent realistic Edge UA so
      Discord's client-detection doesn't flag the WebView2 default
      string. Bump when Discord starts warning about Chrome 130 being
      old; no protocol consequence.
- `app.security.csp` left at `null` (unchanged). Tauri only injects
  CSP into local content; for the external-URL main window Discord's
  server-sent CSP governs. An explicit Tauri-side allowlist would be
  cosmetic and is deliberately omitted. Layer 11's local overlay UI
  will revisit CSP for that window.
- Cookie persistence: WebView2's default user-data folder under
  `%LOCALAPPDATA%\org.discord-privacy.client\EBWebView` keeps cookies
  across restarts with no extra config — Discord login survives a
  full app close/reopen. Verified by the user on a Windows host
  (deferred task — see below).
- `crates/runtime/src/screenshot.rs`:
    - New `apply_to_hwnd_and_children(hwnd, protection)` walks the
      parent and every descendant via `EnumChildWindows` and calls
      `SetWindowDisplayAffinity` on each. Per Microsoft Learn,
      `EnumChildWindows` already recurses into grandchildren, so a
      single call covers the WebView2 tree
      (`Chrome_WidgetWin_*` host + `Chrome_RenderWidgetHostHWND`).
    - Parent failure short-circuits with a typed `ScreenshotError`.
      Descendant failures are recorded into a per-call `ChildState`
      threaded through `LPARAM` and surfaced via `tracing::info!`
      (visited / succeeded counts) and `tracing::debug!` (first error
      message). Cross-process WebView2 render hosts may legitimately
      fail with access-denied; not a reason to fail the whole call.
    - Module docs explain the WebView2-propagation rationale (Tauri
      parent + WebView2 children, observed propagation drift on some
      Windows builds, why descendants need explicit application).
    - Re-exported from `runtime::lib.rs` alongside the existing
      `apply_to_hwnd`.
- `src-tauri/src/screenshot.rs::apply_to_window` now forwards to
  `runtime::apply_to_hwnd_and_children` instead of the parent-only
  `apply_to_hwnd`, so both the startup application and the
  `set_screenshot_protection` Tauri command benefit from the
  child-walk path.
- `src-tauri/src/main.rs::setup`:
    - Calls `apply_to_window(..., ScreenshotProtection::On)` once at
      startup (unchanged shape; uses the new child-walking impl
      transparently).
    - Adds a `WebviewWindow::on_page_load` callback that re-applies
      the same protection on every page-load event. Catches
      WebView2 child HWNDs that don't exist yet at `app.setup`
      time (the navigation to discord.com is still in flight) and
      any new descendants Discord's SPA spawns mid-session
      (embeds, modals, popouts). `apply_to_hwnd_and_children` is
      idempotent on the parent and best-effort on descendants, so
      re-applying is safe.
    - Module-level comment block updated to describe the layer-9
      shell — replaces the old "Layer 9 onward replaces this main"
      placeholder.
- `crates/runtime/tests/screenshot_test.rs`: added
  `linux_macos_stub_with_children_is_a_noop_for_both_states`
  covering the new function on the non-Windows stub path. Pattern
  matches the existing parent-only test. Mirrors existing parent-
  only no-op invariants (arbitrary HWND values, both protection
  states). 4 runtime screenshot tests now (was 3).
- `webview/dist/index.html` placeholder added so Tauri's
  `generate_context!()` macro can resolve the configured
  `frontendDist` path. The placeholder is never visible at runtime
  — the main window navigates to `https://discord.com/app`
  directly. Will be replaced by the Layer 11 overlay-UI bundle
  built from `webview/src/`.

**Out of scope for Layer 9** (per build-order, deferred to Layers
10–11): Discord SPA injection, webpack-selector hooks, message
plumbing, encryption-UI overlays, `dangerousRemoteDomainIpcAccess`
allowlist for IPC-from-Discord. None of those are touched in this
layer.

**Standing-instruction tests:** `cargo test -p runtime` exercises
the cross-platform stub of `apply_to_hwnd_and_children` plus the
existing ~80 runtime tests; on WSL this runs cleanly. `cargo check
-p discord-privacy-client` still fails on WSL because of unrelated
GTK / WebKit system-deps that the Tauri Linux feature gate pulls
in (gio-sys / gobject-sys / glib-sys build scripts) — same caveat
as Layer 8 and the Group A Win32 paths. The Tauri attribute glue,
`webview/dist/index.html` resolution, and `tauri.conf.json` schema
acceptance are deferred to the user's Windows host — see below.

**Deferred Windows-host verification task** (explicitly):

The actual end-to-end behaviour of this layer (Tauri window opens,
discord.com renders, login persists across restart, capture
protection applied to WebView2 children, cookies survive a relaunch)
**cannot be tested in WSL or any headless CI environment**. There is
no headless WebView2 runner, and `SetWindowDisplayAffinity` only
takes effect against a real running Windows compositor.

Verification steps are written up in
`docs/design/layer-9-windows-verification.md`. Five hand-checks
required on a Windows 10 build 2004+ or Windows 11 host with
WebView2 Runtime installed:

1. Tauri window opens at launch.
2. discord.com loads and renders correctly.
3. Login persists across restart (cookie persistence).
4. Capture protection verifiable via Snipping Tool / `Win+Shift+S` —
   chat panel must render as solid black in the snip output.
5. Cookie persistence verified (covered by 3).

Sign-off on those five checks marks Layer 9 verified.

### Layer 9-fixup — windows-rs 0.56.0 API-shape fixes

First Windows-host `cargo tauri dev` surfaced a cluster of windows-rs
0.56.0 signature mismatches that had been latent since Layers A2/A3/B1.
The `cfg(windows)` paths had only ever been reasoned about, never
compiled — Linux builds skip them, and CI never actually runs on
Windows. This commit cross-compiles them via
`cargo check -p <crate> --target x86_64-pc-windows-gnu` so the type
checks run end-to-end before the next Windows attempt.

**No cryptographic behaviour changes.** Wire format, AEAD parameters,
key sizes, padding, AAD encoding, KDF labels — all untouched. The
fixes are purely about matching windows-rs 0.56.0's actual function
signatures and type definitions.

- `crates/runtime/src/screenshot.rs` (Layer A2):
    - `HWND` constructor in 0.56.0 is `HWND(pub isize)`, not
      `HWND(*mut c_void)`. Both call sites switched from
      `HWND(hwnd_isize as *mut core::ffi::c_void)` to
      `HWND(hwnd_isize)` with an inline note. Affects both
      `apply` (parent) and `apply_with_children` (Layer 9 walk).

- `crates/runtime/src/usb.rs` (Layer A3 — first time it has compiled
  for a Windows target):
    - `WNDCLASSEXW.hInstance` is `HINSTANCE`; `GetModuleHandleW`
      returns `HMODULE`. Distinct tuple structs in 0.56.0; relies on
      the upstream `From<HMODULE> for HINSTANCE` impl. Fix: pass
      `hinstance.into()` into the struct literal.
    - `CreateWindowExW` in 0.56.0 returns `HWND` directly (not
      `Result<HWND>`); a NULL/zero return signals failure. The old
      code's `.map_err(|e| ...)?` was a no-op because there's no
      `map_err` on `HWND`. Replaced with an explicit
      `if hwnd.0 == 0 { Err(windows::core::Error::from_win32()...) }`
      check.
    - `DBT_DEVTYP_DEVICEINTERFACE` is the typed wrapper
      `DEV_BROADCAST_HDR_DEVICE_TYPE(pub u32)`. The two struct
      fields it gets compared against in this file have **different
      types**:
        - `DEV_BROADCAST_DEVICEINTERFACE_W.dbcc_devicetype: u32`
          → unwrap as `DBT_DEVTYP_DEVICEINTERFACE.0` at the
          registration call.
        - `DEV_BROADCAST_HDR.dbch_devicetype: DEV_BROADCAST_HDR_DEVICE_TYPE`
          → compare as `DBT_DEVTYP_DEVICEINTERFACE` (no `.0`) in the
          WndProc dispatch path.
      The asymmetry is documented inline at both call sites.
    - Removed an unused `PostMessageW` import (`-D warnings` would
      otherwise reject the cross-compile).

- `crates/keystore/src/sealer.rs` (Layer B1 — `TpmSealer` first time
  it has compiled for a Windows target):
    - `NCryptOpenStorageProvider` / `NCryptOpenKey` /
      `NCryptCreatePersistedKey` / `NCryptFinalizeKey` /
      `NCryptEncrypt` / `NCryptDecrypt` all return `Result<()>`
      directly in 0.56.0. The old code used the `.ok()`-on-NTSTATUS
      pattern (`.ok().map_err(...)`) which doesn't apply: `.ok()`
      on `Result<()>` returns `Option<()>`, and `Option` has no
      `map_err`. Dropped `.ok()` from all six call sites; pattern is
      now plain `.map_err(...)?`.
    - `NCryptOpenKey` and `NCryptCreatePersistedKey` take
      `dwlegacykeyspec: CERT_KEY_SPEC` (typed wrapper), not raw
      `u32`. Pass `CERT_KEY_SPEC(0)` for "no legacy KSP / use
      CNG-native key spec". Added `CERT_KEY_SPEC` to the imports.
    - `NCryptEncrypt` / `NCryptDecrypt` take `pcboutput: *mut u32`
      as a raw pointer (was `Option<&mut u32>` in earlier 0.5x
      versions). Replaced `Some(&mut wrapped_len)` /
      `Some(&mut got)` with `&mut wrapped_len as *mut u32` /
      `&mut got as *mut u32` at all four call sites.
    - `NCryptEncrypt` / `NCryptDecrypt` `dwflags` is `NCRYPT_FLAGS`,
      not raw `u32`. `BCRYPT_PAD_PKCS1` itself is a `BCRYPT_PAD_FLAG`
      (typed). Wrap as `NCRYPT_FLAGS(BCRYPT_PAD_PKCS1.0)`.

- `crates/runtime/Cargo.toml`:
    - Added `Win32_Graphics_Gdi` feature: `WNDCLASSEXW` carries
      `HBRUSH` and `HICON` fields whose types live there.
    - Added `Win32_System_Threading` feature: `GetCurrentThreadId`
      (used by the dedicated USB-monitor thread).
    - Pinned `windows = "=0.56.0"` (was `"0.56"`). Patch-version
      drift in this crate moves typed-wrapper boundaries (HWND
      inner type, NCrypt return shape, typed flag constants), and
      every `cfg(windows)` call site has to be re-walked when it
      moves. Pinning documents the version the code is verified
      against and forces a deliberate audit on bump.

- `crates/keystore/Cargo.toml`: pinned `windows = "=0.56.0"` for
  the same reason.

- `src-tauri/Cargo.toml`: pinned `windows = "=0.56.0"` so the
  workspace resolves a single windows-rs version end-to-end.

**Verification (Layer 9-fixup):**

- `cargo check -p runtime --target x86_64-pc-windows-gnu` — green
  (1 pre-existing `dead_code` warning on `class_atom`; not
  introduced by this fix).
- `cargo check -p keystore --target x86_64-pc-windows-gnu` — green
  (1 pre-existing `unused_imports: c_void` warning; not introduced
  by this fix).
- `cargo check -p runtime -p keystore` (default Linux target) —
  green; `cfg(not(windows))` stub branches still resolve.
- `cargo test -p runtime` — 67 passed (unchanged); the Linux stubs
  for `screenshot` and `usb` are exercised, the Win32 paths aren't
  reachable from this target.
- `cargo test -p keystore --lib` — 8 lib tests passed; Win32 TPM
  paths gated to Windows host.

The user's previous `cargo tauri dev` attempt should now compile
through the runtime + keystore + src-tauri crates against
windows-rs 0.56.0 on a real Windows host. If new errors surface
that the cross-compile didn't catch (e.g. MinGW vs MSVC ABI
differences), they're worth reporting back since the cross-compile
target uses `gnu` while production is `msvc`.

**rustup target add x86_64-pc-windows-gnu** is a one-time WSL setup
step for future `cfg(windows)` audits — adds the pure-Rust
windows-gnu target so `cargo check` validates Win32 type usage
without leaving WSL. Recorded here so future contributors don't
have to re-derive it.

### Layer 9-fixup (round 2) — Tauri build script + page-load API

Second Windows-host `cargo tauri dev` after the round-1 windows-rs
fixes surfaced two pure src-tauri issues:

- **Missing `build.rs`**: `tauri::generate_context!()` panicked with
  `OUT_DIR env var is not set, do you have a build script?`. Tauri 2
  requires a build script that calls `tauri_build::build()` to parse
  `tauri.conf.json`, generate the codegen inputs, and emit
  `cargo:rustc-env=OUT_DIR=...` / capabilities / permission files
  the macro reads at compile time. Added `src-tauri/build.rs` with
  the canonical `tauri_build::build()` entry point.
  `tauri-build` was already in `[build-dependencies]`; Cargo's
  default `build = "build.rs"` resolution picks the file up
  automatically.

- **`WebviewWindow::on_page_load` doesn't exist post-creation**:
  Tauri 2 exposes page-load callbacks two ways — on
  `WebviewBuilder` during construction, or on the app-level
  `tauri::Builder` for an app-wide hook that fires for every
  webview's page loads. `WebviewWindow` after creation has no
  page-load listener API. Refactored to register the handler on
  the app `Builder` chain instead of on the window inside `setup`,
  preserving the original intent (re-apply screenshot capture
  protection on every navigation event so WebView2 child HWNDs
  spawned during render get the affinity flag).

- **`Builder::on_page_load` callback signature**: the closure
  receives `&tauri::Webview` (not `&WebviewWindow`) — page loads
  are webview-scoped because Tauri 2 supports multiple webviews
  per window. Added a parallel `screenshot::apply_to_webview`
  helper alongside the existing `apply_to_window`. The new helper
  goes through `webview.window().hwnd()` because
  `tauri::Webview::hwnd()` doesn't exist directly in 2.11.1 (the
  HWND lives on the parent `Window`, reachable via `Webview::window()`).
  The `EnumChildWindows` walk inside
  `runtime::apply_to_hwnd_and_children` then descends from the
  parent into the WebView2 host + render surface as before — same
  protection, same idempotent re-application, just routed through
  the app-level callback shape.

- **Placeholder `src-tauri/icons/icon.ico` (1118 bytes)**: minimum-
  valid 16×16 transparent ICO. `tauri-build`'s Windows-target
  resource step requires an icon file to compile into the .exe's
  resources; `tauri.conf.json` had `"bundle.icon": []` (empty),
  which falls back to looking for `icons/icon.ico` and errored.
  Replace before ship with a real icon. Generated via a one-shot
  Python script (recorded in CHANGELOG so the placeholder's
  provenance is auditable; bytes are deterministic from the script).

**Verification (Layer 9-fixup round 2):**

- `cargo check -p discord-privacy-client --target x86_64-pc-windows-gnu`
  — green. Two pre-existing warnings only (`class_atom` dead_code
  in `runtime::usb`, `c_void` unused_imports in `keystore::sealer`);
  neither introduced by these fixes.
- WSL cross-compile additionally needs `x86_64-w64-mingw32-windres`
  (from `mingw-w64`) for `tauri-winres` to compile the resource
  file. If `mingw-w64` isn't installed, a temporary stub script in
  `/tmp/winres-stub/x86_64-w64-mingw32-windres` (writes an empty
  output file and exits 0) is enough for type-checking purposes —
  the linker isn't invoked by `cargo check`. On a real Windows host
  this isn't needed; MSVC's `rc.exe` handles the resource step
  natively.
- Pre-existing unrelated cleanup (`PostMessageW` import dropped in
  round 1) unchanged.

Now ready for another Windows-host `cargo tauri dev` attempt. The
on-page-load handler runs at app level and applies to every webview
created from any window declared in `tauri.conf.json` — the main
window in our case.
