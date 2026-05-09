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

### Build state

`cargo check -p crypto --tests` and `cargo test -p crypto` both green
on Windows: 147/147 tests pass across the full crypto crate
(aead, attachment, hkdf, ml_kem_768, padding, pqxdh, ratchet,
sender_keys, wire, x25519). Verified at the layer-4 checkpoint.
