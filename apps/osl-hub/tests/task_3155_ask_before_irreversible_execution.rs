use ipc::app_preferences::{
    AppPreferences, AskBeforeIrreversibleActionsChoice, APP_PREFERENCES_VERSION,
};
use ipc::irreversible_action::{COMPLETED_ANSWER, NEEDS_CONFIRMING_ANSWER};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::irreversible_actions::{delete_scrub_marked_messages, reset_every_setting};
use osl_privacy_hub::security::{self, HubSecurityState};
use osl_privacy_hub::shared_marked_message_deleter::{
    SharedMarkedDeletionRequest, SharedMarkedMessage, SharedMarkedMessageRemover,
    SharedReviewDecision,
};

struct RecordingRemover {
    removed: Vec<String>,
}

impl SharedMarkedMessageRemover for RecordingRemover {
    fn service_id(&self) -> &str {
        "discord"
    }

    fn remove_marked_message(&mut self, message: &SharedMarkedMessage) -> Result<(), String> {
        self.removed.push(message.message_id.clone());
        Ok(())
    }
}

fn scrub_request(message_id: &str) -> SharedMarkedDeletionRequest {
    SharedMarkedDeletionRequest::one(
        "discord",
        Some("owner-3155".to_owned()),
        SharedMarkedMessage::new(
            "discord",
            message_id,
            "chat:3155",
            Some("owner-3155".to_owned()),
            true,
            SharedReviewDecision::MarkedForDeletion,
        ),
    )
}

#[test]
fn task_3155_scrub_delete_refuses_until_confirmed_and_off_runs_immediately() {
    let state = ipc::AppState::new();
    let mut remover = RecordingRemover {
        removed: Vec::new(),
    };

    let refused =
        delete_scrub_marked_messages(&state, &mut remover, scrub_request("scrub-on-3155"), false)
            .unwrap();
    assert_eq!(refused.answer(), NEEDS_CONFIRMING_ANSWER);
    assert!(remover.removed.is_empty());

    let confirmed =
        delete_scrub_marked_messages(&state, &mut remover, scrub_request("scrub-on-3155"), true)
            .unwrap();
    assert_eq!(confirmed.answer(), COMPLETED_ANSWER);
    assert_eq!(remover.removed, ["scrub-on-3155"]);

    state
        .app_preferences
        .lock()
        .unwrap()
        .ask_before_irreversible_actions = AskBeforeIrreversibleActionsChoice::Off;
    let off =
        delete_scrub_marked_messages(&state, &mut remover, scrub_request("scrub-off-3155"), false)
            .unwrap();
    assert_eq!(off.answer(), COMPLETED_ANSWER);
    assert_eq!(remover.removed, ["scrub-on-3155", "scrub-off-3155"]);

    println!(
        "TASK3155 action=scrub-delete on_unconfirmed={} removals_after_refusal=0 on_confirmed={} off_unconfirmed={} total_removals={}",
        refused.answer(),
        confirmed.answer(),
        off.answer(),
        remover.removed.len(),
    );
}

struct StorageGuard {
    previous_account: Option<std::path::PathBuf>,
    previous_key: Option<[u8; 32]>,
}

impl Drop for StorageGuard {
    fn drop(&mut self) {
        keystore::set_active_account_dir(self.previous_account.clone());
        ipc::main_password::set_file_storage_key(self.previous_key);
    }
}

fn use_storage(dir: &std::path::Path) -> StorageGuard {
    let guard = StorageGuard {
        previous_account: keystore::active_account_dir(),
        previous_key: ipc::main_password::get_file_storage_key(),
    };
    keystore::set_active_account_dir(Some(dir.to_owned()));
    ipc::main_password::set_file_storage_key(Some([0x31; 32]));
    guard
}

fn reset_fixture(
    dir: &std::path::Path,
    choice: AskBeforeIrreversibleActionsChoice,
) -> (HubCoreState, HubSecurityState) {
    let core = HubCoreState::default();
    *core.osl.identity.lock().unwrap() = Some(keystore::generate_native_identity());

    let mut preferences = AppPreferences::default();
    preferences.version = APP_PREFERENCES_VERSION;
    preferences.ask_before_irreversible_actions = choice;
    ipc::app_preferences::write_app_preferences(&dir.join("app_preferences.json"), &preferences)
        .unwrap();
    *core.osl.app_preferences.lock().unwrap() = preferences;
    ipc::auto_whitelist_rules::write_auto_whitelist_rules(
        &dir.join("auto_whitelist_rules.json"),
        &ipc::auto_whitelist_rules::AutoWhitelistRules::default(),
    )
    .unwrap();

    let security_state = HubSecurityState::default();
    security::set_look_choice(
        &security_state,
        "accent".to_owned(),
        "task-3155-changed".to_owned(),
    )
    .unwrap();
    (core, security_state)
}

#[test]
fn task_3155_full_settings_reset_refuses_until_confirmed_and_off_runs_immediately() {
    let on_dir = tempfile::tempdir().unwrap();
    let _on_storage = use_storage(on_dir.path());
    let (on_core, on_security) =
        reset_fixture(on_dir.path(), AskBeforeIrreversibleActionsChoice::On);

    let refused = reset_every_setting(&on_core, &on_security, false).unwrap();
    assert_eq!(refused.answer(), NEEDS_CONFIRMING_ANSWER);
    assert_eq!(
        security::look_choice_value(&on_security, "accent".to_owned()).unwrap(),
        Some("task-3155-changed".to_owned())
    );

    let confirmed = reset_every_setting(&on_core, &on_security, true).unwrap();
    assert_eq!(confirmed.answer(), COMPLETED_ANSWER);
    assert_eq!(
        security::look_choice_value(&on_security, "accent".to_owned()).unwrap(),
        None
    );
    drop(_on_storage);

    let off_dir = tempfile::tempdir().unwrap();
    let _off_storage = use_storage(off_dir.path());
    let (off_core, off_security) =
        reset_fixture(off_dir.path(), AskBeforeIrreversibleActionsChoice::Off);
    let off = reset_every_setting(&off_core, &off_security, false).unwrap();
    assert_eq!(off.answer(), COMPLETED_ANSWER);
    assert_eq!(
        security::look_choice_value(&off_security, "accent".to_owned()).unwrap(),
        None
    );

    println!(
        "TASK3155 action=full-settings-reset on_unconfirmed={} changed_setting_after_refusal=1 on_confirmed={} off_unconfirmed={} changed_settings_after_each_run=0",
        refused.answer(),
        confirmed.answer(),
        off.answer(),
    );
}
