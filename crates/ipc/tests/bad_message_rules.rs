use ipc::app_preferences::{load_app_preferences, AppPreferences};
use ipc::commands::{cmd_osl_list_bad_message_rules, cmd_osl_save_bad_message_rule};
use ipc::state::AppState;
use tempfile::tempdir;

struct FileKeyGuard;

impl FileKeyGuard {
    fn install() -> Self {
        ipc::main_password::set_file_storage_key(Some([0x41; 32]));
        Self
    }
}

impl Drop for FileKeyGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
    }
}

fn fresh_state_with_preferences(seed: &AppPreferences) -> AppState {
    let state = AppState::new();
    *state
        .app_preferences
        .lock()
        .expect("app_preferences mutex poisoned") = seed.clone();
    state
}

#[test]
fn task1414_bad_message_rule_parser_refuses_bad_clean_copies() {
    let _file_key = FileKeyGuard::install();
    let dir = tempdir().unwrap();
    let prefs_path = dir.path().join("app_preferences.json");
    let state = AppState::new();
    let before = cmd_osl_list_bad_message_rules(&state).unwrap();
    println!("TASK1414 before saved-rule count={}", before.len());
    assert_eq!(before.len(), 0);

    let saved = cmd_osl_save_bad_message_rule(
        &state,
        "private words".to_string(),
        "MAPLE-4172".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .unwrap();
    let after = cmd_osl_list_bad_message_rules(&state).unwrap();
    let after_prefs = load_app_preferences(&prefs_path);
    println!(
        "TASK1414 after good save count={} result rule_name={} private_word={}",
        after.len(),
        saved.rule_name,
        saved.private_word
    );
    assert_eq!(after.len(), 1);
    assert_eq!(saved.rule_name, "private words");
    assert_eq!(saved.private_word, "MAPLE-4172");
    assert_eq!(after_prefs.bad_message_rules.len(), 1);

    let empty_word_state = fresh_state_with_preferences(&after_prefs);
    let empty_word_err = cmd_osl_save_bad_message_rule(
        &empty_word_state,
        "private words".to_string(),
        "".to_string(),
        None,
    )
    .expect_err("TASK1414 empty private word was accepted");
    let empty_word_rules = cmd_osl_list_bad_message_rules(&empty_word_state).unwrap();
    println!(
        "TASK1414 empty-word copy refused: {empty_word_err}; count={} saved={}={}",
        empty_word_rules.len(),
        empty_word_rules[0].rule_name,
        empty_word_rules[0].private_word
    );
    assert!(empty_word_err.contains("empty private word"));
    assert_eq!(empty_word_rules.len(), 1);
    assert_eq!(empty_word_rules[0].rule_name, "private words");
    assert_eq!(empty_word_rules[0].private_word, "MAPLE-4172");

    let mystery_rule_state = fresh_state_with_preferences(&after_prefs);
    let mystery_rule_err = cmd_osl_save_bad_message_rule(
        &mystery_rule_state,
        "mystery rule".to_string(),
        "MAPLE-4172".to_string(),
        None,
    )
    .expect_err("TASK1414 unknown rule name was accepted");
    let mystery_rule_rules = cmd_osl_list_bad_message_rules(&mystery_rule_state).unwrap();
    println!(
        "TASK1414 mystery-rule copy refused: {mystery_rule_err}; count={} saved={}={}",
        mystery_rule_rules.len(),
        mystery_rule_rules[0].rule_name,
        mystery_rule_rules[0].private_word
    );
    assert!(mystery_rule_err.contains("unknown rule name"));
    assert!(mystery_rule_err.contains("mystery rule"));
    assert_eq!(mystery_rule_rules.len(), 1);
    assert_eq!(mystery_rule_rules[0].rule_name, "private words");
    assert_eq!(mystery_rule_rules[0].private_word, "MAPLE-4172");
}
