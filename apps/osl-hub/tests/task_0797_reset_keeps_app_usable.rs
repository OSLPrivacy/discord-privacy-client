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
    cmd_osl_save_verification_warning_choice, cmd_osl_start_direct_new_message_plan,
    MessageDefaultsDto, NewFriendDefaultsDto,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::look_window::{
    resolved_accent_choice, DEFAULT_ACCENT_NAME, LOOK_STYLE_BINDINGS,
};
use osl_privacy_hub::security::{
    app_notification_enabled_before_notice, chat_approval_suggestion_choice,
    list_app_notification_choices, list_look_choices, reset_every_setting,
    save_chat_approval_suggestion_choice, set_app_notification_choice, set_look_choice,
    HubSecurityState,
};
use std::path::Path;
use std::sync::Mutex;

static STORAGE_LOCK: Mutex<()> = Mutex::new(());
const CHANGED_TIMER_SECONDS: u32 = 17 * 60;
const DEFAULT_TIMER_SECONDS: u32 = 60 * 60;

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

fn use_storage(dir: &Path) -> StorageGuard {
    let guard = StorageGuard {
        previous_account: keystore::active_account_dir(),
        previous_key: ipc::main_password::get_file_storage_key(),
    };
    select_storage(dir);
    guard
}

fn select_storage(dir: &Path) {
    keystore::set_active_account_dir(Some(dir.to_owned()));
    ipc::main_password::set_file_storage_key(Some([0x97; 32]));
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct DefaultSnapshot {
    auto_rule: String,
    friend_account_reach: String,
    friend_auto_rule: String,
    friend_warnings: String,
    privacy: String,
    chat_suggestion: String,
    app_notification: bool,
    next_generation: String,
    verification_warning: String,
    message_scope: String,
    message_timer: u32,
    view_once: u32,
    message_writer: String,
    accent: String,
    look_choice_count: usize,
    behaviour_values: Vec<(String, String)>,
}

fn documented_defaults(state: &ipc::AppState, security: &HubSecurityState) -> DefaultSnapshot {
    let friends = cmd_osl_get_new_friend_defaults(state).expect("read friend defaults");
    let privacy = cmd_osl_read_privacy_protection_choices(state).expect("read privacy defaults");
    let behaviour_values = BehaviourChoiceName::ALL
        .into_iter()
        .map(|name| {
            let value = cmd_osl_read_behaviour_choice(state, name.label().to_owned())
                .expect("read behaviour default")
                .choice;
            (name.label().to_owned(), value)
        })
        .collect();

    DefaultSnapshot {
        auto_rule: cmd_osl_get_auto_whitelist_rule(state, "chat".to_owned())
            .expect("read automatic allow rule"),
        friend_account_reach: friends.account_reach,
        friend_auto_rule: friends.auto_whitelist,
        friend_warnings: friends.verification_warnings,
        privacy: privacy.level,
        chat_suggestion: chat_approval_suggestion_choice()
            .expect("read chat suggestion")
            .choice,
        app_notification: app_notification_enabled_before_notice(security, "discord".to_owned())
            .expect("read Discord notification choice"),
        next_generation: cmd_osl_read_next_generation_message_policy(state)
            .expect("read next-generation default")
            .choice,
        verification_warning: cmd_osl_read_verification_warning_choice(state)
            .expect("read verification warning default")
            .choice,
        message_scope: cmd_osl_read_message_default_burn_scope(state)
            .expect("read message scope default"),
        message_timer: cmd_osl_read_message_default_timer_seconds(state)
            .expect("read message timer default"),
        view_once: cmd_osl_read_message_default_view_once_length_seconds(state)
            .expect("read view-once default"),
        message_writer: cmd_osl_read_message_default_cover_writing(state)
            .expect("read message writer default"),
        accent: resolved_accent_choice(security).expect("resolve accent default"),
        look_choice_count: list_look_choices(security)
            .expect("list saved look choices")
            .len(),
        behaviour_values,
    }
}

fn change_every_resettable_setting(
    core: &HubCoreState,
    security: &HubSecurityState,
    dir: &Path,
    owner: &str,
) {
    let config_dir = Some(dir.to_owned());
    cmd_osl_save_auto_whitelist_rule(
        &core.osl,
        "chat".to_owned(),
        "always".to_owned(),
        config_dir.clone(),
    )
    .expect("change automatic allow rule");
    cmd_osl_save_new_friend_defaults(
        &core.osl,
        NewFriendDefaultsDto {
            account_reach: "all_shared_chats".to_owned(),
            auto_whitelist: "always".to_owned(),
            verification_warnings: "never".to_owned(),
        },
        config_dir.clone(),
    )
    .expect("change friend defaults");
    cmd_osl_save_privacy_level_rule_set(
        &core.osl,
        PrivacyLevel::Maximum.id().to_owned(),
        config_dir.clone(),
    )
    .expect("change privacy defaults");
    cmd_osl_save_next_generation_message_policy(&core.osl, "on".to_owned(), config_dir.clone())
        .expect("change next-generation default");
    {
        let mut prefs = core.osl.app_preferences.lock().unwrap();
        prefs.rn_wire_policy_requested = true;
    }
    core.osl.set_rn_wire_in_enabled(true);
    cmd_osl_save_verification_warning_choice(&core.osl, "never".to_owned(), config_dir.clone())
        .expect("change verification warnings");
    cmd_osl_save_message_defaults(
        &core.osl,
        MessageDefaultsDto {
            burn_scope: "app".to_owned(),
            timer_seconds: CHANGED_TIMER_SECONDS,
            view_once_length_seconds: 45,
            cover_writing: "ai_covertext".to_owned(),
        },
        config_dir.clone(),
    )
    .expect("change message defaults");
    for name in BehaviourChoiceName::ALL {
        cmd_osl_save_behaviour_choice(
            &core.osl,
            name.label().to_owned(),
            format!("changed-{}", name.label()),
            config_dir.clone(),
        )
        .expect("change behaviour setting");
    }
    save_chat_approval_suggestion_choice(security, "off".to_owned())
        .expect("change chat suggestion");
    set_app_notification_choice(security, "discord".to_owned(), true)
        .expect("change app notification");
    for (name, _) in LOOK_STYLE_BINDINGS {
        let value = if name == "accent" {
            "red".to_owned()
        } else {
            format!("changed-{name}")
        };
        set_look_choice(security, name.to_owned(), value).expect("change look setting");
    }
    let png = format!(
        "data:image/png;base64,{}",
        STANDARD.encode(b"\x89PNG\r\n\x1a\n")
    );
    osl_privacy_hub::osl_profile::set_active_profile_picture(owner, png)
        .expect("change profile picture");
}

#[test]
fn task_0797_reset_restart_fresh_profile_plan_and_home_prerequisites_stay_usable() {
    let _serial = STORAGE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let reset_dir = tempfile::tempdir().expect("reset profile directory");
    let fresh_dir = tempfile::tempdir().expect("fresh profile directory");
    let _storage = use_storage(reset_dir.path());

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

    let before = documented_defaults(&core.osl, &security);
    assert_eq!(before.message_timer, DEFAULT_TIMER_SECONDS);
    assert_eq!(before.accent, DEFAULT_ACCENT_NAME);
    change_every_resettable_setting(&core, &security, reset_dir.path(), &owner);
    assert_eq!(
        cmd_osl_read_message_default_timer_seconds(&core.osl).unwrap(),
        CHANGED_TIMER_SECONDS
    );
    assert_eq!(resolved_accent_choice(&security).unwrap(), "red");

    let reset = reset_every_setting(&core, &security).expect("reset every setting");
    assert_eq!(reset.action, "reset");
    assert!(reset.settings_defaulted);

    let restarted = HubCoreState::default();
    let reload =
        ipc::state_reload::reload_encrypted_state_after_unlock(&restarted.osl, reset_dir.path())
            .expect("restart reload");
    assert!(reload.app_prefs_loaded);
    assert!(reload.auto_whitelist_rules_loaded);
    assert!(
        reload.errors.is_empty(),
        "reload errors: {:?}",
        reload.errors
    );
    let restarted_defaults = documented_defaults(&restarted.osl, &HubSecurityState::default());
    assert_eq!(restarted_defaults, before);
    assert_eq!(restarted_defaults.message_timer, DEFAULT_TIMER_SECONDS);
    assert_ne!(restarted_defaults.message_timer, CHANGED_TIMER_SECONDS);
    assert_eq!(restarted_defaults.accent, DEFAULT_ACCENT_NAME);
    assert_ne!(restarted_defaults.accent, "red");
    assert_eq!(restarted_defaults.look_choice_count, 0);
    assert!(restarted_defaults
        .behaviour_values
        .iter()
        .all(|(_, value)| value == "default"));
    assert!(list_app_notification_choices(&HubSecurityState::default())
        .unwrap()
        .is_empty());

    let plan = cmd_osl_start_direct_new_message_plan(&restarted.osl)
        .expect("create protected-message plan after reset and restart");
    assert!(!plan.plan_id.is_empty());
    assert!(plan.plan_id.starts_with("plan-"));
    assert_eq!(plan.timer_seconds, DEFAULT_TIMER_SECONDS);

    select_storage(fresh_dir.path());
    let fresh = HubCoreState::default();
    let fresh_defaults = documented_defaults(&fresh.osl, &HubSecurityState::default());
    assert_eq!(fresh_defaults, restarted_defaults);

    println!(
        "TASK0797 changed_timer_seconds={} changed_timer_words=17_minutes changed_accent=red reset_action={} restarted_timer_seconds={} restarted_timer_words=1_hour restarted_accent={} fresh_timer_seconds={} fresh_accent={} defaults_equal={} plan_id={} plan_id_length={} behaviour_defaults={} look_choice_count={}",
        CHANGED_TIMER_SECONDS,
        reset.action,
        restarted_defaults.message_timer,
        restarted_defaults.accent.replace(' ', "_"),
        fresh_defaults.message_timer,
        fresh_defaults.accent.replace(' ', "_"),
        fresh_defaults == restarted_defaults,
        plan.plan_id,
        plan.plan_id.len(),
        restarted_defaults.behaviour_values.len(),
        restarted_defaults.look_choice_count,
    );
}
