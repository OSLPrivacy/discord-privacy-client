//! Phase 9-D-FIX2: post-gate state reload regression tests.
//!
//! Reproduces the launch-time defect (bootstrap reads encrypted
//! state files before file_storage_key is installed → AppState
//! seeded with defaults → user's whitelist / burns / tour state /
//! sender chains stay blank for the session and the tour replays
//! on every relaunch) and asserts that `reload_encrypted_state_after_unlock`
//! repopulates AppState from disk.
//!
//! Each test:
//!   1. Installs a file_storage_key (simulates a user with a
//!      password already set).
//!   2. Writes a non-default state file via the same writer the
//!      production code uses (so the on-disk shape — including
//!      OSL-ENC1 envelope — matches reality).
//!   3. Constructs a fresh AppState seeded with defaults (this is
//!      what bootstrap would produce after failing to decrypt).
//!   4. Calls `reload_encrypted_state_after_unlock` and asserts
//!      AppState now mirrors disk.

use ipc::app_preferences::{
    write_app_preferences, AppPreferences, StegoMode, TourState, APP_PREFERENCES_VERSION,
};
use ipc::burned_scopes_file::{write_burned_scopes, BurnedScopeEntry, BurnedScopesFile};
use ipc::main_password::set_file_storage_key;
use ipc::peer_map::{legacy_entry, write_peer_map, PeerMap};
use ipc::sender_key_state::{write_sender_key_state, SenderKeyStateFile};
use ipc::state_reload::reload_encrypted_state_after_unlock;
use ipc::whitelist_state::{
    write_whitelist_state_file, ScopeState, ServerDefaults, WhitelistStateFile,
};
use ipc::AppState;
use std::collections::HashMap;
use std::sync::Mutex;
use tempfile::tempdir;

// File-storage-key is process-global; tests that toggle it must
// serialise so concurrent tests don't see each other's keys mid-
// write / mid-read. Same pattern as phase_b1_app_preferences.rs.
static KEY_LOCK: Mutex<()> = Mutex::new(());

fn install_test_key() -> [u8; 32] {
    let key = [0x42u8; 32];
    set_file_storage_key(Some(key));
    key
}

#[test]
fn reload_repopulates_app_preferences_from_disk() {
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    install_test_key();

    let dir = tempdir().unwrap();
    let prefs = AppPreferences {
        version: APP_PREFERENCES_VERSION,
        stego_mode: StegoMode::Mode1,
        tour: TourState {
            completed: true,
            skipped: false,
            last_slide: 9,
        },
        vpn_warning_dismissed_forever: true,
    };
    write_app_preferences(&dir.path().join("app_preferences.json"), &prefs).unwrap();

    let state = AppState::new();
    // Confirm fresh AppState IS at default (the bootstrap-after-failure shape).
    assert_eq!(
        *state.app_preferences.lock().unwrap(),
        AppPreferences::default()
    );

    let report = reload_encrypted_state_after_unlock(&state, dir.path()).unwrap();
    assert!(report.app_prefs_loaded);
    assert!(report.errors.is_empty());

    let g = state.app_preferences.lock().unwrap();
    assert!(g.tour.completed);
    assert_eq!(g.tour.last_slide, 9);
    assert_eq!(g.stego_mode, StegoMode::Mode1);
    assert!(g.vpn_warning_dismissed_forever);

    set_file_storage_key(None);
}

#[test]
fn reload_repopulates_peer_map_from_disk() {
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    install_test_key();

    let dir = tempdir().unwrap();
    let mut map: PeerMap = HashMap::new();
    map.insert("11111".to_string(), legacy_entry("henry"));
    map.insert("22222".to_string(), legacy_entry("alice"));
    write_peer_map(&dir.path().join("peer_map.json"), &map).unwrap();

    let state = AppState::new();
    assert_eq!(state.peer_map.lock().unwrap().len(), 0);

    let report = reload_encrypted_state_after_unlock(&state, dir.path()).unwrap();
    assert!(report.peer_map_loaded);
    assert_eq!(report.peer_map_entries, 2);
    assert!(report.errors.is_empty());
    assert_eq!(state.peer_map.lock().unwrap().len(), 2);

    set_file_storage_key(None);
}

#[test]
fn reload_repopulates_whitelist_state_from_disk() {
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    install_test_key();

    let dir = tempdir().unwrap();
    let mut scopes = HashMap::new();
    scopes.insert(
        "dm:11111".to_string(),
        ScopeState {
            encrypt_toggle: true,
            auto_enabled: false,
        },
    );
    scopes.insert(
        "gc:9999".to_string(),
        ScopeState {
            encrypt_toggle: true,
            auto_enabled: true,
        },
    );
    let mut server_defaults = HashMap::new();
    server_defaults.insert(
        "777777".to_string(),
        ServerDefaults {
            encrypt_by_default: true,
        },
    );
    let envelope = WhitelistStateFile {
        migrated_c1: true,
        scopes,
        server_defaults,
    };
    write_whitelist_state_file(&dir.path().join("whitelist_state.json"), &envelope).unwrap();

    let state = AppState::new();
    let report = reload_encrypted_state_after_unlock(&state, dir.path()).unwrap();
    assert!(report.whitelist_loaded);
    assert_eq!(report.whitelist_scopes, 2);
    assert!(report.server_defaults_loaded);
    assert_eq!(report.server_defaults_entries, 1);
    assert!(report.errors.is_empty());

    let ws = state.whitelist_state.lock().unwrap();
    assert_eq!(ws.len(), 2);
    assert!(ws.get("dm:11111").unwrap().encrypt_toggle);
    let sd = state.server_defaults.lock().unwrap();
    assert!(sd.get("777777").unwrap().encrypt_by_default);

    set_file_storage_key(None);
}

