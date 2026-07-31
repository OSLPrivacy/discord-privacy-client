# Evidence 03 — at-rest and sealer posture

Supports README §7 (`at_rest_and_sealer_posture`).

All citations are `path:line` against the working tree pinned by `reviewed-file-hashes.txt`.
**This section is source review. No sealer was exercised at runtime other than through the
`keystore` test suite, which does not cover the TPM or OS-keyring paths (see `01-test-runs.md`).**

---

## 1. The `Sealer` trait

`crates/keystore/src/sealer.rs:75`

```rust
pub trait Sealer: Send + Sync {
    fn method_label(&self) -> &'static str;                              // :76
    fn is_tpm_backed(&self) -> bool;                                     // :77
    fn requires_insecure_banner(&self) -> bool;                          // :78
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>>;                 // :79
    fn unseal(&self, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>>;   // :88
}
```

- `:71-74` — implementations MUST authenticate the plaintext, so tampered ciphertext is rejected
  on `unseal`.
- `:80-87` — `unseal` returning `Zeroizing<Vec<u8>>` rather than `Vec<u8>` is a deliberate control,
  not incidental.

Method-tag constants (`:37-41`): `METHOD_TPM = "tpm-pcp"`, `METHOD_KEYRING = "keyring"`,
`METHOD_NOOP = "noop-insecure"`, `METHOD_MEMORY = "memory-test"`,
`METHOD_EPHEMERAL = "memory-ephemeral"`.

## 2. Implementations

| Impl | Def | `impl Sealer` | Key material | AEAD |
|---|---|---|---|---|
| `NoOpSealer` | `:115` | `:123-139` | **none** | **none** — `seal` is `plaintext.to_vec()` (`:133-135`) |
| `MemorySealer` | `:144` | `:197-213` | random 32-byte key, never persisted (`:157`) | XChaCha20-Poly1305 |
| `EphemeralProcessSealer` (private) | `:165` | `:179-195` | process-wide `OnceLock` random key (`:171-172`) | XChaCha20-Poly1305 |
| `KeyringSealer` | `:227` | `:307-323` | 32-byte random key, base64, in the OS credential store via the `keyring` crate. Service `"discord-privacy-client"` (`:231`), user `"identity-data-key.v1"` (`:232`). **No KDF — not password-derived** (`:257`). | XChaCha20-Poly1305 |
| `TpmSealer` (Windows, `mod tpm` `:328`) | `:344` | `:416-561` | Hybrid: random 32-byte data key wrapped by a **TPM-resident RSA key** via **Microsoft Platform Crypto Provider / NCrypt**, **PKCS#1 v1.5** (`BCRYPT_PAD_PKCS1` at `:449`, `:465`, `:516`, `:534`). Provider `"Microsoft Platform Crypto Provider"` (`:340`), key `"DiscordPrivacyClientIdentityKeyV1"` (`:341`). Blob layout `u32 BE wrapped_len ‖ wrapped ‖ aead_blob` (`:480-485`). | XChaCha20-Poly1305 |
| `TpmSealer` (non-Windows stub) | `:607` | `:619-639` | always errors (`:629-638`) | — |

Test-only implementations: `SealFailure` (`crates/keystore/tests/sealer_test.rs:8`),
`WrongRoundTrip` (`:28`), `SealFailure` (`crates/ipc/src/wire_rn.rs:2674`).

**Not DPAPI.** Windows protection is NCrypt/PCP, not `CryptProtectData`. (This was checked within
`crates/keystore`; no repo-wide DPAPI grep was performed.)

### Shared AEAD helper

`seal_with_aead_key` (`:682`) / `unseal_with_aead_key` (`:691`). Fresh 24-byte random nonce per
seal (`:683`); fixed AAD `SEAL_NONCE_PREFIX = b"discord-privacy-client/keystore/seal/v1"` (`:676`);
output layout `nonce ‖ ciphertext` (`:685-687`).

Primitive: `crates/crypto/src/aead.rs` — XChaCha20-Poly1305-IETF via RustCrypto
`chacha20poly1305` (`:31`, `:69`, `:89`); `KEY_SIZE = 32` (`:34`), `NONCE_SIZE = 24` (`:35`),
`TAG_SIZE = 16` (`:36`).

## 3. `select_best_sealer()` — `crates/keystore/src/sealer.rs:655`

Decision order (`:656-671`):

