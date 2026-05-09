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

### Build state

`cargo check -p crypto --tests` was attempted in WSL but blocked by
missing `build-essential` / `libc6-dev` on that machine (no `cc`
linker). Verification is pending on a Windows-native dev env or a WSL
env with `build-essential` installed.

The most likely first-iteration issue if cargo check runs is the
`dryoc::classic::crypto_aead_xchacha20poly1305_ietf::encrypt` /
`decrypt` signature; if dryoc 0.7's actual API differs from the
assumed `(ciphertext: &mut [u8], plaintext: &[u8], ad: Option<&[u8]>,
nonce: &Nonce, key: &Key) → Result<(), _>` shape, `crates/crypto/src/aead.rs`
is the only file affected.
