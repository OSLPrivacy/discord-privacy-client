//! Phase 9-PEER-MAP-ENC regression tests.
//!
//! Locks down the two defenses against the bootstrap-time
//! plaintext-clobber bug:
//!
//! 1. `write_peer_map` refuses to overwrite an existing OSL-ENC1
//!    file with plaintext when no `file_storage_key` is in slot.
//!    Pre-fix, bootstrap's `verify_and_persist_peer_map_self_entry`
//!    fired before the password gate, found an empty in-memory map,
//!    "repaired" the self-entry, and persisted — clobbering the
//!    user's encrypted peer_map with a 1-entry plaintext stub on
//!    every launch.
//!
//! 2. `reload_encrypted_state_after_unlock` re-encrypts any
//!    plaintext peer_map.json once the key is in slot, so users
//!    already affected by the pre-fix corruption get their file
//!    upgraded on the next gate verify.

use ipc::main_password::{
    get_file_storage_key, has_enc_magic, maybe_encrypt, set_file_storage_key,
};
use ipc::peer_map::{legacy_entry, write_peer_map, PeerMap};
use ipc::state_reload::reload_encrypted_state_after_unlock;
use ipc::AppState;
use std::collections::HashMap;
use std::sync::Mutex;
use tempfile::tempdir;

static KEY_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn peer_map_writes_are_encrypted_when_key_present() {
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(Some([0x42u8; 32]));

    let dir = tempdir().unwrap();
    let path = dir.path().join("peer_map.json");
    let mut map: PeerMap = HashMap::new();
    map.insert("11111".to_string(), legacy_entry("henry"));
    write_peer_map(&path, &map).unwrap();

    let raw = std::fs::read(&path).unwrap();
    assert!(
        has_enc_magic(&raw),
        "write with key in slot must produce OSL-ENC1 envelope"
    );

    set_file_storage_key(None);
}

#[test]
fn peer_map_writes_are_plaintext_when_key_absent_and_no_existing_file() {
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(None);

    let dir = tempdir().unwrap();
    let path = dir.path().join("peer_map.json");
    let mut map: PeerMap = HashMap::new();
    map.insert("11111".to_string(), legacy_entry("henry"));
    write_peer_map(&path, &map).unwrap();

    let raw = std::fs::read(&path).unwrap();
    assert!(
        !has_enc_magic(&raw),
        "first-time write with no key in slot is plaintext (no encrypted file to protect)"
    );
}

/// The core regression: bootstrap's pre-gate verify_and_persist
/// would clobber an encrypted file with plaintext. The write_peer_map
/// guard now refuses that path with PermissionDenied so callers can
/// no-op (their tracing::warn-and-continue pattern survives).
#[test]
fn write_peer_map_refuses_to_clobber_encrypted_with_plaintext() {
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // Step 1: with key A, write the "real" encrypted peer_map.
    let key_a = [0xAAu8; 32];
    set_file_storage_key(Some(key_a));
    let dir = tempdir().unwrap();
    let path = dir.path().join("peer_map.json");
    let mut real_map: PeerMap = HashMap::new();
    real_map.insert("11111".to_string(), legacy_entry("henry"));
    real_map.insert("22222".to_string(), legacy_entry("alice"));
    write_peer_map(&path, &real_map).unwrap();
    let encrypted_blob = std::fs::read(&path).unwrap();
    assert!(has_enc_magic(&encrypted_blob));

    // Step 2: simulate bootstrap state — clear the key.
    set_file_storage_key(None);

    // Step 3: bootstrap's repair builds a 1-entry "self-only" map
    // and calls write_peer_map. With the new guard, this MUST be
    // refused so the encrypted file survives.
    let mut stub_map: PeerMap = HashMap::new();
    stub_map.insert("99999".to_string(), legacy_entry("self_user"));
    let result = write_peer_map(&path, &stub_map);
    assert!(
        result.is_err(),
        "write_peer_map with key absent + encrypted file present MUST refuse"
    );
    assert_eq!(
        result.err().unwrap().kind(),
        std::io::ErrorKind::PermissionDenied
    );

    // Step 4: the on-disk file is byte-for-byte unchanged.
    let after = std::fs::read(&path).unwrap();
    assert_eq!(after, encrypted_blob, "encrypted file MUST be preserved");
}

/// Defense-in-depth coverage: even when key absence is a legitimate
/// state (e.g. very first launch, no password ever set), the guard
/// only triggers when an encrypted file EXISTS on disk. Otherwise
/// plaintext writes proceed normally.
#[test]
fn write_peer_map_allows_plaintext_when_no_existing_file() {
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(None);

    let dir = tempdir().unwrap();
    let path = dir.path().join("peer_map.json");
    assert!(!path.exists());

    let mut map: PeerMap = HashMap::new();
    map.insert("11111".to_string(), legacy_entry("henry"));
    write_peer_map(&path, &map).expect("plaintext write to fresh path is allowed");
    assert!(path.exists());
}

/// Same guard semantics: an EXISTING plaintext file (e.g. on a fresh
/// install before the user has set a password) does NOT block
/// rewrites with the key still absent. The guard only fires on
/// encrypted-existing-plus-no-key.
#[test]
fn write_peer_map_allows_overwriting_existing_plaintext_when_key_absent() {
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(None);

    let dir = tempdir().unwrap();
    let path = dir.path().join("peer_map.json");
    std::fs::write(&path, b"{}").unwrap();

    let mut map: PeerMap = HashMap::new();
    map.insert("11111".to_string(), legacy_entry("henry"));
    write_peer_map(&path, &map).expect("plaintext-over-plaintext write is allowed");
}

