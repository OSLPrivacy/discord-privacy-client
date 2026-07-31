#![cfg(feature = "core")]

//! Unit a5 (checklist A1 / A6 "no protected plaintext at rest"):
//! prove `identity.json` stays sealed via the `keystore::Sealer`
//! (TPM / keyring / process-ephemeral) layer independently of whether a
//! main password is set.
//!
//! Context. `ipc::main_password::maybe_encrypt` (main_password.rs
//! ~L943-948) is a documented no-op plaintext passthrough for
//! `peer_map.json` / `whitelist_state.json` / `burned_scopes.json`
//! when no main password key is in the process-global slot. The header
//! comment on that layer (main_password.rs ~L804-810) claims
//! `identity.json` is NOT covered by it and instead keeps its own
//! `keystore::Sealer` layer, which does not consult the main-password
//! key at all. This test exercises that claim end to end, in both
//! main-password states, so a future change that folds identity
//! storage into the same password-gated at-rest layer -- silently
//! reintroducing the A6 no-op-when-unset defect for identity material
//! -- fails loudly here instead of shipping quietly.
//!
//! No `#[ignore]` gating: `keystore::select_best_sealer()` always
//! returns a real (non-plaintext) sealer -- TPM is Windows-only and
//! skipped on other platforms, keyring falls back to an encrypted
//! process-ephemeral sealer when the OS keyring backend is unavailable
//! (e.g. no DBus/Secret Service in WSL) -- so every assertion below
//! runs on every platform this suite builds on.

use ipc::main_password;
use keystore::{generate_identity, load_identity, save_identity, select_best_sealer, METHOD_NOOP};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn isolated_dir(tag: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("osl-unit-a5-{tag}-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create isolated test dir");
    dir
}

/// Base64 strings that would appear verbatim in an on-disk identity
/// blob IF (and only if) the sealing layer were a plaintext passthrough
/// -- e.g. `NoOpSealer`, or a hypothetical route through
/// `main_password::maybe_encrypt` while no key sits in the slot. A real
/// sealer (TPM/keyring/ephemeral) AEAD-wraps the inner JSON, so none of
/// these substrings may appear in the sealed bytes.
fn secret_b64_needles(identity: &keystore::Identity) -> Vec<String> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    vec![
        STANDARD.encode(identity.x25519_secret.as_bytes()),
        STANDARD.encode(identity.ed25519_secret.as_bytes()),
        STANDARD.encode(identity.mlkem_secret_bytes()),
    ]
}

fn assert_sealed_on_disk(path: &Path, identity: &keystore::Identity) {
    let raw = std::fs::read(path).expect("sealed identity file exists on disk");
    let raw_str = String::from_utf8_lossy(&raw);

    // 1. The wrapper's declared method must be a real sealer, never the
    //    plaintext NoOp tag, and must not carry the INSECURE banner
    //    that only a plaintext sealer emits.
    let on_disk: keystore::IdentityOnDisk =
        serde_json::from_slice(&raw).expect("identity.json is well-formed JSON");
    assert_ne!(
        on_disk.method, METHOD_NOOP,
        "identity.json was sealed with the plaintext NoOp method -- \
         DEFECT: identity secret material is at rest unsealed"
    );
    assert!(
        on_disk.insecure_banner.is_none(),
        "a real sealer must never emit the INSECURE banner (method={})",
        on_disk.method
    );

    // 2. None of the raw secret key material may appear verbatim
    //    anywhere in the on-disk bytes.
    for needle in secret_b64_needles(identity) {
        assert!(
            !raw_str.contains(&needle),
            "secret key material found verbatim in on-disk identity.json \
             (method={}) -- identity is NOT sealed at rest",
            on_disk.method
        );
    }

    // 3. identity.json must not be wrapped by the separate main-password
    //    file-storage-key AEAD layer (the `OSL-ENC1` magic used for
    //    peer_map.json / whitelist_state.json / burned_scopes.json).
    //    That is the layer with the documented no-op-when-unset defect
    //    (A6); identity.json carrying that magic would mean the two
    //    layers have been conflated.
    assert!(
        !main_password::has_enc_magic(&raw),
        "identity.json unexpectedly carries the main-password at-rest \
         (OSL-ENC1) magic -- it is supposed to have its own independent \
         keystore::Sealer layer"
    );
}

