use ipc::commands::{
    cmd_osl_read_privacy_level_rule_set, cmd_osl_read_privacy_protection_choices,
    cmd_osl_save_privacy_level_rule_set, PrivacyLevelRuleSetDto,
};
use ipc::main_password::set_file_storage_key;
use ipc::state::AppState;
use std::sync::Mutex;

static KEY_LOCK: Mutex<()> = Mutex::new(());

struct FileStorageKeyGuard;

impl Drop for FileStorageKeyGuard {
    fn drop(&mut self) {
        set_file_storage_key(None);
    }
}

fn rules_line(rules: &PrivacyLevelRuleSetDto) -> String {
    format!(
        "level={},label={},before_send_warnings={},attachment_cleaning={},cleanup_review_days={},public_post_checks={},vpn_required_actions={},protected_contacts_required={}",
        rules.level,
        rules.label,
        rules.before_send_warnings,
        rules.attachment_cleaning,
        rules.cleanup_review_days,
        rules.public_post_checks,
        rules.vpn_required_actions,
        rules.protected_contacts_required
    )
}

fn saved_rule_set_count(state: &AppState) -> usize {
    state
        .app_preferences
        .lock()
        .expect("app_preferences mutex poisoned")
        .privacy_level_rule_sets
        .len()
}

#[test]
fn task_0341_privacy_level_commands_save_read_and_refuse_unknown_levels() {
    let _lock = KEY_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    set_file_storage_key(Some([0x34u8; 32]));
    let _key_guard = FileStorageKeyGuard;

    let dir = tempfile::tempdir().expect("tempdir");
    let state = AppState::new();
    let initial = cmd_osl_read_privacy_protection_choices(&state).expect("read initial choices");
    println!(
        "TASK0341 native.initial.level={},label={},saved_rule_set_count={}",
        initial.level,
        initial.label,
        saved_rule_set_count(&state)
    );
    assert_eq!(initial.level, "balanced");
    assert_eq!(saved_rule_set_count(&state), 0);

    for (control, level) in [
        ("Basic", "basic"),
        ("Balanced", "balanced"),
        ("Maximum", "maximum"),
    ] {
        let saved = cmd_osl_save_privacy_level_rule_set(
            &state,
            level.to_string(),
            Some(dir.path().to_path_buf()),
        )
        .expect("save privacy level rules");
        let read = cmd_osl_read_privacy_level_rule_set(&state, level.to_string())
            .expect("read saved privacy level rules");
        println!(
            "TASK0341 native.control.{control}.saved_rules={}",
            rules_line(&saved)
        );
        println!(
            "TASK0341 native.control.{control}.read_rules={}",
            rules_line(&read)
        );
        assert_eq!(saved, read);
        assert_eq!(saved.level, level);
        assert_eq!(saved.label, control);
    }

    let before_unknown_rules = cmd_osl_read_privacy_level_rule_set(&state, "maximum".to_string())
        .expect("read maximum before unknown");
    let before_unknown_count = saved_rule_set_count(&state);
    let refusal = cmd_osl_save_privacy_level_rule_set(
        &state,
        "unknown".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .expect_err("unknown privacy level must be refused");
    let after_unknown_rules = cmd_osl_read_privacy_level_rule_set(&state, "maximum".to_string())
        .expect("read maximum after unknown");
    let after_unknown_count = saved_rule_set_count(&state);
    println!("TASK0341 native.control.Unknown.refusal={refusal}");
    println!(
        "TASK0341 native.control.Unknown.saved_rules_unchanged={}",
        before_unknown_rules == after_unknown_rules
    );
    println!(
        "TASK0341 native.control.Unknown.saved_rule_set_count_before={before_unknown_count},after={after_unknown_count}"
    );
    assert_eq!(refusal, "OSL: unknown privacy level 'unknown'");
    assert_eq!(before_unknown_rules, after_unknown_rules);
    assert_eq!(before_unknown_count, after_unknown_count);
}
