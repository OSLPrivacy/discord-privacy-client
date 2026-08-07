use ipc::commands::{
    cmd_osl_check_recovery_words, cmd_osl_set_main_password, RecoveryWordRetypeEntryDto,
};
use std::sync::Mutex;
use tempfile::TempDir;

static PROCESS_GLOBALS: Mutex<()> = Mutex::new(());

struct RestoreGlobals;

impl Drop for RestoreGlobals {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

#[test]
fn task_0304_selected_recovery_words_pass_and_wrong_position_fails() {
    let _serial = PROCESS_GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    let _restore = RestoreGlobals;
    let dir = TempDir::new().unwrap();
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(dir.path().to_path_buf()));
    ipc::main_password::set_file_storage_key(None);

    let password = "task-0304-main-password";
    let phrase = cmd_osl_set_main_password(password.to_string()).expect("main password is set");
    let words: Vec<&str> = phrase.split_whitespace().collect();
    assert_eq!(words.len(), 12, "fixture phrase must have twelve words");

    let distinct_from_first = words
        .iter()
        .enumerate()
        .find(|(_, word)| **word != words[0])
        .map(|(index, _)| index)
        .expect("fixture phrase must have at least two distinct words");
    let mut selected = vec![0usize, distinct_from_first];
    if distinct_from_first == 11 {
        selected.push(1);
    } else {
        selected.push(11);
    }
    let correct_entries: Vec<RecoveryWordRetypeEntryDto> = selected
        .iter()
        .map(|index| RecoveryWordRetypeEntryDto {
            position: (*index + 1) as u8,
            word: words[*index].to_string(),
        })
        .collect();

    let pass = cmd_osl_check_recovery_words(password.to_string(), correct_entries)
        .expect("exact selected recovery words should be checked");
    assert!(pass.ok, "exact selected words must pass");
    assert!(pass.checked.iter().all(|entry| entry.matched));
    println!(
        "TASK 0304 exact selected words pass: selected={} ok={}",
        selected
            .iter()
            .map(|index| format!("{}:{}", index + 1, words[*index]))
            .collect::<Vec<_>>()
            .join(","),
        pass.ok
    );

    let wrong_position = distinct_from_first + 1;
    let wrong_position_check = cmd_osl_check_recovery_words(
        password.to_string(),
        vec![RecoveryWordRetypeEntryDto {
            position: wrong_position as u8,
            word: words[0].to_string(),
        }],
    )
    .expect("a correct word in the wrong position should be evaluated");
    assert!(
        !wrong_position_check.ok,
        "a correct recovery word entered for the wrong position must fail"
    );
    assert_eq!(
        wrong_position_check.checked,
        vec![ipc::commands::RecoveryWordRetypeMatchDto {
            position: wrong_position as u8,
            matched: false,
        }]
    );
    println!(
        "TASK 0304 correct word in wrong position fails: word={} expected_position=1 checked_position={} ok={} matched={}",
        words[0],
        wrong_position,
        wrong_position_check.ok,
        wrong_position_check.checked[0].matched
    );
}
