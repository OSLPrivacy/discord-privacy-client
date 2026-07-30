//! Unit a7 (checklist A1 · Local OSL identity and recovery, item 3/6):
//! prove every [`Sealer`] variant round-trips an identity through
//! [`save_identity`] / [`load_identity`] byte-identically, and that the
//! identity's recovery entropy reconstructs the SAME keys.
//!
//! The recovery *phrase* itself is a bip39 encoding of `Identity::recovery_entropy`
//! (see `crates/ipc/src/commands.rs::cmd_osl_view_identity_recovery_phrase` /
//! `verify_recovery_phrase`) — bip39 is not a `keystore` dependency, and this
//! crate does not carry the word-list codec. What IS this crate's job, and
//! what the recovery phrase ultimately depends on, is `identity_from_entropy`:
//! that same entropy, fed back in, must deterministically rebuild the exact
//! same X25519 / Ed25519 / ML-KEM-768 keypairs. That is what's asserted below
//! (`identity.rs::same_entropy_reproduces_identical_keys` already covers the
//! entropy->keys function in isolation; this file adds the save/load hop in
//! front of it).
//!
//! ## What actually ran vs what is gated out
//!
//! - [`NoOpSealer`] and [`MemorySealer`]: run unconditionally, everywhere
//!   (including this WSL box). These are real proofs.
//! - [`KeyringSealer`]: on Linux without a linux-native/secret-service keyring
//!   feature compiled in (this crate only enables `windows-native`), the
//!   `keyring` crate falls back to its in-memory `mock` backend, whose
//!   `MockCredentialBuilder::build` hands each `Entry::new` call an
//!   independent, unshared `MockCredential` (see keyring 3.6.3
//!   `src/mock.rs`). `KeyringSealer::new()`'s own persistence self-probe
//!   (a second fresh `Entry::new` read-back) therefore reliably fails on
//!   this box — there is no real keyring to prove anything against here.
//!   The test below is `#[ignore]`-gated behind `OSL_TEST_KEYRING_SEALER=1`
//!   and must be run on a machine with a real persistent backend (Windows
//!   Credential Manager, macOS Keychain, or Linux Secret Service via DBus)
//!   to count as proof. It did NOT run in this environment.
//! - [`TpmSealer`]: Windows-only by construction (`#[cfg(not(windows))]`
//!   stub always returns `SealerError::Tpm`); needs a real TPM behind the
//!   Microsoft Platform Crypto Provider. `#[cfg(windows)]` + `#[ignore]`-gated
//!   behind `OSL_TEST_TPM_SEALER=1`. It did NOT run in this environment
//!   (this box is Linux/WSL) and could not even compile as a real test here.

use keystore::{
    generate_identity, identity_from_entropy, load_identity, save_identity, Error, MemorySealer,
    NoOpSealer, Sealer,
};
use tempfile::TempDir;

#[cfg(windows)]
use keystore::KeyringSealer;
#[cfg(windows)]
use keystore::TpmSealer;