1. `#[cfg(windows)]` → `TpmSealer::new()` **and** `verify_sealer_round_trip(&s).is_ok()` → return.
2. `KeyringSealer::new()` **and** round-trip ok → return.
3. Otherwise `EphemeralProcessSealer::new()` (`debug_assert!` round-trip only).

`verify_sealer_round_trip` (`:98`) seals and unseals the probe
`b"OSL/keystore/sealer-readiness/v1"` (`:99`) and byte-compares (`:102-106`). Rationale at
`:91-97`: Windows PCP can open a handle while the TPM is not actually ready to encrypt.

**There is no path in this function that returns `NoOpSealer` or `MemorySealer`.**

Production call sites of `select_best_sealer()` observed: `crates/ipc/src/state.rs:406`,
`state_reload.rs:369`, `main_password.rs:1104`, `fresh_start.rs:142`,
`license_lifecycle.rs:129/157/189`, `wire_rn.rs:650/728/916/990/1044`.
**Not exhaustively audited** — whether any shipping path constructs `KeyringSealer::new()` or
`NoOpSealer::new()` directly is *not verified*.

Corroborating test: `crates/ipc/src/wire_rn.rs:3284`
`b42_rn_session_store_uses_select_best_sealer_for_at_rest_sessions` asserts the persisted blob's
`method` is in `{tpm-pcp, keyring, memory-ephemeral}` (`:3299-3308`) and is neither
`noop-insecure` nor `memory-test` (`:3309-3310`). **This test lives in `-p ipc`, which was not
run in this package** (see `01-test-runs.md`).

### Three things to press on

1. `NoOpSealer` and `MemorySealer` are **not `#[cfg(test)]`-gated**; both are publicly re-exported
   from the production library (`crates/keystore/src/lib.rs:99-103`). Only the *selection function*
   refuses them. `NoOpSealer`'s own doc (`sealer.rs:112-113`) says it is "retained for explicit
   compatibility tests only".
2. The non-Windows `TpmSealer` stub reports `is_tpm_backed() == true` (`:623-625`) and
   `requires_insecure_banner() == false` (`:626-628`) while being unable to seal anything.
3. TPM RSA modulus size: the comment at `:367` says 2048-bit, but `NCryptCreatePersistedKey`
   (`:374-381`) is called without an explicit length property, so the real size is the PCP default.
   **Not determined from source.**

## 4. `requires_insecure_banner()` — a self-report

Only `NoOpSealer` returns `true` (`:130-132`). All others return `false`
(`:186`, `:204`, `:314`, `:423`, `:626`).

Consumers that **write a visible banner** when it is true:
`crates/keystore/src/storage.rs:140` (banner constant `:51`:
`"INSECURE prototype storage — plain JSON, no passphrase, no TPM. …"`), `prekeys.rs:399`,
`pending_rotation.rs:99`, `license_cache.rs:100`, `password.rs:359`.

Consumers that **refuse** when it is true:
`crates/ipc/src/wire_rn.rs:660-662` → `RnError::PlaintextSealerRefused`.
Assertions elsewhere: `crates/ipc/src/state.rs:1101-1108`,
`crates/ipc/src/commands.rs:4064-4071`, `:5316-5317`.

Tests: `crates/keystore/tests/sealer_test.rs:61` (`NoOpSealer` → true), `:109`
(`MemorySealer` → false). **These two were exercised** — they are in the `keystore` suite that ran.

**The weakness, stated:** the refusal at `wire_rn.rs:660-662` trusts the sealer's own answer. A
sealer that encrypts badly, or with a constant key, returns `false` and is accepted.

## 5. RN sealed-session file format and its two gates

Envelope (`crates/ipc/src/wire_rn.rs:588-592`):

```rust
struct SealedBlob { version: u32, method: String, sealed_b64: String }
```

Written at `:668-672`. `SESSION_BLOB_VERSION = 1` (`:92`), version checked on load (`:750-755`).
Method label compared **constant-time** via `subtle::ConstantTimeEq` (`:759-769`).
Plaintext export zeroized immediately after sealing (`:663-666`) and after import (`:775-777`).

Pin file: `{"version": PIN_BLOB_VERSION, "pin": …}` (`:834-838`), `PIN_BLOB_VERSION = 1` (`:94`),
checked at `:873-878`. **The pin file is plain JSON — it is not sealed.** It lives in a separate
file from session state on purpose (`:41-55`): deleting the session must not silently drop the
downgrade pin, and `delete_session` never touches it (`:791-802`).

