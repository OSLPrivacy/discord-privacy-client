use ipc::auto_whitelist_rules::{AutoWhitelistAppKind, AutoWhitelistChoice};
use ipc::commands::{
    cmd_osl_get_auto_whitelist_rule_choices, cmd_osl_get_discord_whitelist_kinds,
    cmd_osl_get_telegram_whitelist_kinds, cmd_osl_get_whatsapp_whitelist_kinds, cmd_osl_new_place,
    cmd_osl_query_auto_whitelist_rule, cmd_osl_read_auto_whitelist_rule,
    cmd_osl_save_auto_whitelist_rule,
};
use ipc::state::AppState;
use std::process::Command;

#[test]
fn direct_rule_query_prints_each_valid_choice() {
    let state = AppState::new();
    let saved = cmd_osl_save_auto_whitelist_rule(
        &state,
        "chat".to_string(),
        AutoWhitelistChoice::OnlyIfAFriend.label().to_string(),
        None,
    )
    .expect("saving an app-kind rule must succeed");
    assert_eq!(saved.app_kind, "chat");
    assert_eq!(saved.choice, AutoWhitelistChoice::OnlyIfAFriend.label());

    let query = cmd_osl_query_auto_whitelist_rule(&state, AutoWhitelistAppKind::Chat)
        .expect("direct rule query must succeed");
    assert_eq!(query.saved_choice, Some(AutoWhitelistChoice::OnlyIfAFriend));

    let labels: Vec<String> = cmd_osl_get_auto_whitelist_rule_choices()
        .unwrap()
        .into_iter()
        .map(|choice| choice.label)
        .collect();
    println!("direct rule query valid choices: {}", labels.join(", "));
    assert_eq!(
        labels,
        vec!["never", "ask me", "always", "only if a friend"]
    );
}

#[test]
fn save_and_read_returns_different_choices_per_app_kind() {
    let state = AppState::new();

    cmd_osl_save_auto_whitelist_rule(&state, "discord".to_string(), "always".to_string(), None)
        .unwrap();
    cmd_osl_save_auto_whitelist_rule(
        &state,
        "telegram".to_string(),
        "only if a friend".to_string(),
        None,
    )
    .unwrap();

    let discord = cmd_osl_read_auto_whitelist_rule(&state, "discord".to_string()).unwrap();
    let telegram = cmd_osl_read_auto_whitelist_rule(&state, "telegram".to_string()).unwrap();

    println!(
        "two app kinds saved choices: {}={}, {}={}",
        discord.app_kind, discord.choice, telegram.app_kind, telegram.choice
    );
    assert_eq!(discord.choice, "always");
    assert_eq!(telegram.choice, "only if a friend");
    assert_ne!(discord.choice, telegram.choice);
}

#[test]
fn direct_new_place_command_returns_unlisted_with_no_prompt_when_rule_is_never() {
    let state = AppState::new();
    cmd_osl_save_auto_whitelist_rule(&state, "discord".to_string(), "never".to_string(), None)
        .unwrap();
    let never_place = ipc::allowed_places::AllowedPlaceRecord::discord_direct_message(
        "900000000000000135",
        "900000000000000136",
    );
    let result = cmd_osl_new_place(&state, never_place, None).unwrap();

    println!(
        "TASK0135 direct_new_place rule={} result={} prompt={}",
        result.rule_choice, result.result, result.prompt
    );
    assert_eq!(result.app_kind, "discord");
    assert_eq!(result.rule_choice, "never");
    assert_eq!(
        result.stable_id,
        "discord:900000000000000135:direct_message:900000000000000136"
    );
    assert_eq!(result.result, "unlisted");
    assert!(!result.prompt);
}

#[test]
fn direct_new_place_command_allows_when_rule_is_always() {
    let state = AppState::new();
    let dir = tempfile::tempdir().unwrap();
    cmd_osl_save_auto_whitelist_rule(&state, "discord".to_string(), "always".to_string(), None)
        .unwrap();
    let place = ipc::allowed_places::AllowedPlaceRecord::discord_direct_message(
        "900000000000000135",
        "900000000000000136",
    );
    let result = cmd_osl_new_place(&state, place, Some(dir.path().to_path_buf())).unwrap();

    assert_eq!(result.app_kind, "discord");
    assert_eq!(result.rule_choice, "always");
    assert_eq!(result.result, "allowed");
    assert!(!result.prompt);
}

