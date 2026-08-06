use ipc::commands::{
    cmd_osl_get_auto_whitelist_rule_choices, cmd_osl_get_discord_whitelist_kinds,
    cmd_osl_get_whatsapp_whitelist_kinds, cmd_osl_read_auto_whitelist_rule,
    cmd_osl_save_auto_whitelist_rule,
};
use ipc::state::AppState;
use std::process::Command;

#[test]
fn direct_rule_query_prints_each_valid_choice() {
    let choices = cmd_osl_get_auto_whitelist_rule_choices().unwrap();
    let labels: Vec<String> = choices.into_iter().map(|choice| choice.label).collect();
    println!("direct rule query valid choices: {}", labels.join(", "));
    assert_eq!(
        labels,
        vec!["never", "ask me", "always", "only if a friend"]
    );
}

#[test]
fn whatsapp_kinds_command_returns_exactly_three_named_kinds() {
    let kinds = cmd_osl_get_whatsapp_whitelist_kinds().unwrap();
    let ids: Vec<String> = kinds.iter().map(|kind| kind.id.clone()).collect();
    let names: Vec<String> = kinds.iter().map(|kind| kind.name.clone()).collect();

    println!("whatsapp whitelist kinds count: {}", kinds.len());
    println!("whatsapp whitelist kinds: {}", names.join(", "));

    assert_eq!(kinds.len(), 3);
    assert_eq!(ids, vec!["direct_message", "group_chat", "channel"]);
    assert_eq!(names, vec!["direct message", "group chat", "channel"]);
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
    let proof: Vec<String> = lookups
        .iter()
        .map(|rule| {
            let place = rule
                .allowed_place
                .as_ref()
                .expect("Discord kind rule carries allowed-place record");
            format!(
                "{}={} allowed_place={}:{}",
                rule.app_kind, rule.choice, place.app, place.kind
            )
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

    println!(
        "TASK0146_DISCORD_FIXTURE_PLACES created={} resolved={} kinds={} {}",
        saved.len(),
        resolved,
        lookups
            .iter()
            .map(|rule| rule
                .allowed_place
                .as_ref()
                .expect("Discord kind rule carries allowed-place record")
                .kind
                .as_str())
            .collect::<Vec<_>>()
            .join(","),
        proof.join(" | ")
    );

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
    let proof: Vec<String> = lookups
        .iter()
        .map(|rule| {
            let place = rule
                .allowed_place
                .as_ref()
                .expect("WhatsApp kind rule carries allowed-place record");
            format!(
                "{}={} allowed_place={}:{}",
                rule.app_kind, rule.choice, place.app, place.kind
            )
        })
        .collect();

    println!(
        "whatsapp kind direct lookup count={} {}",
        lookups.len(),
        proof.join(" | ")
    );

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