Caps and refusals:

| Constant | Value | Line | On breach |
|---|---|---|---|
| `MAX_SESSION_FILE_BYTES` | 1 MiB | `:107` | `RnError::StateTooLarge` (`:185-188`), checked on **both** write (`:675-680`) and read (`:738`) |
| `MAX_PIN_FILE_BYTES` | 4 KiB | `:110` | bounded read (`:862`) |
| `MAX_SESSION_RECORDS` | 512 | `:129` | `RnError::StoreFull` (`:193-194`) — **refuses the new record, never evicts a live one** (`:682-694`) |
| `MAX_SKIPPED_KEYS_POLICY` | 4096 | `:141` | `RnError::SkippedCacheTooLarge` (`:198-199`), enforced on import (`:781-787`) |

File naming: first 16 bytes of `SHA-256(b"OSL-RN/v1/session-file/" ‖ peer_x25519)`, hex
(`:606-612`); `.session` (`:614-617`), `.pin` (`:619-622`), `.pin-floor` (`:624-629`).

### Gate 1 — `a_plaintext_sealer_is_refused` (`wire_rn.rs:3262-3281`)

Asserts (a) `save_session_with_sealer(…, &NoOpSealer)` is
`Err(RnError::PlaintextSealerRefused)` (`:3272-3275`), and (b) **nothing was written** — a
subsequent load with a real sealer returns `Ok(None)` (`:3276-3280`).

This is the load-bearing gate. It is a genuine behaviour test, not a source-text test.

### Gate 2 — `the_sealed_file_contains_no_recognisable_state` (`wire_rn.rs:3353-3396`)

Seals a real `Session` with `MemorySealer`, reads the raw file, asserts the bytes contain none of:
the raw `SecureSession::export` plaintext (`:3369-3373`), its standard base64 (`:3374-3379`), its
base64url-nopad (`:3380-3384`), its lowercase hex (`:3385-3392`), or the peer's 32-byte X25519
identity key (`:3393-3395`).

**This is a substring blacklist of four encodings.** It cannot detect a weak cipher, a constant
key, or compression. **It is therefore not an independent second gate on the at-rest guarantee.**
Genuine two-gating would need a positive assertion that the bytes decrypt back under the real
sealer's key and under no other key. That assertion does not exist in this tree.

Both gates are in `-p ipc` and **were not run by this package**.

## 6. `crates/store` — schema, refusals, on-disk exposure, burn

### Version constants (`crates/store/src/schema.rs`)

```
SCHEMA_VERSION                     = 8    :68   <-- current
PRIVACY_SCHEMA_VERSION             = 4    :69
MESSAGE_ENVELOPE_SCHEMA_VERSION    = 5    :70
ATTACHMENT_ENVELOPE_SCHEMA_VERSION = 6    :71
ATTACHMENT_MANIFEST_SCHEMA_VERSION = 7    :72
CANARY_PLAINTEXT = b"osl-message-store-canary-v1"   :77
CANARY_AAD       = b"osl-message-store/canary"      :81
```

**The store is at v8, not v4.** Any document calling v4 "current" is wrong as of this tree.

v4 (`schema.rs:49-53`, verbatim):

> *v4 — 2026-07-26. Removes every plaintext identifier from disk. Ids and timestamps move inside a
> sealed per-row blob; lookups run against keyed blind indexes; ordering runs against an opaque
> seq. Closes the audit finding that an offline reader could reconstruct the protected social graph
> without the store key.*

Migration `migrate_v3_to_v4` at `:390` (builds `messages_v4` `:398`, `attachments_v4` `:419`,
renames `:659-660`, stamps the version in the same transaction `:673`). Narrative:
`crates/store/SECURITY.md:357-411`. Later: v5/v6 per-record content keys, v7 sealed attachment
manifest, v8 removes plaintext `burned_at` (`schema.rs:54-67`).

### Refusal on a newer on-disk version (`schema.rs:222-228`)

```rust
Some(v) if v > SCHEMA_VERSION => {
    return Err(StoreError::Schema(format!(
        "on-disk schema version {v} is newer than this binary supports \
         ({SCHEMA_VERSION}); refusing to open"
    )));
}
```

