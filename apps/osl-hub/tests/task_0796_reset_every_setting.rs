use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::app_preferences::{BehaviourChoiceName, PrivacyLevel};
use ipc::auto_whitelist_rules::AutoWhitelistAppKind;
use ipc::commands::{
    cmd_osl_get_auto_whitelist_rule, cmd_osl_get_new_friend_defaults,
    cmd_osl_read_behaviour_choice, cmd_osl_read_message_default_burn_scope,
    cmd_osl_read_message_default_cover_writing, cmd_osl_read_message_default_timer_seconds,
    cmd_osl_read_message_default_view_once_length_seconds,
    cmd_osl_read_next_generation_message_policy, cmd_osl_read_privacy_protection_choices,
    cmd_osl_read_verification_warning_choice, cmd_osl_save_auto_whitelist_rule,
    cmd_osl_save_behaviour_choice, cmd_osl_save_message_defaults,
    cmd_osl_save_new_friend_defaults, cmd_osl_save_next_generation_message_policy,
    cmd_osl_save_privacy_level_rule_set, cmd_osl_save_verification_warning_choice,
    MessageDefaultsDto, NewFriendDefaultsDto,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::look_window::LOOK_STYLE_BINDINGS;
use osl_privacy_hub::security::{
    app_notification_enabled_before_notice, chat_approval_suggestion_choice,
    list_app_notification_choices, list_look_choices, look_choice_value, reset_every_setting,
    save_chat_approval_suggestion_choice, set_app_notification_choice, set_look_choice,
    HubSecurityState,
};
use std::sync::Mutex;

static STORAGE_LOCK: Mutex<()> = Mutex::new(());

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
    ipc::main_password::set_file_storage_key(Some([0x79; 32]));
    guard
}

