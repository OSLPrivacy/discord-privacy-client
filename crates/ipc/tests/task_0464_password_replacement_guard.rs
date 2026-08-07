use std::fs;
use std::sync::Mutex;

use ipc::main_password::{
    get_file_storage_key, set_file_storage_key, set_main_password,
    set_main_password_after_recovery, verify_main_password, verify_recovery_phrase,
};
use ipc::state::AppState;
use tempfile::tempdir;

static PROCESS_GLOBALS: Mutex<()> = Mutex::new(());

struct FileKeyReset;

impl Drop for FileKeyReset {
    fn drop(&mut self) {
        set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

#[test]
fn task0464_recovery_reset_replaces_the_unlock_password() {
    let _serial = PROCESS_GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    let _reset = FileKeyReset;
    set_file_storage_key(None);

    let base = tempdir().expect("base tempdir");
    keystore::set_base_dir_override(Some(base.path().to_path_buf()));

    let dir = tempdir().expect("account tempdir");
    let marker_path = dir.path().join("password_marker.json");
    let marker_tmp = marker_path.with_extension("tmp");
    let marker_backup = marker_path.with_extension("bak");

    let phrase = set_main_password(dir.path(), "password-one").expect("set initial password");
    let marker_before = fs::read(&marker_path).expect("read initial marker");
    set_file_storage_key(None);

    let state = AppState::new();
    let token =
        verify_recovery_phrase(&state, dir.path(), &phrase).expect("recovery phrase approves");
    set_main_password_after_recovery(&state, dir.path(), "password-two", &token)
        .expect("recovery reset saves the new password");

    let marker_after = fs::read(&marker_path).expect("read replacement marker");
    assert_ne!(
        marker_before, marker_after,
        "reset must replace the on-disk marker instead of retaining the old password verifier"
    );
    assert!(
        !marker_tmp.exists(),
        "successful marker replacement must not leave an incomplete temp marker"
    );
    assert!(
        !marker_backup.exists(),
        "successful marker replacement must not leave the old marker as a backup"
    );

    set_file_storage_key(None);
    let old_error = verify_main_password(dir.path(), "password-one")
        .expect_err("old password must be refused after reset");
    println!("TASK0464 old_password_result=refused error={old_error}");

    set_file_storage_key(None);
    verify_main_password(dir.path(), "password-two").expect("new password unlocks after reset");
    let installed = get_file_storage_key().is_some();
    println!("TASK0464 new_password_result=unlocked file_storage_key_installed={installed}");
    println!(
        "TASK0464 marker_replaced={} tmp_present={} backup_present={}",
        marker_before != marker_after,
        marker_tmp.exists(),
        marker_backup.exists()
    );

    assert!(
        installed,
        "new password unlock must install the file storage key"
    );
}