`StoreError::Schema` at `crates/store/src/error.rs:57-58`; rationale `:19-23`.

Downgrade guard: permanently-NULL compatibility columns are retained so an old v3 reader reaches
its own refusal instead of failing earlier with "no such column"
(`schema.rs:115-122`, `:145-153`, `:180`, `:191`, rationale `:109-114`). Pinned old-reader fixture:
`crates/store/tests/fixtures/adff4e45_schema_reader.rs:18` (`SCHEMA_VERSION = 3`) with the same
refusal string at `:109-110`. **The test exercising that fixture was not run here** — `-p store`
was not run.

### What remains visible on disk

Census: `crates/store/SECURITY.md:130-147`, `:75-128`, `:152-190`.

Sealed inside `meta_ct`: Discord message ids, channel ids, sender Discord ids, sender OSL user
ids, exact timestamps, attachment filenames, MIME types, byte lengths (`SECURITY.md:157-167`).

Still in the clear:

- `seq` — opaque monotonic ordering (`schema.rs:103`)
- the `burned` bit (`schema.rs:104`) — deliberately queryable so re-put fails closed
- `content_version`, wrapped-key nonce/ciphertext lengths (`schema.rs:105-107`)
- **deterministic keyed blind indexes** `mid_bi`, `chan_bi`, `sender_bi`, `ck_bi` — equality and
  frequency leak (`SECURITY.md:145`, `:172-174`)
- `attachment_manifests(mid_bi, complete, generation)` (`schema.rs:168-174`)
- `_meta.key` plaintext: `schema_version`, canary nonce/ct, `vacuum_pending`,
  `shred_checkpoint_pending` (`SECURITY.md:79-85`)
- row counts, ciphertext lengths, page/WAL shape (`SECURITY.md:147`)

Exact burn time is **not** stored in v8 (`SECURITY.md:142`).

### Key derivation

`crates/store/src/cipher.rs`:

- master: `HKDF-SHA256(salt = "", ikm = identity_secret, info = "osl-message-store-v1")`
  (`:35`, derived `:71`)
- blind-index key: info `"osl-message-store-index-v1"` (`:40`, derived `:79`)
- anchor infos `:41-43`; blind index computed `:118`

`identity_secret` in production is the identity **X25519 secret key bytes**
(`crates/ipc/src/commands.rs:4903-4908`), passed to `open_production_message_store`
(`commands.rs:14843`, `:14852`, `:14883`) → `MessageStore::open_anchored`
(`crates/store/src/lib.rs:548`).

### Burn

Terminal by SQL predicate, not by convention:

- messages: `WHERE messages.burned = 0` on the upsert's `DO UPDATE` — `crates/store/src/lib.rs:923`
- attachments: `WHERE attachments.burned = 0` — `:1469` (doc `:1283-1284`)
- `mark_burned` `:1055`; zeroing/stub SQL `:1174`, `:1191`, `:1207`, `:1217`; shred checkpoint
  `:1078`, `:1222`, `:1269`; `PRAGMA wal_checkpoint(TRUNCATE)` `:405`; `secure_delete=ON` `:631`;
  crash-resume of a pending shred `:692-699`
- deliberately *not* predicated on `burned = 0` (the re-shred path): `:424-425`, `:1046-1050`
- doc: `:780-796`

**Burn is local destruction only. It is not revocation, and there is no external monotonic
anchor** (`SECURITY.md:291-319`).

## 7. Identity key derivation and zeroization

### The identity is not password-derived

`crates/keystore/src/identity.rs`:

- `identity_from_entropy(entropy: [u8; 16], user_id) -> Identity` (`:246`) — deterministic from
  16 bytes of BIP39 entropy (doc `:235-245`)
- `seeded_rng(entropy, label) -> ChaCha20Rng` (`:228-233`) using
  `crypto::hkdf::derive_32(b"OSL-identity-v1", entropy, label)` at `:231` — HKDF-SHA256, salt
  `b"OSL-identity-v1"`, ikm = entropy, info = per-key label
- labels: `b"x25519"` (`:248`), `b"ed25519"` (`:252`), `b"mlkem768"` (`:256`),
  `b"ratchet-initial"` (`:269`)
- `NATIVE_ID_DOMAIN = b"OSL-NATIVE-IDENTITY-v1"` (`:13`), 20-byte hash (`:14`)
- `IDENTITY_BLOB_VERSION = 3` (`:29`)