#[test]
fn whatsapp_kinds_command_returns_exactly_six_named_kinds() {
    let kinds = cmd_osl_get_whatsapp_whitelist_kinds().unwrap();
    let ids: Vec<String> = kinds.iter().map(|kind| kind.id.clone()).collect();
    let names: Vec<String> = kinds.iter().map(|kind| kind.name.clone()).collect();

    println!("whatsapp whitelist kinds count: {}", kinds.len());
    println!("whatsapp whitelist kinds: {}", names.join(", "));

    assert_eq!(kinds.len(), 6);
    assert_eq!(
        ids,
        vec![
            "direct_message",
            "group_chat",
            "channel",
            "community",
            "community_group",
            "broadcast_list"
        ]
    );
    assert_eq!(
        names,
        vec![
            "direct message",
            "group chat",
            "channel",
            "community",
            "community group",
            "broadcast list"
        ]
    );
}

#[test]
fn three_whatsapp_kind_rule_lookups_return_independently_saved_choices() {
    let state = AppState::new();
    let saved = [
        ("whatsapp:direct_message", "always"),
        ("whatsapp:group_chat", "ask me"),
        ("whatsapp:channel", "only if a friend"),
    ];

    for (rule_key, choice) in saved {
        cmd_osl_save_auto_whitelist_rule(&state, rule_key.to_string(), choice.to_string(), None)
            .unwrap();
    }

    let lookups: Vec<_> = saved
        .iter()
        .map(|(rule_key, _)| {
            cmd_osl_read_auto_whitelist_rule(&state, (*rule_key).to_string()).unwrap()
        })
        .collect();

    assert_eq!(lookups.len(), 3);
    assert_eq!(lookups[0].app_kind, "whatsapp:direct_message");
    assert_eq!(lookups[0].choice, "always");
    assert_eq!(
        lookups[0]
            .allowed_place
            .as_ref()
            .map(|place| place.kind.as_str()),
        Some("direct_message")
    );
    assert_eq!(lookups[1].app_kind, "whatsapp:group_chat");
    assert_eq!(lookups[1].choice, "ask me");
    assert_eq!(
        lookups[1]
            .allowed_place
            .as_ref()
            .map(|place| place.kind.as_str()),
        Some("group_chat")
    );
    assert_eq!(lookups[2].app_kind, "whatsapp:channel");
    assert_eq!(lookups[2].choice, "only if a friend");
    assert_eq!(
        lookups[2]
            .allowed_place
            .as_ref()
            .map(|place| place.kind.as_str()),
        Some("channel")
    );
}

#[test]
fn discord_kinds_command_returns_exactly_five_named_kinds() {
    let kinds = cmd_osl_get_discord_whitelist_kinds().unwrap();
    let ids: Vec<String> = kinds.iter().map(|kind| kind.id.clone()).collect();
    let names: Vec<String> = kinds.iter().map(|kind| kind.name.clone()).collect();

    println!("discord whitelist kinds count: {}", kinds.len());
    println!("discord whitelist kinds: {}", names.join(", "));

    assert_eq!(kinds.len(), 5);
    assert_eq!(
        ids,
        vec![
            "direct_message",
            "group_chat",
            "server",
            "server_channel",
            "thread"
        ]
    );
    assert_eq!(
        names,
        vec![
            "direct message",
            "group chat",
            "server",
            "server channel",
            "thread"
        ]
    );
}