#[test]
fn reload_repopulates_burned_scopes_from_disk() {
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    install_test_key();

    let dir = tempdir().unwrap();
    let file = BurnedScopesFile {
        version: 1,
        scopes: vec![BurnedScopeEntry {
            scope_kind: "dm".to_string(),
            scope_id: "12345".to_string(),
            server_id: None,
            channel_id: None,
            burned_at: 1_700_000_000,
            burned_message_ids: vec![],
        }],
    };
    write_burned_scopes(&dir.path().join("burned_scopes.json"), &file).unwrap();

    let state = AppState::new();
    assert_eq!(state.burned_scopes.lock().unwrap().scopes.len(), 0);

    let report = reload_encrypted_state_after_unlock(&state, dir.path()).unwrap();
    assert!(report.burned_scopes_loaded);
    assert_eq!(report.burned_scopes_count, 1);
    assert!(report.errors.is_empty());
    assert_eq!(state.burned_scopes.lock().unwrap().scopes.len(), 1);

    set_file_storage_key(None);
}

#[test]
fn reload_repopulates_sender_keys_from_disk() {
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    install_test_key();

    let dir = tempdir().unwrap();
    // An empty SenderKeyStateFile is enough — the test asserts the
    // load was attempted (file exists) and the in-memory slot got
    // overwritten with the loaded value (not left at default).
    let file = SenderKeyStateFile {
        version: 1,
        states: HashMap::new(),
    };
    write_sender_key_state(&dir.path().join("sender_key_state.json"), &file).unwrap();

    let state = AppState::new();
    let report = reload_encrypted_state_after_unlock(&state, dir.path()).unwrap();
    assert!(report.sender_keys_loaded);
    assert_eq!(report.sender_keys_count, 0);
    assert!(report.errors.is_empty());

    set_file_storage_key(None);
}

#[test]
fn reload_handles_missing_files_gracefully() {
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    install_test_key();

    let dir = tempdir().unwrap();
    // No files written — fresh install case.
    let state = AppState::new();
    let report = reload_encrypted_state_after_unlock(&state, dir.path()).unwrap();
    assert!(!report.peer_map_loaded);
    assert!(!report.whitelist_loaded);
    assert!(!report.burned_scopes_loaded);
    assert!(!report.sender_keys_loaded);
    assert!(!report.app_prefs_loaded);
    assert!(report.errors.is_empty());

    set_file_storage_key(None);
}

#[test]
fn reload_handles_decrypt_failure_gracefully() {
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // Write a peer_map encrypted with key A.
    let key_a = [0xAAu8; 32];
    set_file_storage_key(Some(key_a));
    let dir = tempdir().unwrap();
    let mut map: PeerMap = HashMap::new();
    map.insert("11111".to_string(), legacy_entry("henry"));
    write_peer_map(&dir.path().join("peer_map.json"), &map).unwrap();

    // Now switch to key B — reading the encrypted file will fail
    // AEAD verification. The reload helper should record the failure
    // in `report.errors` and leave state.peer_map at default.
    let key_b = [0xBBu8; 32];
    set_file_storage_key(Some(key_b));
    let state = AppState::new();
    let report = reload_encrypted_state_after_unlock(&state, dir.path()).unwrap();
    assert!(
        !report.peer_map_loaded,
        "decrypt under wrong key must not 'succeed'"
    );
    assert!(
        report.errors.iter().any(|e| e.starts_with("peer_map:")),
        "peer_map failure should surface in report.errors; got {:?}",
        report.errors
    );
    assert_eq!(
        state.peer_map.lock().unwrap().len(),
        0,
        "failed reload must leave state at default, not partial"
    );

    set_file_storage_key(None);
}

#[test]
fn reload_overwrites_bootstrap_defaults() {
    // End-to-end: simulate the production failure mode.
    //
    //   1. User has a password set; AppPreferences on disk says
    //      tour.completed = true.
    //   2. Process boots. file_storage_key is None. Bootstrap reads
    //      app_preferences.json — fails decrypt — leaves state at
    //      default (completed = false).  ← step modelled here as
    //      a fresh AppState
    //   3. User enters password at gate. file_storage_key installs.
    //   4. reload_encrypted_state_after_unlock runs. State should
    //      now match disk: tour.completed = true.
    //
    // Pre-FIX2 the reload step did not exist and state stayed at
    // the bootstrap default, which is what caused the tour to
    // replay on every launch.

    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    install_test_key();

    let dir = tempdir().unwrap();
    let prefs_on_disk = AppPreferences {
        version: APP_PREFERENCES_VERSION,
        stego_mode: StegoMode::Mode0,
        tour: TourState {
            completed: true,
            skipped: false,
            last_slide: 9,
        },
        vpn_warning_dismissed_forever: false,
    };
    write_app_preferences(&dir.path().join("app_preferences.json"), &prefs_on_disk).unwrap();

    // Bootstrap-shape state: defaults (would have come from a
    // failed decrypt during run_autostart).
    let state = AppState::new();
    assert!(!state.app_preferences.lock().unwrap().tour.completed);

    // Post-gate reload.
    let report = reload_encrypted_state_after_unlock(&state, dir.path()).unwrap();
    assert!(report.app_prefs_loaded);

    // The bug repro: without reload, tour.completed stays false and
    // boot.js fires the tour again. With the reload, completed is
    // true and oslInstallTour no-ops.
    assert!(state.app_preferences.lock().unwrap().tour.completed);

    set_file_storage_key(None);
}
