use ipc::commands::{
    cmd_osl_read_privacy_level_rule_set, cmd_osl_save_privacy_level_rule_set,
    PrivacyLevelRuleSetDto,
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

fn rule_values(rules: &PrivacyLevelRuleSetDto) -> String {
    format!(
        "before_send_warnings={},attachment_cleaning={},cleanup_review_days={},public_post_checks={},vpn_required_actions={},protected_contacts_required={}",
        rules.before_send_warnings,
        rules.attachment_cleaning,
        rules.cleanup_review_days,
        rules.public_post_checks,
        rules.vpn_required_actions,
        rules.protected_contacts_required
    )
}

#[test]
fn task_0700_direct_commands_return_different_rule_values_for_three_privacy_levels() {
    let _lock = KEY_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    set_file_storage_key(Some([0x70u8; 32]));
    let _key_guard = FileStorageKeyGuard;

    let dir = tempfile::tempdir().expect("tempdir");
    let saving_state = AppState::new();
    for level in ["basic", "balanced", "maximum"] {
        cmd_osl_save_privacy_level_rule_set(
            &saving_state,
            level.to_string(),
            Some(dir.path().to_path_buf()),
        )
        .expect("save privacy level rule set");
    }

    let loaded_state = AppState::new();
    *loaded_state.app_preferences.lock().unwrap() =
        ipc::app_preferences::load_app_preferences(&dir.path().join("app_preferences.json"));

    let mut saved_rule_sets = 0usize;
    let mut distinct_rule_values = BTreeSet::new();
    for level in ["basic", "balanced", "maximum"] {
        let read = cmd_osl_read_privacy_level_rule_set(&loaded_state, level.to_string())
            .expect("read privacy level rule set");
        let values = rule_values(&read);
        println!(
            "TASK0700 privacy_level.{}.choice={}",
            read.level, read.label
        );
        println!("TASK0700 privacy_level.{}.rule_values={values}", read.level);
        distinct_rule_values.insert(values);
        saved_rule_sets += 1;
    }

    println!("TASK0700 privacy_level.saved_rule_set_count={saved_rule_sets}");
    println!(
        "TASK0700 privacy_level.different_rule_value_count={}",
        distinct_rule_values.len()
    );

    assert_eq!(saved_rule_sets, 3);
    assert_eq!(distinct_rule_values.len(), 3);
}