HKDF primitive: `crates/crypto/src/hkdf.rs:19` / `:29`, `Hkdf::<Sha256>` (`:9`, `:21`).

### The password protects the recovery phrase

`crates/ipc/src/main_password.rs`:

- Argon2id, `ARGON_OUTPUT_LEN = 64` (`:66`), `ARGON_MEMORY_KB = 65_536` (`:73`),
  `ARGON_ITERATIONS = 3` (`:74`), `ARGON_PARALLELISM = 1` (`:75`)
- `derive(password, salt, params) -> Zeroizing<[u8; 64]>` (`:241-252`),
  `Argon2::new(Algorithm::Argon2id, Version::V0x13, …)` (`:246`)
- split (`:24-27`): first 32 bytes = verification hash, last 32 = **AES-256-GCM** key
  (`aes_gcm::Aes256Gcm`, `:40-41`) protecting the BIP39 12-word recovery phrase (`:11-15`)
- `MARKER_FILENAME = "password_marker.json"` (`:52`), `MARKER_VERSION = 2` (`:55`),
  `ENC_MAGIC = b"OSL-ENC1"` (`:59`), `PASSWORD_MIN_LEN = 6` (`:61`)
- stealth / burn / duress password hashes share the **same salt and params** as the main password
  (`:107-135`, `:121-128`)
- constant-time compare `ct_eq` (`:254-261`)

`crates/keystore/src/password.rs` is **legacy** (`:14-17`); it states outright (`:19-23`) that
password verification is "a UX gate, not part of identity-key derivation". Its Argon2id params:
production `m=65_536, t=3, p=1, out=32` (`:81-88`); `fast_for_tests` `m=8, t=1, p=1` documented
"**NOT secure**" (`:91-98`). `MIN_PASSWORD_LENGTH = 6` (`:62`).

**Reviewer note:** a 6-character minimum, with the password gating the recovery phrase that
regenerates the entire identity, is worth an explicit opinion from the firm.

### Zeroization sites

- `Sealer::unseal` returns `Zeroizing<Vec<u8>>` by contract (`sealer.rs:88`), implemented in
  `unseal_with_aead_key` (`:706`) — the comment at `:702-704` records a previously-broken
  `Zeroizing::new(())` that wiped nothing
- TPM unwrap buffers `Zeroizing` (`sealer.rs:527`, `:554`)
- `Identity`: `mlkem_secret_bytes: Zeroizing<…>` (`identity.rs:57`), `impl Drop` wiping
  `recovery_entropy` (`:109-115`), `impl ZeroizeOnDrop` (`:118`)
- `InnerIdentity` on-disk struct derives `Zeroize, ZeroizeOnDrop`
  (`crates/keystore/src/storage.rs:75`, rationale `:67-74`); `decode_array` returns `Zeroizing`
  (`:263-274`); compile-time `assert_zeroize_on_drop::<InnerIdentity>()` / `::<Identity>()`
  (`:345-347`)
- RN state export zeroized after seal (`wire_rn.rs:665`) and after import (`:776`)
- password verify candidate wiped (`password.rs:145`); `main_password::derive` returns `Zeroizing`
  (`:245-247`)
- page locking: `crates/keystore/src/sensitive_memory.rs` — `LockedSensitivePlaintext` (`:68`)
  holding `Zeroizing<Vec<u8>>` (`:70`), `lock_sensitive_plaintext` (`:88`, `:115`),
  `lock_sensitive_pages` (`:185`), `mlock` (`:348`) / `VirtualLock` (`:358`)

**Exercised** by the `keystore` suite that ran: `crates/keystore/tests/zeroization_test.rs:31`,
`:69`, `:99`, `:111` (all four passed).

## 8. Not determined / not verified

- TPM and OS-keyring sealers: **no runtime coverage** (their tests are `#[ignore]`d; this host is
  Linux).
- Whether `select_best_sealer()` is the only sealer factory on shipping paths.
- Whether `EphemeralProcessSealer` failure-on-restart is handled gracefully by every caller
  (by construction its key cannot survive a restart; the callers' handling was not traced).
- Whether an old v3 binary is *proven* refused end-to-end (fixture exists; `-p store` not run).
- Actual TPM RSA modulus size.
- No repo-wide DPAPI grep outside `crates/keystore`.