/// Retroactive migration: a user on the pre-fix build has a
/// plaintext peer_map on disk despite a password being set. On
/// the next gate verify, the post-gate reload should detect the
/// missing OSL-ENC1 magic and re-write the file encrypted.
#[test]
fn reload_reencrypts_plaintext_peer_map_when_key_now_present() {
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let dir = tempdir().unwrap();
    let path = dir.path().join("peer_map.json");

    // Step 1: write a plaintext peer_map (simulates the pre-fix
    // bootstrap-clobbered state on the user's install).
    set_file_storage_key(None);
    let mut map: PeerMap = HashMap::new();
    map.insert("11111".to_string(), legacy_entry("henry"));
    map.insert("22222".to_string(), legacy_entry("alice"));
    write_peer_map(&path, &map).unwrap();
    let raw_plain = std::fs::read(&path).unwrap();
    assert!(
        !has_enc_magic(&raw_plain),
        "test setup precondition: file must start plaintext"
    );

    // Step 2: gate verify installs the file_storage_key. Then
    // reload_encrypted_state_after_unlock runs.
    set_file_storage_key(Some([0x77u8; 32]));
    let state = AppState::new();
    let report = reload_encrypted_state_after_unlock(&state, dir.path()).unwrap();

    // Step 3: report flags the re-encrypt.
    assert!(
        report.peer_map_reencrypted,
        "reload should have re-encrypted the plaintext peer_map"
    );

    // Step 4: file on disk now has OSL-ENC1 magic.
    let raw_after = std::fs::read(&path).unwrap();
    assert!(
        has_enc_magic(&raw_after),
        "post-reload peer_map.json must be encrypted; got {:?}",
        &raw_after[..8.min(raw_after.len())]
    );

    // Step 5: same peers preserved (decrypt + parse round-trip
    // checked via ipc::peer_map's loader).
    let reloaded = ipc::peer_map::load_peer_map_from_path(&path).unwrap();
    assert_eq!(reloaded.len(), 2);
    assert!(reloaded.contains_key("11111"));
    assert!(reloaded.contains_key("22222"));

    set_file_storage_key(None);
}

/// Subsequent reloads (file already encrypted) skip the re-encrypt
/// pass. Idempotency check.
#[test]
fn reload_skips_reencrypt_when_file_already_encrypted() {
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let dir = tempdir().unwrap();
    let path = dir.path().join("peer_map.json");
    set_file_storage_key(Some([0x55u8; 32]));
    let mut map: PeerMap = HashMap::new();
    map.insert("11111".to_string(), legacy_entry("henry"));
    write_peer_map(&path, &map).unwrap();
    assert!(has_enc_magic(&std::fs::read(&path).unwrap()));

    // Snapshot bytes to verify the file ISN'T rewritten by the reload.
    let pre = std::fs::read(&path).unwrap();

    let state = AppState::new();
    let report = reload_encrypted_state_after_unlock(&state, dir.path()).unwrap();
    assert!(
        !report.peer_map_reencrypted,
        "reload must NOT re-encrypt an already-encrypted file"
    );

    // Bytes unchanged.
    let post = std::fs::read(&path).unwrap();
    assert_eq!(pre, post);

    set_file_storage_key(None);
}

/// The maybe_encrypt round-trip + write_peer_map guard interact
/// safely under the normal post-gate path: with the key installed,
/// the guard never fires, and writes encrypt normally.
#[test]
fn key_present_writes_round_trip_correctly() {
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(Some([0x11u8; 32]));

    let dir = tempdir().unwrap();
    let path = dir.path().join("peer_map.json");

    // First write: file is created encrypted.
    let mut m1: PeerMap = HashMap::new();
    m1.insert("11111".to_string(), legacy_entry("henry"));
    write_peer_map(&path, &m1).unwrap();

    // Second write: file already encrypted, key still in slot — the
    // guard doesn't apply because key is Some. Write succeeds and
    // produces a different blob (fresh AEAD nonce).
    let mut m2: PeerMap = HashMap::new();
    m2.insert("11111".to_string(), legacy_entry("henry"));
    m2.insert("22222".to_string(), legacy_entry("alice"));
    write_peer_map(&path, &m2).unwrap();

    // Sanity: still encrypted, parses to two peers via maybe_encrypt
    // round-trip.
    let raw = std::fs::read(&path).unwrap();
    assert!(has_enc_magic(&raw));
    let reloaded = ipc::peer_map::load_peer_map_from_path(&path).unwrap();
    assert_eq!(reloaded.len(), 2);

    // Sanity: re-encrypting the same plaintext produces a different
    // ciphertext (nonce randomness).
    let plain_bytes = b"{}";
    let a = maybe_encrypt(plain_bytes).unwrap();
    let b = maybe_encrypt(plain_bytes).unwrap();
    assert_ne!(a, b, "AEAD nonce randomness expected");
    assert!(has_enc_magic(&a) && has_enc_magic(&b));
    assert_eq!(get_file_storage_key().is_some(), true);

    set_file_storage_key(None);
}
