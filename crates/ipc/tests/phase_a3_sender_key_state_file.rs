//! Phase 9-A3 Task 2: sender_key_state.json round-trip tests.

use crypto::sender_keys::{SenderKeyState, SenderKeyStateOnDisk};
use ipc::sender_key_state::{load_sender_key_state, write_sender_key_state, SenderKeyStateFile};
use std::fs;
use tempfile::tempdir;

fn populated_state() -> SenderKeyStateOnDisk {
    let mut s = SenderKeyState::new();
    s.install_sender().unwrap();
    let chain_id = s.sender_chain().unwrap().current_chain_id();
    let root = s.sender_chain().unwrap().rotation_root_bytes();
    s.install_receiver(b"alice".to_vec(), chain_id, &root)
        .unwrap();
    s.install_receiver(b"bob".to_vec(), chain_id, &root)
        .unwrap();
    SenderKeyStateOnDisk::from(&s)
}

// The encrypted-roundtrip and plain-roundtrip cases share a global
// (file_storage_key) so we sequence them inside one #[test] rather
// than trusting cargo-test's default parallel ordering. The test
// asserts both paths and the transition between them.
#[test]
fn sender_key_state_file_roundtrip_plain_then_encrypted() {
    use ipc::main_password::set_file_storage_key;

    // Ensure no leftover key from another test in this binary.
    set_file_storage_key(None);

    // ---- Plain (no password) path ----
    let dir = tempdir().unwrap();
    let plain_path = dir.path().join("sender_key_state_plain.json");
    let mut file = SenderKeyStateFile::default();
    file.version = 1;
    file.states.insert("gc:1234".to_string(), populated_state());
    file.states
        .insert("server_channel:1:2".to_string(), populated_state());
    write_sender_key_state(&plain_path, &file).expect("write plain");
    let plain_raw = fs::read(&plain_path).unwrap();
    assert!(
        !plain_raw.starts_with(b"OSL-ENC1"),
        "no-password write must be plain JSON"
    );
    let back = load_sender_key_state(&plain_path);
    assert_eq!(back, file);

    // ---- Encrypted path ----
    let key = [0x42u8; 32];
    set_file_storage_key(Some(key));
    let enc_path = dir.path().join("sender_key_state_enc.json");
    let mut enc_file = SenderKeyStateFile::default();
    enc_file.version = 1;
    enc_file
        .states
        .insert("gc:encrypted".to_string(), populated_state());
    write_sender_key_state(&enc_path, &enc_file).expect("write encrypted");
    let enc_raw = fs::read(&enc_path).unwrap();
    assert!(
        enc_raw.starts_with(b"OSL-ENC1"),
        "encrypted write must stamp OSL-ENC1 magic"
    );
    let back_enc = load_sender_key_state(&enc_path);
    assert_eq!(back_enc, enc_file);

    set_file_storage_key(None);
}

#[test]
fn sender_key_state_file_loads_legacy_empty_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sender_key_state.json");
    // Pre-A3 deployments have no file at all. Loader returns empty
    // default, no error.
    let back = load_sender_key_state(&path);
    assert!(back.states.is_empty());
    assert_eq!(back.version, 0);
}

#[test]
fn sender_key_state_file_load_returns_empty_when_missing() {
    // Same case, distinct test name per spec.
    let dir = tempdir().unwrap();
    let path = dir.path().join("does-not-exist.json");
    let back = load_sender_key_state(&path);
    assert!(back.states.is_empty());
}
