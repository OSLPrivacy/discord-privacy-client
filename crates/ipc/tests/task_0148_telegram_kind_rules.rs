use ipc::commands::{
    cmd_osl_read_telegram_auto_whitelist_rule, cmd_osl_save_telegram_auto_whitelist_rule,
};
use ipc::main_password::set_file_storage_key;
use ipc::state::AppState;
use std::sync::Mutex;

static KEY_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn task_0148_four_telegram_kind_lookups_return_independently_saved_choices() {
    let _guard = KEY_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    set_file_storage_key(Some([0x48u8; 32]));

    let dir = tempfile::tempdir().expect("tempdir");
    let account = "telegram-account-0148";
    let cases = [
        ("direct_message", "tg-dm-0148", "never"),
        ("group_chat", "tg-group-0148", "ask me"),
        ("channel", "tg-channel-0148", "always"),
        ("public_post", "tg-post-0148", "only if a friend"),
    ];

    let saving_state = AppState::new();
    for (kind, place_id, choice) in cases {
        cmd_osl_save_telegram_auto_whitelist_rule(
            &saving_state,
            account.to_string(),
            kind.to_string(),
            place_id.to_string(),
            choice.to_string(),
            Some(dir.path().to_path_buf()),
        )
        .expect("save telegram auto-whitelist rule");
    }

    let loaded_state = AppState::new();
    *loaded_state.app_preferences.lock().unwrap() =
        ipc::app_preferences::load_app_preferences(&dir.path().join("app_preferences.json"));

    let mut direct_lookups = 0usize;
    let mut choices = Vec::new();
    for (index, (kind, place_id, expected_choice)) in cases.into_iter().enumerate() {
        let read = cmd_osl_read_telegram_auto_whitelist_rule(
            &loaded_state,
            account.to_string(),
            kind.to_string(),
            place_id.to_string(),
        )
        .expect("read telegram auto-whitelist rule");
        direct_lookups += 1;

        println!(
            "TASK0148 telegram_kind_lookup.{index}.rule_lookup={}",
            read.rule_lookup
        );
        println!(
            "TASK0148 telegram_kind_lookup.{index}.choice={}",
            read.choice
        );
        println!(
            "TASK0148 telegram_kind_lookup.{index}.allowed_place.app={}",
            read.allowed_place.app
        );
        println!(
            "TASK0148 telegram_kind_lookup.{index}.allowed_place.account={}",
            read.allowed_place.account
        );
        println!(
            "TASK0148 telegram_kind_lookup.{index}.allowed_place.kind={}",
            read.allowed_place.kind
        );
        println!(
            "TASK0148 telegram_kind_lookup.{index}.allowed_place.stable_id={}",
            read.allowed_place.stable_id
        );

        assert_eq!(read.choice, expected_choice);
        assert_eq!(read.rule_lookup, format!("telegram:{kind}"));
        assert_eq!(read.allowed_place.app, "telegram");
        assert_eq!(read.allowed_place.account, account);
        assert_eq!(read.allowed_place.kind, kind);
        assert_eq!(
            read.allowed_place.stable_id,
            format!("telegram:{account}:{kind}:{place_id}")
        );
        choices.push(read.choice);
    }

    choices.sort();
    choices.dedup();
    println!(
        "TASK0148 telegram_kind_lookup.direct_lookup_count={}",
        direct_lookups
    );
    println!(
        "TASK0148 telegram_kind_lookup.independent_choice_count={}",
        choices.len()
    );
    assert_eq!(direct_lookups, 4);
    assert_eq!(choices.len(), 4);

    set_file_storage_key(None);
}
