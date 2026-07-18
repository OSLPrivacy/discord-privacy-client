//! Phase 9-B1 Task 1: app_preferences.json round-trip + Tauri-shape tests.
//!
//! 9-MODE1-FIX dropped the `always_preview_mode1` + `mode1_confirmed_scopes`
//! fields. Tests retain coverage for the surviving `stego_mode` selector,
//! plus the round-trip + Tauri-DTO shape.

use ipc::app_preferences::{
    load_app_preferences, write_app_preferences, AppPreferences, StegoMode, APP_PREFERENCES_VERSION,
};
use ipc::commands::{cmd_osl_get_app_preferences, cmd_osl_set_app_preferences, AppPreferencesDto};
use ipc::AppState;
use std::sync::Mutex;
use tempfile::tempdir;

// The global `file_storage_key` is shared by every test in this
// binary. Two tests mutating it concurrently can produce a write
// stamped with one key but a load expecting another, so serialize
// the password-touching tests against this mutex.
static KEY_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn app_preferences_default_is_mode0() {
    let p = AppPreferences::default();
    assert_eq!(p.stego_mode, StegoMode::Mode0);
}

#[test]
fn app_preferences_load_missing_file_returns_default() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("does-not-exist.json");
    let p = load_app_preferences(&path);
    assert_eq!(p, AppPreferences::default());
}

/// 9-MODE1-FIX: legacy app_preferences.json files written before the
/// removal still carry `always_preview_mode1` and `mode1_confirmed_scopes`.
/// serde's default behaviour silently drops unknown fields, so loading
/// a legacy file returns a default-otherwise AppPreferences with the
/// new shape.
#[test]
fn app_preferences_legacy_fields_are_ignored() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("app_preferences_legacy.json");
    let legacy = r#"{
      "version": 1,
      "stego_mode": "mode1",
      "always_preview_mode1": true,
      "mode1_confirmed_scopes": ["gc:1234", "dm:abc:def"]
    }"#;
    std::fs::write(&path, legacy).unwrap();
    let p = load_app_preferences(&path);
    assert_eq!(p.stego_mode, StegoMode::Mode1);
    assert_eq!(p.version, 1);
}

// The encrypted-roundtrip and plain-roundtrip cases share a global
// (file_storage_key) so we sequence them inside one #[test] rather
// than trusting cargo-test's default parallel ordering. Same pattern
// as `phase_a3_sender_key_state_file.rs`.
#[test]
fn app_preferences_roundtrip_plain_then_encrypted() {
    use ipc::main_password::set_file_storage_key;
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(None);

    // ---- Plain ----
    let dir = tempdir().unwrap();
    let plain_path = dir.path().join("app_preferences_plain.json");
    let prefs = AppPreferences {
        version: APP_PREFERENCES_VERSION,
        stego_mode: StegoMode::Mode1,
        ..Default::default()
    };
    write_app_preferences(&plain_path, &prefs).unwrap();
    let plain_raw = std::fs::read(&plain_path).unwrap();
    assert!(
        !plain_raw.starts_with(b"OSL-ENC1"),
        "no-password write must be plain JSON"
    );
    assert_eq!(load_app_preferences(&plain_path), prefs);

    // ---- Encrypted ----
    let key = [0x33u8; 32];
    set_file_storage_key(Some(key));
    let enc_path = dir.path().join("app_preferences_enc.json");
    let enc_prefs = AppPreferences {
        version: APP_PREFERENCES_VERSION,
        stego_mode: StegoMode::Mode1,
        ..Default::default()
    };
    write_app_preferences(&enc_path, &enc_prefs).unwrap();
    let enc_raw = std::fs::read(&enc_path).unwrap();
    assert!(
        enc_raw.starts_with(b"OSL-ENC1"),
        "encrypted write must stamp OSL-ENC1"
    );
    assert_eq!(load_app_preferences(&enc_path), enc_prefs);

    set_file_storage_key(None);
}

#[test]
fn tauri_get_then_set_writes_through_to_disk() {
    use ipc::main_password::set_file_storage_key;
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(None);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    let path = dir.path().join("app_preferences.json");

    // Default state → Mode 0.
    let initial = cmd_osl_get_app_preferences(&state).unwrap();
    assert_eq!(initial.stego_mode, StegoMode::Mode0);

    // Set Mode 1; verify state mutated AND file is on disk.
    let dto = AppPreferencesDto {
        stego_mode: StegoMode::Mode1,
    };
    cmd_osl_set_app_preferences(&state, dto, Some(dir.path().to_path_buf())).unwrap();

    let back = cmd_osl_get_app_preferences(&state).unwrap();
    assert_eq!(back.stego_mode, StegoMode::Mode1);

    let on_disk = load_app_preferences(&path);
    assert_eq!(on_disk.stego_mode, StegoMode::Mode1);
}
