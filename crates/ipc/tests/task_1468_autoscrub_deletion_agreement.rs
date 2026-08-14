//! TASK 1468: AutoScrub's Find-and-delete authority is a durable, exact-scope
//! agreement, not a presentation-only checkbox.

use ipc::app_preferences::load_app_preferences;
use ipc::autoscrub_deletion_agreement::{
    AutoScrubDeletionAgreementRequest, DELETION_AGREEMENT_REQUIRED,
};
use ipc::commands::{
    cmd_osl_authorize_autoscrub_find_and_delete, cmd_osl_save_autoscrub_bad_message_rule,
    cmd_osl_save_autoscrub_deletion_agreement,
};
use ipc::state::AppState;
use tempfile::tempdir;

struct FileKeyGuard;

impl FileKeyGuard {
    fn install() -> Self {
        ipc::main_password::set_file_storage_key(Some([0x68; 32]));
        Self
    }
}

impl Drop for FileKeyGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
    }
}

fn reviewed_request(ticked: bool) -> AutoScrubDeletionAgreementRequest {
    AutoScrubDeletionAgreementRequest {
        selected_account_ids: vec!["telegram-pine".to_owned(), "discord-maple".to_owned()],
        selected_rule_names: vec!["private words".to_owned()],
        deleted_messages_can_be_permanent: true,
        service_rules_may_forbid_automated_reading_or_deletion: true,
        suspension_or_ban_risk_is_real: true,
        allow_autoscrub_to_delete_matching_messages: ticked,
    }
}

fn authorize(
    state: &AppState,
) -> Result<ipc::autoscrub_deletion_agreement::AutoScrubFindAndDeleteAuthorization, String> {
    cmd_osl_authorize_autoscrub_find_and_delete(
        state,
        vec!["discord-maple".to_owned(), "telegram-pine".to_owned()],
        vec!["private words".to_owned()],
    )
}

#[test]
fn task_1468_find_and_delete_is_refused_until_the_agreement_tick_is_saved() {
    let _file_key = FileKeyGuard::install();
    let dir = tempdir().unwrap();
    let prefs_path = dir.path().join("app_preferences.json");
    let state = AppState::new();

    cmd_osl_save_autoscrub_bad_message_rule(
        &state,
        "private words".to_owned(),
        "MAPLE-DELETE".to_owned(),
        Some(dir.path().to_path_buf()),
    )
    .unwrap();

    let before = authorize(&state).expect_err("Find and delete ran before agreement");
    println!(
        "TASK1468 before_saved_tick find_and_delete=refused reason={before} agreement_count=0"
    );
    assert_eq!(before, DELETION_AGREEMENT_REQUIRED);
    assert!(state
        .app_preferences
        .lock()
        .unwrap()
        .autoscrub_deletion_agreement
        .is_none());

    let unticked = cmd_osl_save_autoscrub_deletion_agreement(
        &state,
        reviewed_request(false),
        Some(dir.path().to_path_buf()),
    )
    .expect_err("unticked deletion agreement was saved");
    let after_unticked = authorize(&state).expect_err("unticked agreement authorized deletion");
    println!(
        "TASK1468 unticked_save=refused reason={unticked} find_and_delete=refused saved_agreement_count=0"
    );
    assert_eq!(unticked, DELETION_AGREEMENT_REQUIRED);
    assert_eq!(after_unticked, DELETION_AGREEMENT_REQUIRED);
    assert!(load_app_preferences(&prefs_path)
        .autoscrub_deletion_agreement
        .is_none());

    let saved = cmd_osl_save_autoscrub_deletion_agreement(
        &state,
        reviewed_request(true),
        Some(dir.path().to_path_buf()),
    )
    .expect("ticked deletion agreement saves");
    let persisted = load_app_preferences(&prefs_path)
        .autoscrub_deletion_agreement
        .expect("agreement survived encrypted disk round-trip");
    assert_eq!(persisted, saved);
    assert_eq!(saved.selected_account_ids.len(), 2);
    assert_eq!(saved.selected_rules.len(), 1);
    assert_eq!(saved.selected_rules[0].rule_name, "private words");
    assert_eq!(saved.selected_rules[0].private_word, "MAPLE-DELETE");
    assert!(saved.deleted_messages_can_be_permanent);
    assert!(saved.service_rules_may_forbid_automated_reading_or_deletion);
    assert!(saved.suspension_or_ban_risk_is_real);
    assert!(saved.allow_autoscrub_to_delete_matching_messages);

    let authorized = authorize(&state).expect("saved tick authorizes exact Find and delete scope");
    println!(
        "TASK1468 after_saved_tick find_and_delete={} agreement_count=1 selected_accounts={} selected_rules={} permanence={} service_risk={} ban_risk={} agreement_tick={}",
        authorized.mode,
        authorized.selected_account_ids.len(),
        authorized.selected_rules.len(),
        saved.deleted_messages_can_be_permanent,
        saved.service_rules_may_forbid_automated_reading_or_deletion,
        saved.suspension_or_ban_risk_is_real,
        saved.allow_autoscrub_to_delete_matching_messages,
    );
    assert_eq!(authorized.mode, "find_and_delete");

    // The stored accounts/rules are load-bearing. Agreement for this exact
    // scope does not authorize a different account, nor an edited private word.
    let different_account = cmd_osl_authorize_autoscrub_find_and_delete(
        &state,
        vec!["discord-maple".to_owned()],
        vec!["private words".to_owned()],
    )
    .expect_err("agreement leaked to a different account selection");
    assert_eq!(different_account, DELETION_AGREEMENT_REQUIRED);

    cmd_osl_save_autoscrub_bad_message_rule(
        &state,
        "private words".to_owned(),
        "CHANGED-WORD".to_owned(),
        Some(dir.path().to_path_buf()),
    )
    .unwrap();
    let changed_rule = authorize(&state).expect_err("agreement survived a rule-value change");
    println!(
        "TASK1468 scope_drift account_refusal={different_account} rule_refusal={changed_rule}"
    );
    assert_eq!(changed_rule, DELETION_AGREEMENT_REQUIRED);
}
