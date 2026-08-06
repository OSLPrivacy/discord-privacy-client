use ipc::commands::{
    cmd_osl_get_auto_whitelist_rule_choices, cmd_osl_get_whatsapp_whitelist_kinds,
    cmd_osl_read_auto_whitelist_rule, cmd_osl_save_auto_whitelist_rule,
};
use ipc::state::AppState;

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