#[test]
fn five_discord_kind_rule_lookups_return_independently_saved_choices() {
    let state = AppState::new();
    let saved = [
        ("discord:direct_message", "always"),
        ("discord:group_chat", "ask me"),
        ("discord:server", "only if a friend"),
        ("discord:server_channel", "never"),
        ("discord:thread", "always"),
    ];

    for (rule_key, choice) in saved {
        cmd_osl_save_auto_whitelist_rule(&state, rule_key.to_string(), choice.to_string(), None)
            .unwrap();
    }

    let lookups: Vec<_> = saved
        .iter()
        .map(|(rule_key, _)| {
            cmd_osl_read_auto_whitelist_rule(&state, (*rule_key).to_string()).unwrap()
        })
        .collect();

    assert_eq!(lookups.len(), 5);
    assert_eq!(lookups[0].app_kind, "discord:direct_message");
    assert_eq!(lookups[0].choice, "always");
    assert_eq!(
        lookups[0]
            .allowed_place
            .as_ref()
            .map(|place| place.kind.as_str()),
        Some("direct_message")
    );
    assert_eq!(lookups[1].app_kind, "discord:group_chat");
    assert_eq!(lookups[1].choice, "ask me");
    assert_eq!(
        lookups[1]
            .allowed_place
            .as_ref()
            .map(|place| place.kind.as_str()),
        Some("group_chat")
    );
    assert_eq!(lookups[2].app_kind, "discord:server");
    assert_eq!(lookups[2].choice, "only if a friend");
    assert_eq!(
        lookups[2]
            .allowed_place
            .as_ref()
            .map(|place| place.kind.as_str()),
        Some("server")
    );
    assert_eq!(lookups[3].app_kind, "discord:server_channel");
    assert_eq!(lookups[3].choice, "never");
    assert_eq!(
        lookups[3]
            .allowed_place
            .as_ref()
            .map(|place| place.kind.as_str()),
        Some("server_channel")
    );
    assert_eq!(lookups[4].app_kind, "discord:thread");
    assert_eq!(lookups[4].choice, "always");
    assert_eq!(
        lookups[4]
            .allowed_place
            .as_ref()
            .map(|place| place.kind.as_str()),
        Some("thread")
    );
}

#[test]
fn task_0146_discord_fixture_places_resolve_and_saved_messages_exits_1() {
    let state = AppState::new();
    let saved = [
        ("discord:direct_message", "never"),
        ("discord:group_chat", "ask me"),
        ("discord:server", "always"),
        ("discord:server_channel", "only if a friend"),
        ("discord:thread", "always"),
    ];

    for (rule_key, choice) in saved {
        cmd_osl_save_auto_whitelist_rule(&state, rule_key.to_string(), choice.to_string(), None)
            .unwrap();
    }

    let lookups: Vec<_> = saved
        .iter()
        .map(|(rule_key, _)| {
            cmd_osl_read_auto_whitelist_rule(&state, (*rule_key).to_string()).unwrap()
        })
        .collect();
    let resolved = lookups
        .iter()
        .filter(|rule| {
            rule.allowed_place
                .as_ref()
                .is_some_and(|place| place.app == "discord")
        })
        .count();

    assert_eq!(lookups.len(), 5);
    assert_eq!(resolved, 5);

    let output = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--ignored")
        .arg("--exact")
        .arg("task_0146_saved_messages_probe")
        .arg("--nocapture")
        .env("TASK0146_SAVED_MESSAGES_PROBE", "1")
        .output()
        .expect("run saved-messages rejection helper");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    print!("{stdout}");
    eprint!("{stderr}");

    assert_eq!(output.status.code(), Some(1));
    println!("TASK0146_SIXTH_KIND kind=saved_messages exit_code=1");
}

#[test]
#[ignore = "task 0146 helper: exits 1 only when saved_messages is refused"]
fn task_0146_saved_messages_probe() {
    if std::env::var_os("TASK0146_SAVED_MESSAGES_PROBE").is_none() {
        return;
    }
    let state = AppState::new();
    match cmd_osl_read_auto_whitelist_rule(&state, "discord:saved_messages".to_string()) {
        Ok(rule) => {
            println!(
                "TASK0146_SIXTH_KIND_UNEXPECTEDLY_RESOLVED app_kind={} choice={}",
                rule.app_kind, rule.choice
            );
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("TASK0146_SIXTH_KIND_REFUSED kind=saved_messages error={error}");
            std::process::exit(1);
        }
    }
}

