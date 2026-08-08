use ipc::app_preferences::load_app_preferences;
use ipc::commands::{
    cmd_osl_list_autoscrub_bad_message_rules, cmd_osl_list_bad_message_rules,
    cmd_osl_save_autoscrub_bad_message_rule, cmd_osl_save_bad_message_rule,
};
use ipc::state::AppState;
use tempfile::tempdir;

struct FileKeyGuard;

impl FileKeyGuard {
    fn install() -> Self {
        ipc::main_password::set_file_storage_key(Some([0x59; 32]));
        Self
    }
}

impl Drop for FileKeyGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
    }
}

#[test]
fn task_1459_autoscrub_saves_one_private_word_without_changing_the_normal_scrub_run() {
    let _file_key = FileKeyGuard::install();
    let dir = tempdir().unwrap();
    let prefs_path = dir.path().join("app_preferences.json");
    let state = AppState::new();

    let scrub_before = cmd_osl_list_bad_message_rules(&state).unwrap();
    let autoscrub_before = cmd_osl_list_autoscrub_bad_message_rules(&state).unwrap();
    println!(
        "TASK1459 before scrub_count={} autoscrub_count={}",
        scrub_before.len(),
        autoscrub_before.len()
    );
    assert_eq!(scrub_before.len(), 0);
    assert_eq!(autoscrub_before.len(), 0);

    let saved = cmd_osl_save_autoscrub_bad_message_rule(
        &state,
        "private words".to_string(),
        "AUTOSCRUB-PRIVATE-1".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .unwrap();

    let scrub_after = cmd_osl_list_bad_message_rules(&state).unwrap();
    let autoscrub_after = cmd_osl_list_autoscrub_bad_message_rules(&state).unwrap();
    let prefs_after = load_app_preferences(&prefs_path);
    println!(
        "TASK1459 after save scrub_count={} autoscrub_count={} saved_rule={} saved_word={}",
        scrub_after.len(),
        autoscrub_after.len(),
        saved.rule_name,
        saved.private_word
    );

    // AutoScrub saved exactly one private word.
    assert_eq!(autoscrub_after.len(), 1);
    assert_eq!(saved.rule_name, "private words");
    assert_eq!(saved.private_word, "AUTOSCRUB-PRIVATE-1");
    assert_eq!(autoscrub_after[0].private_word, "AUTOSCRUB-PRIVATE-1");
    assert_eq!(prefs_after.autoscrub_bad_message_rules.len(), 1);

    // The normal Scrub run is untouched by the AutoScrub save.
    assert_eq!(scrub_after.len(), 0);
    assert_eq!(prefs_after.bad_message_rules.len(), 0);

    // Reused Scrub rule types: an unknown rule name is refused the same way
    // on the AutoScrub surface as it is on the normal Scrub surface.
    let bad_rule_err = cmd_osl_save_autoscrub_bad_message_rule(
        &state,
        "mystery rule".to_string(),
        "whatever".to_string(),
        None,
    )
    .expect_err("TASK1459 unknown rule name was accepted on AutoScrub");
    println!("TASK1459 autoscrub unknown rule refused: {bad_rule_err}");
    assert!(bad_rule_err.contains("unknown rule name"));

    // Now save a normal Scrub rule and confirm AutoScrub is untouched by it.
    let scrub_saved = cmd_osl_save_bad_message_rule(
        &state,
        "private words".to_string(),
        "SCRUB-PRIVATE-1".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .unwrap();
    let scrub_after_2 = cmd_osl_list_bad_message_rules(&state).unwrap();
    let autoscrub_after_2 = cmd_osl_list_autoscrub_bad_message_rules(&state).unwrap();
    println!(
        "TASK1459 after normal scrub save scrub_count={} autoscrub_count={} scrub_word={} autoscrub_word={}",
        scrub_after_2.len(),
        autoscrub_after_2.len(),
        scrub_saved.private_word,
        autoscrub_after_2[0].private_word
    );
    assert_eq!(scrub_after_2.len(), 1);
    assert_eq!(scrub_after_2[0].private_word, "SCRUB-PRIVATE-1");
    // AutoScrub's private word is still the one AutoScrub saved, independent
    // of the normal Scrub save that just happened.
    assert_eq!(autoscrub_after_2.len(), 1);
    assert_eq!(autoscrub_after_2[0].private_word, "AUTOSCRUB-PRIVATE-1");
}