/// Core property, main password NOT set: identity.json still saves,
/// still seals (no plaintext secrets, no NoOp method, no INSECURE
/// banner), and still loads back correctly.
#[test]
fn identity_at_rest_is_sealed_with_no_main_password_set() {
    main_password::set_file_storage_key(None);
    let dir = isolated_dir("no-password");
    let path = dir.join("identity.json");

    let original = generate_identity("unit-a5-no-password".to_string());
    let sealer = select_best_sealer();
    save_identity(&path, &original, sealer.as_ref())
        .expect("identity must save even with no main password set");

    assert_sealed_on_disk(&path, &original);

    let loaded =
        load_identity(&path, sealer.as_ref()).expect("sealed identity must load back correctly");
    assert_eq!(
        loaded.x25519_secret.as_bytes(),
        original.x25519_secret.as_bytes()
    );
    assert_eq!(loaded.mlkem_secret_bytes(), original.mlkem_secret_bytes());

    let _ = std::fs::remove_dir_all(&dir);
}

/// Same property, main password SET: a `file_storage_key` populated in
/// the process-global slot (simulating an unlocked main password) must
/// make no difference to identity.json's sealing.
#[test]
fn identity_at_rest_is_sealed_with_main_password_set() {
    let fake_key = [0x42u8; 32];
    main_password::set_file_storage_key(Some(fake_key));
    let dir = isolated_dir("with-password");
    let path = dir.join("identity.json");

    let original = generate_identity("unit-a5-with-password".to_string());
    let sealer = select_best_sealer();
    save_identity(&path, &original, sealer.as_ref())
        .expect("identity must save with a main password set");

    assert_sealed_on_disk(&path, &original);

    let loaded =
        load_identity(&path, sealer.as_ref()).expect("sealed identity must load back correctly");
    assert_eq!(
        loaded.x25519_secret.as_bytes(),
        original.x25519_secret.as_bytes()
    );
    assert_eq!(loaded.mlkem_secret_bytes(), original.mlkem_secret_bytes());

    main_password::set_file_storage_key(None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The strongest form of the claim: identity.json's seal/unseal does
/// not consult the main-password `file_storage_key` at all. Saving
/// under one main-password state and loading under the OPPOSITE state
/// must still work. If a future change routes identity storage through
/// `main_password::maybe_encrypt` / `maybe_decrypt` (reusing the layer
/// that has the A6 no-op-when-unset defect), this is exactly the test
/// that starts failing: `load_identity` would either error ("no key in
/// slot") after the password state changes, or -- matching the A6
/// shape exactly -- a save performed with no key would silently
/// downgrade to plaintext that a later `assert_sealed_on_disk` catches.
#[test]
fn identity_at_rest_survives_main_password_state_changes_across_save_and_load() {
    let dir = isolated_dir("toggle");
    let sealer = select_best_sealer();

    // Save WITH a password key present...
    main_password::set_file_storage_key(Some([0x11u8; 32]));
    let path_a = dir.join("saved-with-password.json");
    let id_a = generate_identity("unit-a5-toggle-a".to_string());
    save_identity(&path_a, &id_a, sealer.as_ref()).expect("save while password key is set");
    assert_sealed_on_disk(&path_a, &id_a);

    // ...then clear the password key entirely before loading.
    main_password::set_file_storage_key(None);
    let loaded_a = load_identity(&path_a, sealer.as_ref()).expect(
        "identity saved while a main-password key existed must still load \
         after that key is cleared -- sealing must be independent of \
         main-password state",
    );
    assert_eq!(
        loaded_a.x25519_secret.as_bytes(),
        id_a.x25519_secret.as_bytes()
    );

    // Save WITHOUT a password key...
    let path_b = dir.join("saved-without-password.json");
    let id_b = generate_identity("unit-a5-toggle-b".to_string());
    save_identity(&path_b, &id_b, sealer.as_ref()).expect("save with no password key set");
    assert_sealed_on_disk(&path_b, &id_b);

    // ...then set a (different) password key before loading.
    main_password::set_file_storage_key(Some([0x22u8; 32]));
    let loaded_b = load_identity(&path_b, sealer.as_ref()).expect(
        "identity saved with no main-password key must still load after a \
         main-password key is later set",
    );
    assert_eq!(
        loaded_b.x25519_secret.as_bytes(),
        id_b.x25519_secret.as_bytes()
    );

    main_password::set_file_storage_key(None);
    let _ = std::fs::remove_dir_all(&dir);
}