/// Save a freshly generated identity to disk under `writer`, load it back
/// under `reader` (same underlying key material: either literally the same
/// sealer instance, or a fresh construction reading the same persisted
/// key), and assert:
///
/// 1. Every field on the loaded `Identity` — including `recovery_entropy`
///    — is byte-identical to what was saved.
/// 2. Feeding the recovered `recovery_entropy` back through
///    `identity_from_entropy` reconstructs byte-identical secret AND
///    public key material. This is the property a real device-transfer
///    recovery-phrase entry depends on: the phrase only ever carries the
///    entropy, never the keys directly.
fn assert_round_trip(label: &str, writer: &dyn Sealer, reader: &dyn Sealer) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let original = generate_identity(format!("{label}-user"));

    save_identity(&path, &original, writer)
        .unwrap_or_else(|e| panic!("{label}: save_identity failed: {e}"));
    let loaded = load_identity(&path, reader)
        .unwrap_or_else(|e| panic!("{label}: load_identity failed: {e}"));

    assert_eq!(loaded.user_id, original.user_id, "{label}: user_id");
    assert_eq!(
        loaded.x25519_secret.as_bytes(),
        original.x25519_secret.as_bytes(),
        "{label}: x25519 secret"
    );
    assert_eq!(
        loaded.x25519_public.as_bytes(),
        original.x25519_public.as_bytes(),
        "{label}: x25519 public"
    );
    assert_eq!(
        loaded.ed25519_secret.as_bytes(),
        original.ed25519_secret.as_bytes(),
        "{label}: ed25519 secret"
    );
    assert_eq!(
        loaded.ed25519_public.as_bytes(),
        original.ed25519_public.as_bytes(),
        "{label}: ed25519 public"
    );
    assert_eq!(
        loaded.mlkem_secret_bytes(),
        original.mlkem_secret_bytes(),
        "{label}: mlkem secret"
    );
    assert_eq!(
        loaded.mlkem_public_bytes, original.mlkem_public_bytes,
        "{label}: mlkem public"
    );
    assert_eq!(
        loaded.recovery_entropy, original.recovery_entropy,
        "{label}: recovery entropy (what the 12-word phrase actually encodes)"
    );

    let entropy = original
        .recovery_entropy
        .unwrap_or_else(|| panic!("{label}: generate_identity always seeds recovery entropy"));
    let rederived = identity_from_entropy(entropy, loaded.user_id.clone());
    assert_eq!(
        rederived.x25519_secret.as_bytes(),
        loaded.x25519_secret.as_bytes(),
        "{label}: phrase-rederived x25519 secret must match the saved/loaded identity"
    );
    assert_eq!(
        rederived.x25519_public.as_bytes(),
        loaded.x25519_public.as_bytes(),
        "{label}: phrase-rederived x25519 public must match the saved/loaded identity"
    );
    assert_eq!(
        rederived.ed25519_secret.as_bytes(),
        loaded.ed25519_secret.as_bytes(),
        "{label}: phrase-rederived ed25519 secret must match the saved/loaded identity"
    );
    assert_eq!(
        rederived.ed25519_public.as_bytes(),
        loaded.ed25519_public.as_bytes(),
        "{label}: phrase-rederived ed25519 public must match the saved/loaded identity"
    );
    assert_eq!(
        rederived.mlkem_secret_bytes(),
        loaded.mlkem_secret_bytes(),
        "{label}: phrase-rederived mlkem secret must match the saved/loaded identity"
    );
    assert_eq!(
        rederived.mlkem_public_bytes, loaded.mlkem_public_bytes,
        "{label}: phrase-rederived mlkem public must match the saved/loaded identity"
    );
}

// ---- table: one case per Sealer variant ----

#[test]
fn noop_sealer_round_trips_through_save_load() {
    let sealer = NoOpSealer::new();
    assert_round_trip("noop", &sealer, &sealer);
}

#[test]
fn memory_sealer_round_trips_through_save_load() {
    let sealer = MemorySealer::new();
    assert_round_trip("memory", &sealer, &sealer);
}

#[test]
#[ignore = "needs a real, persistent OS keyring backend (Windows Credential \
            Manager / macOS Keychain / Linux Secret Service over DBus). On \
            this crate's Linux build only `windows-native` is compiled in, \
            so `keyring` falls back to its unshared in-memory mock and \
            KeyringSealer::new()'s own persistence self-probe reliably fails \
            -- there's nothing real to prove here. Run with \
            `--ignored` and OSL_TEST_KEYRING_SEALER=1 on a machine with a \
            real backend."]
fn keyring_sealer_round_trips_through_save_load() {
    if std::env::var_os("OSL_TEST_KEYRING_SEALER").is_none() {
        panic!(
            "set OSL_TEST_KEYRING_SEALER=1 and run with --ignored on a machine \
             with a real persistent OS keyring backend to execute this case"
        );
    }
    #[cfg(windows)]
    {
        let writer = KeyringSealer::new().expect("Windows Credential Manager available");
        let reader = KeyringSealer::new().expect("fresh credential entry reads the same key");
        assert_round_trip("keyring", &writer, &reader);
    }
    #[cfg(not(windows))]
    {
        panic!(
            "this crate only compiles a persistent keyring backend for \
             `windows-native`; no non-Windows backend feature is enabled, so \
             there is no real keyring to test against on this platform"
        );
    }
}

#[cfg(windows)]
#[test]
#[ignore = "needs a real Windows TPM behind the Microsoft Platform Crypto \
            Provider, unavailable in WSL/CI. Run with --ignored and \
            OSL_TEST_TPM_SEALER=1 on a machine that has one."]
