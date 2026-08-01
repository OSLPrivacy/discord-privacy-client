//! G3.3 — update channel persistence + DTO surface.
//!
//! Pure-Rust coverage of the channel command surface (no Tauri /
//! webview runtime), mirroring the G3.1 split: the plugin-touching
//! parts live in `src-tauri/src/main.rs`; the persisted-preference
//! logic + serde shapes are exercised here.

use ipc::app_preferences::{load_app_preferences, UpdateChannel};
use ipc::commands::{cmd_osl_get_update_channel, cmd_osl_set_update_channel, UpdateInstallResult};
use ipc::AppState;
use tempfile::TempDir;

// `ipc`'s file-storage key and config-dir overrides are process globals, and
// the tests in one integration binary share a process. `cargo test` runs them
// on several threads, so without this every test here can have its key swapped
// out from under it mid-write -- which is exactly how Windows CI produced
// `at-rest decrypt: aead::Error`.
static OSL_PROCESS_GLOBALS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn default_channel_is_stable() {
    let _osl_serial = OSL_PROCESS_GLOBALS_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let state = AppState::new();
    assert_eq!(
        cmd_osl_get_update_channel(&state).unwrap(),
        UpdateChannel::Stable
    );
}

#[test]
fn set_channel_updates_in_memory_state() {
    let _osl_serial = OSL_PROCESS_GLOBALS_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let state = AppState::new();
    cmd_osl_set_update_channel(&state, UpdateChannel::Beta, None).unwrap();
    assert_eq!(
        cmd_osl_get_update_channel(&state).unwrap(),
        UpdateChannel::Beta
    );
    // Back to stable.
    cmd_osl_set_update_channel(&state, UpdateChannel::Stable, None).unwrap();
    assert_eq!(
        cmd_osl_get_update_channel(&state).unwrap(),
        UpdateChannel::Stable
    );
}

#[test]
fn set_channel_persists_to_app_preferences_file() {
    let _osl_serial = OSL_PROCESS_GLOBALS_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _base = tempfile::TempDir::new().unwrap();
    keystore::set_base_dir_override(Some(_base.path().to_path_buf()));
    ipc::main_password::set_file_storage_key(None);

    let dir = TempDir::new().unwrap();
    let state = AppState::new();
    cmd_osl_set_update_channel(&state, UpdateChannel::Beta, Some(dir.path().to_path_buf()))
        .unwrap();

    // Reload straight off disk via the existing persistence layer —
    // no new mechanism introduced.
    let reloaded = load_app_preferences(&dir.path().join("app_preferences.json"));
    assert_eq!(reloaded.update_channel, UpdateChannel::Beta);
}

#[test]
fn channel_serde_matches_keyserver_query_values() {
    let _osl_serial = OSL_PROCESS_GLOBALS_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    assert_eq!(
        serde_json::to_value(UpdateChannel::Stable).unwrap(),
        serde_json::json!("stable")
    );
    assert_eq!(
        serde_json::to_value(UpdateChannel::Beta).unwrap(),
        serde_json::json!("beta")
    );
    assert_eq!(UpdateChannel::Stable.as_query_value(), "stable");
    assert_eq!(UpdateChannel::Beta.as_query_value(), "beta");
}

#[test]
fn install_result_serializes_with_status_tag() {
    let _osl_serial = OSL_PROCESS_GLOBALS_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let no = serde_json::to_value(UpdateInstallResult::NoUpdate).unwrap();
    assert_eq!(no["status"], "no_update");

    let err = serde_json::to_value(UpdateInstallResult::Error {
        message: "sig verify failed".to_string(),
    })
    .unwrap();
    assert_eq!(err["status"], "error");
    assert_eq!(err["message"], "sig verify failed");
}
