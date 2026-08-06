use ipc::commands::{
    cmd_osl_reset_main_password_after_recovery, cmd_osl_set_main_password,
    cmd_osl_verify_main_password,
};
use ipc::main_password::{get_file_storage_key, set_file_storage_key};
use ipc::AppState;
use std::sync::Mutex;
use tempfile::TempDir;

static PROCESS_GLOBALS: Mutex<()> = Mutex::new(());

const OLD_PASSWORD: &str = "old-0302-password";
const NEW_PASSWORD: &str = "new-0302-password";

struct KeystoreReset;

impl Drop for KeystoreReset {
    fn drop(&mut self) {
        set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn changed_recovery_phrase(phrase: &str) -> String {
    let mut words: Vec<&str> = phrase.split_whitespace().collect();
    assert!(
        !words.is_empty(),
        "fixture recovery phrase must contain words"
    );
    words[0] = if words[0] == "abandon" {
        "ability"
    } else {
        "abandon"
    };
    words.join(" ")
}

#[test]
fn reset_command_requires_phrase_approval_and_replaces_the_unlock_password() {
    let _serial = PROCESS_GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    let _reset = KeystoreReset;
    let dir = TempDir::new().expect("disposable account dir");
    keystore::set_base_dir_override(Some(dir.path().to_path_buf()));
    keystore::set_active_account_dir(Some(dir.path().to_path_buf()));
    set_file_storage_key(None);

    let state = AppState::new();
    let phrase =
        cmd_osl_set_main_password(OLD_PASSWORD.to_owned()).expect("old password is enrolled");
    let wrong_phrase = changed_recovery_phrase(&phrase);

    set_file_storage_key(None);
    let reset_without_approval =
        cmd_osl_reset_main_password_after_recovery(&state, wrong_phrase, NEW_PASSWORD.to_owned())
            .expect_err("changed phrase must not approve password reset");
    println!("0302 PRE-APPROVAL RESET REFUSED: {reset_without_approval}");

    set_file_storage_key(None);
    cmd_osl_verify_main_password(OLD_PASSWORD.to_owned())
        .expect("old password still unlocks before phrase-approved reset");
    println!("0302 PRE-APPROVAL OLD PASSWORD UNLOCKED");

    set_file_storage_key(None);
    let new_before_approval = cmd_osl_verify_main_password(NEW_PASSWORD.to_owned())
        .expect_err("new password must not unlock before phrase approval");
    println!("0302 PRE-APPROVAL NEW PASSWORD REFUSED: {new_before_approval}");

    set_file_storage_key(None);
    cmd_osl_reset_main_password_after_recovery(&state, phrase, NEW_PASSWORD.to_owned())
        .expect("valid phrase approves reset and command replaces the password");
    println!("0302 PHRASE APPROVAL: accepted");

    set_file_storage_key(None);
    let old_after_reset = cmd_osl_verify_main_password(OLD_PASSWORD.to_owned())
        .expect_err("old password must be refused after reset");
    println!("0302 OLD PASSWORD REFUSED: {old_after_reset}");

    set_file_storage_key(None);
    cmd_osl_verify_main_password(NEW_PASSWORD.to_owned())
        .expect("new password unlocks after reset");
    println!(
        "0302 NEW PASSWORD UNLOCKED: file_storage_key_installed={}",
        get_file_storage_key().is_some()
    );
}
