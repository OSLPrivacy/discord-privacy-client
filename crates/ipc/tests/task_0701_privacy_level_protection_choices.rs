use ipc::commands::{
    cmd_osl_read_privacy_protection_choices, cmd_osl_save_privacy_level_rule_set,
    PrivacyProtectionChoicesDto,
};
use ipc::main_password::set_file_storage_key;
use ipc::state::AppState;
use std::collections::BTreeSet;
use std::sync::Mutex;

static KEY_LOCK: Mutex<()> = Mutex::new(());

struct FileStorageKeyGuard;

impl Drop for FileStorageKeyGuard {
    fn drop(&mut self) {
        set_file_storage_key(None);
    }
}

fn named_results(choices: &PrivacyProtectionChoicesDto) -> String {
    format!(
        "warnings={},cleanup={},app_exceptions={},contact_rules={}",
        choices.warnings, choices.cleanup, choices.app_exceptions, choices.contact_rules
    )
}

#[test]
fn task_0701_changing_saved_level_changes_all_named_protection_choice_results() {
    let _lock = KEY_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    set_file_storage_key(Some([0x71u8; 32]));
    let _key_guard = FileStorageKeyGuard;

    let dir = tempfile::tempdir().expect("tempdir");
    let saving_state = AppState::new();
    let mut warning_results = BTreeSet::new();
    let mut cleanup_results = BTreeSet::new();
    let mut app_exception_results = BTreeSet::new();
    let mut contact_rule_results = BTreeSet::new();
    let mut changed_levels = 0usize;

    for level in ["basic", "balanced", "maximum"] {
        cmd_osl_save_privacy_level_rule_set(
            &saving_state,
            level.to_string(),
            Some(dir.path().to_path_buf()),
        )
        .expect("save current privacy level");

        let loaded_state = AppState::new();
        *loaded_state.app_preferences.lock().unwrap() =
            ipc::app_preferences::load_app_preferences(&dir.path().join("app_preferences.json"));

        let choices = cmd_osl_read_privacy_protection_choices(&loaded_state).expect("read choices");
        let results = named_results(&choices);
        println!("TASK0701 privacy_level.{}.current={}", level, choices.level);
        println!(
            "TASK0701 privacy_level.{}.named_rule_results={results}",
            level
        );

        assert_eq!(choices.level, level);
        warning_results.insert(choices.warnings);
        cleanup_results.insert(choices.cleanup);
        app_exception_results.insert(choices.app_exceptions);
        contact_rule_results.insert(choices.contact_rules);
        changed_levels += 1;
    }

    println!("TASK0701 privacy_level.changed_level_count={changed_levels}");
    println!(
        "TASK0701 named_rule_result.warnings.distinct_count={}",
        warning_results.len()
    );
    println!(
        "TASK0701 named_rule_result.cleanup.distinct_count={}",
        cleanup_results.len()
    );
    println!(
        "TASK0701 named_rule_result.app_exceptions.distinct_count={}",
        app_exception_results.len()
    );
    println!(
        "TASK0701 named_rule_result.contact_rules.distinct_count={}",
        contact_rule_results.len()
    );

    assert_eq!(changed_levels, 3);
    assert_eq!(warning_results.len(), 3, "warnings must change by level");
    assert_eq!(cleanup_results.len(), 3, "cleanup must change by level");
    assert_eq!(
        app_exception_results.len(),
        3,
        "app exceptions must change by level"
    );
    assert_eq!(
        contact_rule_results.len(),
        3,
        "contact rules must change by level"
    );
}
