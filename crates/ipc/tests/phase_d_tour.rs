//! Phase 9-D: onboarding tour + VPN warning command tests.
//!
//! Covers the persistence semantics of the 7 tour/VPN preference
//! commands. The `cmd_osl_check_vpn` HTTP probe lives in main.rs
//! (uses tokio+reqwest, which `ipc` deliberately avoids) and is
//! covered by manual acceptance instead.

use ipc::app_preferences::{load_app_preferences, AppPreferences, TourState};
use ipc::commands::{
    cmd_osl_tour_advance, cmd_osl_tour_complete, cmd_osl_tour_get_state, cmd_osl_tour_reset,
    cmd_osl_tour_skip, cmd_osl_vpn_warning_dismiss_forever, cmd_osl_vpn_warning_reset,
};
use ipc::AppState;
use std::sync::Mutex;
use tempfile::tempdir;

static KEY_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn tour_state_default_is_not_completed() {
    let state = AppState::new();
    let s = cmd_osl_tour_get_state(&state).unwrap();
    assert!(!s.completed);
    assert!(!s.skipped);
    assert_eq!(s.last_slide, 0);
    assert!(!s.vpn_warning_dismissed_forever);
}

#[test]
fn tour_advance_persists_slide() {
    use ipc::main_password::set_file_storage_key;
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(None);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    let path = dir.path().join("app_preferences.json");

    cmd_osl_tour_advance(&state, 4, Some(dir.path().to_path_buf())).unwrap();
    let s = cmd_osl_tour_get_state(&state).unwrap();
    assert_eq!(s.last_slide, 4);
    assert!(!s.completed);

    let on_disk = load_app_preferences(&path);
    assert_eq!(on_disk.tour.last_slide, 4);
}

#[test]
fn tour_complete_sets_flag() {
    use ipc::main_password::set_file_storage_key;
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(None);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    let path = dir.path().join("app_preferences.json");

    cmd_osl_tour_complete(&state, Some(dir.path().to_path_buf())).unwrap();
    let s = cmd_osl_tour_get_state(&state).unwrap();
    assert!(s.completed);
    assert!(!s.skipped);
    assert_eq!(s.last_slide, 9);

    let on_disk = load_app_preferences(&path);
    assert!(on_disk.tour.completed);
    assert_eq!(on_disk.tour.last_slide, 9);
}

#[test]
fn tour_skip_sets_both_flags() {
    use ipc::main_password::set_file_storage_key;
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(None);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    let path = dir.path().join("app_preferences.json");

    cmd_osl_tour_skip(&state, Some(dir.path().to_path_buf())).unwrap();
    let s = cmd_osl_tour_get_state(&state).unwrap();
    assert!(s.completed);
    assert!(s.skipped);

    let on_disk = load_app_preferences(&path);
    assert!(on_disk.tour.completed);
    assert!(on_disk.tour.skipped);
}

#[test]
fn tour_reset_clears_state() {
    use ipc::main_password::set_file_storage_key;
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(None);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    let path = dir.path().join("app_preferences.json");

    // Drive into completed state first, then reset.
    cmd_osl_tour_complete(&state, Some(dir.path().to_path_buf())).unwrap();
    cmd_osl_tour_skip(&state, Some(dir.path().to_path_buf())).unwrap();
    cmd_osl_tour_reset(&state, Some(dir.path().to_path_buf())).unwrap();

    let s = cmd_osl_tour_get_state(&state).unwrap();
    assert!(!s.completed);
    assert!(!s.skipped);
    assert_eq!(s.last_slide, 0);

    let on_disk = load_app_preferences(&path);
    assert_eq!(on_disk.tour, TourState::default());
}

#[test]
fn vpn_dismiss_forever_persists() {
    use ipc::main_password::set_file_storage_key;
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(None);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    let path = dir.path().join("app_preferences.json");

    cmd_osl_vpn_warning_dismiss_forever(&state, Some(dir.path().to_path_buf())).unwrap();
    let s = cmd_osl_tour_get_state(&state).unwrap();
    assert!(s.vpn_warning_dismissed_forever);

    let on_disk = load_app_preferences(&path);
    assert!(on_disk.vpn_warning_dismissed_forever);
}

#[test]
fn vpn_warning_reset_clears_dismissal() {
    use ipc::main_password::set_file_storage_key;
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(None);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    let path = dir.path().join("app_preferences.json");

    cmd_osl_vpn_warning_dismiss_forever(&state, Some(dir.path().to_path_buf())).unwrap();
    cmd_osl_vpn_warning_reset(&state, Some(dir.path().to_path_buf())).unwrap();
    let s = cmd_osl_tour_get_state(&state).unwrap();
    assert!(!s.vpn_warning_dismissed_forever);

    let on_disk = load_app_preferences(&path);
    assert!(!on_disk.vpn_warning_dismissed_forever);
}

/// 9-D version bump: writing through any of the new commands stamps
/// version=2 on disk. Legacy v1 files keep their stego_mode but get
/// the version field bumped on next mutation.
#[test]
fn writes_stamp_version_2() {
    use ipc::app_preferences::APP_PREFERENCES_VERSION;
    use ipc::main_password::set_file_storage_key;
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(None);

    assert_eq!(APP_PREFERENCES_VERSION, 2);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    let path = dir.path().join("app_preferences.json");
    cmd_osl_tour_advance(&state, 1, Some(dir.path().to_path_buf())).unwrap();

    let on_disk = load_app_preferences(&path);
    assert_eq!(on_disk.version, APP_PREFERENCES_VERSION);
}

/// Legacy v1 file (pre-D) carries `stego_mode` but no `tour` or
/// `vpn_warning_dismissed_forever` keys. serde defaults must apply.
#[test]
fn legacy_v1_loads_with_defaults_for_new_fields() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy.json");
    let legacy = r#"{ "version": 1, "stego_mode": "mode1" }"#;
    std::fs::write(&path, legacy).unwrap();
    let p = load_app_preferences(&path);
    assert_eq!(p.tour, TourState::default());
    assert!(!p.vpn_warning_dismissed_forever);
    let expected_mode = ipc::app_preferences::StegoMode::Mode1;
    assert_eq!(p.stego_mode, expected_mode);
}

/// Mid-tour-quit case: setting last_slide and quitting must round-
/// trip across a hypothetical reload (simulate by loading the file
/// and constructing a fresh AppState seeded with that snapshot).
#[test]
fn mid_tour_resume_survives_reload() {
    use ipc::main_password::set_file_storage_key;
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(None);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    let path = dir.path().join("app_preferences.json");
    cmd_osl_tour_advance(&state, 4, Some(dir.path().to_path_buf())).unwrap();

    let snapshot: AppPreferences = load_app_preferences(&path);
    assert_eq!(snapshot.tour.last_slide, 4);
    assert!(!snapshot.tour.completed);

    let state2 = AppState::new();
    *state2.app_preferences.lock().unwrap() = snapshot;
    let resumed = cmd_osl_tour_get_state(&state2).unwrap();
    assert_eq!(resumed.last_slide, 4);
    assert!(!resumed.completed);
}
