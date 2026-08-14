use std::sync::Mutex;

use ipc::main_password::{
    get_file_storage_key, set_file_storage_key, set_main_password, set_stealth_password,
    verify_gate_password_attempt, GatePasswordAttemptResult,
};
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
fn task0336_saved_main_and_stealth_passwords_take_the_real_gate_paths() {
    let _serial = PROCESS_GLOBALS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _reset = FileKeyReset;
    let base = tempdir().expect("temporary keystore base directory");
    keystore::set_base_dir_override(Some(base.path().to_path_buf()));
    let dir = tempdir().expect("temporary password-marker directory");
    let main = "normal-0336";
    let stealth = "stealth-0336";
    let near_match = "stealth-0336!";
    let mut decoy_unlocks = 0_u8;

    set_file_storage_key(None);
    set_main_password(dir.path(), main).expect("save normal password through production storage");
    set_stealth_password(dir.path(), main, stealth)
        .expect("save exact stealth password through production storage");
    println!(
        "TASK0336_BACKEND_START saved_normal={main} saved_stealth={stealth} decoy_unlock_count={decoy_unlocks}"
    );

    match verify_gate_password_attempt(dir.path(), main).expect("verify saved normal password") {
        GatePasswordAttemptResult::Main(_) => {}
        _ => panic!("saved normal password must unlock real workspace"),
    }
    assert!(
        get_file_storage_key().is_some(),
        "normal unlock must install the real workspace key"
    );
    println!("TASK0336_NORMAL_UNLOCK result=real_workspace decoy_unlock_count={decoy_unlocks}");

    set_file_storage_key(None);
    match verify_gate_password_attempt(dir.path(), stealth)
        .expect("verify exact saved stealth password")
    {
        GatePasswordAttemptResult::Stealth => decoy_unlocks += 1,
        _ => panic!("exact stealth password must open the decoy workspace"),
    }
    assert_eq!(
        get_file_storage_key(),
        None,
        "stealth unlock must not install the real workspace key"
    );
    assert_eq!(
        decoy_unlocks, 1,
        "the exact stealth credential must record one decoy unlock"
    );
    println!("TASK0336_STEALTH_UNLOCK result=decoy_workspace decoy_unlock_count={decoy_unlocks}");

    let count_before_near_match = decoy_unlocks;
    match verify_gate_password_attempt(dir.path(), near_match).expect("verify near-match refusal") {
        GatePasswordAttemptResult::Wrong { .. } => {}
        _ => panic!("near-match must be refused"),
    }
    assert_eq!(
        decoy_unlocks, count_before_near_match,
        "near-match must not record a decoy unlock"
    );
    println!("TASK0336_NEAR_MATCH entered={near_match} result=refused decoy_unlock_count={decoy_unlocks}");
}