#[test]
fn two_messenger_kind_rule_lookups_return_independently_saved_choices() {
    let state = AppState::new();
    let saved = [
        ("messenger:direct_message", "always"),
        ("messenger:group_chat", "ask me"),
    ];

    for (rule_key, choice) in saved {
        cmd_osl_save_auto_whitelist_rule(&state, rule_key.to_string(), choice.to_string(), None)
            .unwrap();
    }

    let lookups: Vec<_> = saved
        .iter()
        .map(|(rule_key, _)| {
            cmd_osl_read_auto_whitelist_rule(&state, (*rule_key).to_string()).unwrap()
        })
        .collect();

    assert_eq!(lookups.len(), 2);
    assert_eq!(lookups[0].app_kind, "messenger:direct_message");
    assert_eq!(lookups[0].choice, "always");
    assert_eq!(
        lookups[0]
            .allowed_place
            .as_ref()
            .map(|place| (place.app.as_str(), place.kind.as_str())),
        Some(("messenger", "direct_message"))
    );
    assert_eq!(lookups[1].app_kind, "messenger:group_chat");
    assert_eq!(lookups[1].choice, "ask me");
    assert_eq!(
        lookups[1]
            .allowed_place
            .as_ref()
            .map(|place| (place.app.as_str(), place.kind.as_str())),
        Some(("messenger", "group_chat"))
    );
    assert_ne!(lookups[0].choice, lookups[1].choice);
}

#[test]
fn task3740_telegram_kind_list_has_exactly_six_and_all_resolve_allowed_place() {
    let state = AppState::new();
    let kinds = cmd_osl_get_telegram_whitelist_kinds().unwrap();
    let ids: Vec<String> = kinds.iter().map(|kind| kind.id.clone()).collect();
    let names: Vec<String> = kinds.iter().map(|kind| kind.name.clone()).collect();

    println!("telegram whitelist kind count={}", kinds.len());
    println!("telegram whitelist kinds={}", ids.join(","));
    println!("telegram whitelist kind names={}", names.join(","));

    assert_eq!(kinds.len(), 6);
    assert_eq!(
        ids,
        vec![
            "direct_message",
            "group_chat",
            "channel",
            "public_post",
            "supergroup",
            "saved_messages"
        ]
    );

    let lookups: Vec<_> = ids
        .iter()
        .map(|id| {
            cmd_osl_read_auto_whitelist_rule(&state, format!("telegram:{id}"))
                .expect("telegram kind should normalize")
        })
        .collect();
    let allowed: Vec<String> = lookups
        .iter()
        .map(|rule| {
            let place = rule
                .allowed_place
                .as_ref()
                .expect("Telegram kind rule carries allowed-place record");
            format!("{}:{}", place.app, place.kind)
        })
        .collect();

    assert_eq!(allowed.len(), 6);
    assert_eq!(
        allowed,
        vec![
            "telegram:direct_message",
            "telegram:group_chat",
            "telegram:channel",
            "telegram:public_post",
            "telegram:supergroup",
            "telegram:saved_messages"
        ]
    );
}

#[test]
fn task3740_1027_and_1028_are_allowed_and_story_is_refused_by_name() {
    let state = AppState::new();
    let task1027 = cmd_osl_read_auto_whitelist_rule(&state, "telegram:supergroup".to_string())
        .expect("task 1027 supergroup should be allowed");
    let task1028 = cmd_osl_read_auto_whitelist_rule(&state, "telegram:saved_messages".to_string())
        .expect("task 1028 saved messages should be allowed");
    let story = cmd_osl_read_auto_whitelist_rule(&state, "telegram:story".to_string()).unwrap_err();

    assert_eq!(
        task1027
            .allowed_place
            .as_ref()
            .map(|place| (place.app.as_str(), place.kind.as_str())),
        Some(("telegram", "supergroup"))
    );
    assert_eq!(
        task1028
            .allowed_place
            .as_ref()
            .map(|place| (place.app.as_str(), place.kind.as_str())),
        Some(("telegram", "saved_messages"))
    );
    assert!(story.contains("story"), "{story}");
}
