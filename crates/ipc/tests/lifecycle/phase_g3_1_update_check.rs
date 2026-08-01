//! G3.1 — update-check command surface.
//!
//! `crates/ipc` has no Tauri dependency by design, so the
//! `tauri-plugin-updater` `check()` call lives in the
//! `#[tauri::command] osl_check_for_updates` wrapper in
//! `src-tauri/src/main.rs`. This test exercises the pure mapper it
//! delegates to (`ipc::commands::cmd_osl_check_for_updates`) — i.e.
//! the command surface that JS reaches via
//! `invoke('osl_check_for_updates')` — asserting it is publicly
//! reachable, callable, and produces the three documented
//! JS-facing shapes with the correct serde discriminants.

use ipc::commands::{cmd_osl_check_for_updates, UpdateCheckResult, UpdateInfo};

#[test]
fn up_to_date_when_no_update_offered() {
    let r = cmd_osl_check_for_updates("0.0.1".to_string(), Ok(None));
    assert_eq!(
        r,
        UpdateCheckResult::UpToDate {
            current: "0.0.1".to_string()
        }
    );
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["status"], "up_to_date");
    assert_eq!(v["current"], "0.0.1");
}

#[test]
fn update_available_maps_all_fields() {
    let r = cmd_osl_check_for_updates(
        "0.0.1".to_string(),
        Ok(Some(UpdateInfo {
            version: "0.0.2".to_string(),
            notes: Some("bug fixes".to_string()),
            url: "https://installers.oslprivacy.com/osl-privacy-0.0.2.msi".to_string(),
        })),
    );
    assert_eq!(
        r,
        UpdateCheckResult::UpdateAvailable {
            current: "0.0.1".to_string(),
            next: "0.0.2".to_string(),
            notes: "bug fixes".to_string(),
            url: "https://installers.oslprivacy.com/osl-privacy-0.0.2.msi".to_string(),
        }
    );
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["status"], "update_available");
    assert_eq!(v["next"], "0.0.2");
    assert_eq!(
        v["url"],
        "https://installers.oslprivacy.com/osl-privacy-0.0.2.msi"
    );
}

#[test]
fn missing_notes_become_empty_string() {
    let r = cmd_osl_check_for_updates(
        "0.0.1".to_string(),
        Ok(Some(UpdateInfo {
            version: "0.0.2".to_string(),
            notes: None,
            url: "https://example.test/x.msi".to_string(),
        })),
    );
    match r {
        UpdateCheckResult::UpdateAvailable { notes, .. } => assert_eq!(notes, ""),
        other => panic!("expected UpdateAvailable, got {other:?}"),
    }
}

#[test]
fn check_failure_becomes_error_status() {
    let r = cmd_osl_check_for_updates("0.0.1".to_string(), Err("network unreachable".to_string()));
    assert_eq!(
        r,
        UpdateCheckResult::Error {
            message: "network unreachable".to_string()
        }
    );
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["status"], "error");
    assert_eq!(v["message"], "network unreachable");
}