#[test]
fn task_0796_one_reset_restores_all_29_gate_scoped_direct_reads() {
    let _serial = STORAGE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let dir = tempfile::tempdir().expect("temporary account");
    let _storage = use_storage(dir.path());
    let config_dir = Some(dir.path().to_owned());
    let core = HubCoreState::default();
    *core.osl.identity.lock().unwrap() = Some(keystore::generate_native_identity());
    let owner = core
        .osl
        .identity
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .user_id
        .clone();
    let security = HubSecurityState::default();

    cmd_osl_save_auto_whitelist_rule(
        &core.osl,
        "chat".to_owned(),
        "always".to_owned(),
        config_dir.clone(),
    )
    .unwrap();
    cmd_osl_save_new_friend_defaults(
        &core.osl,
        NewFriendDefaultsDto {
            account_reach: "all_shared_chats".to_owned(),
            auto_whitelist: "always".to_owned(),
            verification_warnings: "never".to_owned(),
        },
        config_dir.clone(),
    )
    .unwrap();
    cmd_osl_save_privacy_level_rule_set(
        &core.osl,
        PrivacyLevel::Maximum.id().to_owned(),
        config_dir.clone(),
    )
    .unwrap();
    cmd_osl_save_next_generation_message_policy(
        &core.osl,
        "on".to_owned(),
        config_dir.clone(),
    )
    .unwrap();
    {
        let mut prefs = core.osl.app_preferences.lock().unwrap();
        prefs.rn_wire_policy_requested = true;
    }
    core.osl.set_rn_wire_in_enabled(true);
    cmd_osl_save_verification_warning_choice(
        &core.osl,
        "never".to_owned(),
        config_dir.clone(),
    )
    .unwrap();
    cmd_osl_save_message_defaults(
        &core.osl,
        MessageDefaultsDto {
            burn_scope: "app".to_owned(),
            timer_seconds: 86_400,
            view_once_length_seconds: 45,
            cover_writing: "ai_covertext".to_owned(),
        },
        config_dir.clone(),
    )
    .unwrap();
    for name in BehaviourChoiceName::ALL {
        cmd_osl_save_behaviour_choice(
            &core.osl,
            name.label().to_owned(),
            format!("changed-{}", name.label()),
            config_dir.clone(),
        )
        .unwrap();
    }
    save_chat_approval_suggestion_choice(&security, "off".to_owned()).unwrap();
    set_app_notification_choice(&security, "discord".to_owned(), true).unwrap();
    for (name, _) in LOOK_STYLE_BINDINGS {
        set_look_choice(&security, name.to_owned(), format!("changed-{name}")).unwrap();
    }
    let png = format!(
        "data:image/png;base64,{}",
        STANDARD.encode(b"\x89PNG\r\n\x1a\n")
    );
    osl_privacy_hub::osl_profile::set_active_profile_picture(&owner, png).unwrap();

    let reset = reset_every_setting(&core, &security).unwrap();
    assert_eq!(reset.action, "reset");

    let restarted = HubCoreState::default();
    *restarted.osl.app_preferences.lock().unwrap() =
        ipc::app_preferences::load_app_preferences(&dir.path().join("app_preferences.json"));
    *restarted.osl.auto_whitelist_rules.lock().unwrap() =
        ipc::auto_whitelist_rules::load_auto_whitelist_rules(
            &dir.path().join("auto_whitelist_rules.json"),
        );

    let mut direct_reads = Vec::new();
    let auto_rule = cmd_osl_get_auto_whitelist_rule(&restarted.osl, "chat".to_owned()).unwrap();
    assert_eq!(auto_rule, "never");
    direct_reads.push(format!("auto_rule={auto_rule}"));
    let typed_auto_rule = restarted
        .osl
        .auto_whitelist_rules
        .lock()
        .unwrap()
        .query(AutoWhitelistAppKind::Chat)
        .saved_choice;
    assert_eq!(typed_auto_rule, None);

    let picture = osl_privacy_hub::osl_profile::read_active_profile_picture(&owner).unwrap();
    assert_eq!(picture.status, "image-absent");
    assert_eq!(picture.image, None);
    direct_reads.push(format!("friend_picture={}", picture.status));

    let friends = cmd_osl_get_new_friend_defaults(&restarted.osl).unwrap();
    assert_eq!(friends.account_reach, "approved_chats_only");
    assert_eq!(friends.auto_whitelist, "never");
    assert_eq!(friends.verification_warnings, "always");
    direct_reads.extend([
        format!("friend_account_reach={}", friends.account_reach),
        format!("friend_auto_rule={}", friends.auto_whitelist),
        format!("friend_warnings={}", friends.verification_warnings),
    ]);

    let privacy = cmd_osl_read_privacy_protection_choices(&restarted.osl).unwrap();
    assert_eq!(privacy.level, "balanced");
    assert_eq!(privacy.warnings, "before_send_warnings");
    assert_eq!(privacy.cleanup, "attachment_cleaning_plus_30_day_review");
    assert_eq!(privacy.app_exceptions, "app_exceptions_reviewed");
    assert_eq!(privacy.contact_rules, "verified_contacts_suggested");
    direct_reads.push(format!("privacy={}", privacy.level));

    let suggestion = chat_approval_suggestion_choice().unwrap();
    assert_eq!(suggestion.choice, "on");
    direct_reads.push(format!("chat_suggestion={}", suggestion.choice));
    let app_notice =
        app_notification_enabled_before_notice(&security, "discord".to_owned()).unwrap();
    assert!(!app_notice);
    assert!(list_app_notification_choices(&security).unwrap().is_empty());
    direct_reads.push(format!("app_notification={app_notice}"));

    let next_generation =
        cmd_osl_read_next_generation_message_policy(&restarted.osl).unwrap();
    assert_eq!(next_generation.choice, "off");
    assert!(!core.osl.rn_wire_in_enabled());
    assert!(!restarted
        .osl
        .app_preferences
        .lock()
        .unwrap()
        .rn_wire_policy_requested);
    direct_reads.push(format!("next_generation={}", next_generation.choice));

    let warning = cmd_osl_read_verification_warning_choice(&restarted.osl).unwrap();
    assert_eq!(warning.choice, "every time");
    direct_reads.push(format!("verification_warning={}", warning.choice));

    let burn_scope = cmd_osl_read_message_default_burn_scope(&restarted.osl).unwrap();
    let timer = cmd_osl_read_message_default_timer_seconds(&restarted.osl).unwrap();
    let view_once =
        cmd_osl_read_message_default_view_once_length_seconds(&restarted.osl).unwrap();
    let writing = cmd_osl_read_message_default_cover_writing(&restarted.osl).unwrap();
    assert_eq!(burn_scope, "message");
    assert_eq!(timer, 3_600);
    assert_eq!(view_once, 10);
    assert_eq!(writing, "plaintext");
    direct_reads.extend([
        format!("message_scope={burn_scope}"),
        format!("message_timer={timer}"),
        format!("view_once={view_once}"),
        format!("message_writer={writing}"),
    ]);

    for (name, _) in LOOK_STYLE_BINDINGS {
        assert_eq!(look_choice_value(&security, name.to_owned()).unwrap(), None);
        direct_reads.push(format!("look_{name}=unset"));
    }
    assert!(list_look_choices(&security).unwrap().is_empty());

    for name in BehaviourChoiceName::ALL {
        let value =
            cmd_osl_read_behaviour_choice(&restarted.osl, name.label().to_owned()).unwrap();
        assert_eq!(value.choice, "default");
        direct_reads.push(format!("behaviour_{}={}", name.label(), value.choice));
    }

    assert_eq!(direct_reads.len(), 29);
    println!(
        "TASK0796 action={} direct_read_count={} {} runtime_rn={} typed_auto_rule={:?}",
        reset.action,
        direct_reads.len(),
        direct_reads.join(" | "),
        core.osl.rn_wire_in_enabled(),
        typed_auto_rule,
    );
}