fn tpm_sealer_round_trips_through_save_load() {
    if std::env::var_os("OSL_TEST_TPM_SEALER").is_none() {
        panic!(
            "set OSL_TEST_TPM_SEALER=1 and run with --ignored on a machine \
             with a real TPM to execute this case"
        );
    }
    let writer = TpmSealer::new().expect("TPM / Platform Crypto Provider available");
    let reader = TpmSealer::new().expect("second TPM handle opens the same persisted key");
    assert_round_trip("tpm", &writer, &reader);
}

// ---- negative cases: wrong / absent sealing key must be refused ----

#[test]
fn wrong_sealer_key_is_refused_not_silently_different() {
    // Two independent MemorySealer instances each hold their own random
    // AEAD key -- this models "wrong key": the reader is a real, working
    // sealer, but not the one the blob was actually sealed under.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let writer = MemorySealer::new();
    let reader = MemorySealer::new();
    let original = generate_identity("victim-wrong-key".to_string());
    save_identity(&path, &original, &writer).unwrap();

    let result = load_identity(&path, &reader);

    // `Sealer::unseal` is AEAD-authenticated (see `sealer.rs`'s
    // `unseal_with_aead_key`), so decrypting under the wrong key cannot
    // produce a plausible-looking-but-different plaintext: it can only
    // fail the authentication tag check. `load_identity`'s Result<Identity>
    // return type also makes "silently returns a different identity"
    // structurally impossible to observe as an `Ok` -- the only way to be
    // sure that failure mode is closed is to assert this really is `Err`.
    match result {
        Err(Error::Sealer(_)) => {}
        Err(other) => panic!("expected a sealer/auth error, got a different error: {other}"),
        Ok(wrong) => panic!(
            "load_identity MUST refuse a wrong-key blob, not silently decode a \
             different identity (user_id={:?})",
            wrong.user_id
        ),
    }
}

#[test]
fn absent_matching_sealer_is_refused_via_method_mismatch() {
    // "Absent sealing key" modeled as: the reader has no key context that
    // could ever match at all -- a completely different sealer method
    // (NoOp has no key; Memory has a real key the NoOp writer never used).
    // `load_identity` must reject this loudly and distinctly, not attempt
    // to unseal with the wrong strategy and hand back garbage.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let writer = NoOpSealer::new();
    let reader = MemorySealer::new();
    let original = generate_identity("victim-absent-key".to_string());
    save_identity(&path, &original, &writer).unwrap();

    let result = load_identity(&path, &reader);
    match result {
        Err(Error::BlobMethodMismatch { got, expected }) => {
            assert_eq!(got, keystore::NoOpSealer::new().method_label());
            assert_eq!(expected, reader.method_label());
        }
        Err(other) => panic!("expected BlobMethodMismatch, got a different error: {other}"),
        Ok(wrong) => panic!(
            "load_identity MUST refuse when no matching sealing key/method exists, \
             not silently decode a different identity (user_id={:?})",
            wrong.user_id
        ),
    }
}

#[test]
fn tampered_sealed_blob_is_refused() {
    // Bit-flip inside the sealed payload after a legitimate save: this is
    // the "absent key" case from the ciphertext's point of view -- whatever
    // key would make this blob authenticate no longer exists, because the
    // blob itself was corrupted. Must be refused, never silently accepted
    // as a differently-keyed identity.
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let sealer = MemorySealer::new();
    let original = generate_identity("victim-tampered".to_string());
    save_identity(&path, &original, &sealer).unwrap();

    let raw = std::fs::read_to_string(&path).unwrap();
    let on_disk: keystore::IdentityOnDisk = serde_json::from_str(&raw).unwrap();
    let mut sealed = STANDARD.decode(&on_disk.sealed_b64).unwrap();
    let last = sealed.len() - 1;
    sealed[last] ^= 0x01;
    let tampered_b64 = STANDARD.encode(&sealed);
    let mutated = raw.replace(&on_disk.sealed_b64, &tampered_b64);
    std::fs::write(&path, mutated).unwrap();

    let result = load_identity(&path, &sealer);
    match result {
        Err(Error::Sealer(_)) => {}
        Err(other) => panic!("expected a sealer/auth error, got a different error: {other}"),
        Ok(wrong) => panic!(
            "tampered sealed blob MUST be refused, not silently decoded into an \
             identity (user_id={:?})",
            wrong.user_id
        ),
    }
}
