use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::app_preferences::{BehaviourChoiceName, PrivacyLevel};
use ipc::commands::{
    cmd_osl_get_auto_whitelist_rule, cmd_osl_get_new_friend_defaults,
    cmd_osl_read_behaviour_choice, cmd_osl_read_message_default_burn_scope,
    cmd_osl_read_message_default_cover_writing, cmd_osl_read_message_default_timer_seconds,
    cmd_osl_read_message_default_view_once_length_seconds,
    cmd_osl_read_next_generation_message_policy, cmd_osl_read_privacy_protection_choices,
    cmd_osl_read_verification_warning_choice, cmd_osl_save_auto_whitelist_rule,
    cmd_osl_save_behaviour_choice, cmd_osl_save_message_defaults, cmd_osl_save_new_friend_defaults,
    cmd_osl_save_next_generation_message_policy, cmd_osl_save_privacy_level_rule_set,
    cmd_osl_save_verification_warning_choice, MessageDefaultsDto, NewFriendDefaultsDto,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::look_window::LOOK_STYLE_BINDINGS;
use osl_privacy_hub::security::{
    app_notification_enabled_before_notice, chat_approval_suggestion_choice, look_choice_value,
    reset_setting_group, save_chat_approval_suggestion_choice, set_app_notification_choice,
    set_look_choice, HubSecurityState,
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
    ipc::main_password::set_file_storage_key(Some([0x86; 32]));
    guard
}

#[test]
fn task_0861_reset_one_named_group_defaults_only_that_groups_eight_settings() {
    let _serial = STORAGE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
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

    // Change every one of TASK 0860's 29 owned settings before resetting the
    // single named Look screen below.
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
    cmd_osl_save_next_generation_message_policy(&core.osl, "on".to_owned(), config_dir.clone())
        .unwrap();
    cmd_osl_save_verification_warning_choice(&core.osl, "never".to_owned(), config_dir.clone())
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
    osl_privacy_hub::osl_profile::set_active_profile_picture(&owner, png.clone()).unwrap();

    let reset = reset_setting_group(&core, &security, "Look".to_owned()).unwrap();
    assert_eq!(reset.action, "reset");
    assert_eq!(reset.group, "Look");
    assert_eq!(reset.settings_defaulted, 8);

    // Exactly Look's eight owned settings return to their default (unset).
    for (name, _) in LOOK_STYLE_BINDINGS {
        assert_eq!(
            look_choice_value(&security, name.to_owned()).unwrap(),
            None,
            "{name}"
        );
    }

    // Every changed setting owned by every other screen remains exactly as it
    // was before the named Look reset.
    assert_eq!(
        cmd_osl_get_auto_whitelist_rule(&core.osl, "chat".to_owned()).unwrap(),
        "always"
    );
    let friends = cmd_osl_get_new_friend_defaults(&core.osl).unwrap();
    assert_eq!(friends.account_reach, "all_shared_chats");
    assert_eq!(friends.auto_whitelist, "always");
    assert_eq!(friends.verification_warnings, "never");
    assert_eq!(
        osl_privacy_hub::osl_profile::read_active_profile_picture(&owner)
            .unwrap()
            .image,
        Some(png)
    );
    assert_eq!(
        cmd_osl_read_privacy_protection_choices(&core.osl)
            .unwrap()
            .level,
        "maximum"
    );
    assert_eq!(
        cmd_osl_read_verification_warning_choice(&core.osl)
            .unwrap()
            .choice,
        "never"
    );
    assert_eq!(chat_approval_suggestion_choice().unwrap().choice, "off");
    assert!(app_notification_enabled_before_notice(&security, "discord".to_owned()).unwrap());
    assert_eq!(
        cmd_osl_read_next_generation_message_policy(&core.osl)
            .unwrap()
            .choice,
        "on"
    );
    assert_eq!(
        cmd_osl_read_message_default_burn_scope(&core.osl).unwrap(),
        "app"
    );
    assert_eq!(
        cmd_osl_read_message_default_timer_seconds(&core.osl).unwrap(),
        86_400
    );
    assert_eq!(
        cmd_osl_read_message_default_view_once_length_seconds(&core.osl).unwrap(),
        45
    );
    assert_eq!(
        cmd_osl_read_message_default_cover_writing(&core.osl).unwrap(),
        "ai_covertext"
    );
    for name in BehaviourChoiceName::ALL {
        assert_eq!(
            cmd_osl_read_behaviour_choice(&core.osl, name.label().to_owned())
                .unwrap()
                .choice,
            format!("changed-{}", name.label())
        );
    }

    println!(
        "TASK0861 group={} changed_setting_count=29 reset_setting_count={} preserved_setting_count={}",
        reset.group,
        reset.settings_defaulted,
        29 - reset.settings_defaulted,
    );
}
