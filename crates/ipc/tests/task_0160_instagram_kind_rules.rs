use ipc::commands::{cmd_osl_read_auto_whitelist_rule, cmd_osl_save_auto_whitelist_rule};
use ipc::state::AppState;

#[test]
fn instagram_kind_rule_lookups_return_independently_saved_choices_for_the_full_list() {
    let state = AppState::new();
    let saved = [
        ("instagram:direct_message", "always"),
        ("instagram:group_chat", "ask me"),
        ("instagram:public_post", "only if a friend"),
        ("instagram:story", "always"),
        ("instagram:reel", "ask me"),
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
                .expect("Instagram kind rule carries allowed-place record");
            format!(
                "{}={} allowed_place={}:{}",
                rule.app_kind, rule.choice, place.app, place.kind
            )
        })
        .collect();

    println!(
        "TASK_0160_INSTAGRAM_KIND_RULES direct_lookup_count={} {}",
        lookups.len(),
        proof.join(" | ")
    );

    assert_eq!(lookups.len(), saved.len());
    assert_eq!(lookups[0].app_kind, "instagram:direct_message");
    assert_eq!(lookups[0].choice, "always");
    assert_eq!(
        lookups[0]
            .allowed_place
            .as_ref()
            .map(|place| (place.app.as_str(), place.kind.as_str())),
        Some(("instagram", "direct_message"))
    );
    assert_eq!(lookups[1].app_kind, "instagram:group_chat");
    assert_eq!(lookups[1].choice, "ask me");
    assert_eq!(
        lookups[1]
            .allowed_place
            .as_ref()
            .map(|place| (place.app.as_str(), place.kind.as_str())),
        Some(("instagram", "group_chat"))
    );
    assert_eq!(lookups[2].app_kind, "instagram:public_post");
    assert_eq!(lookups[2].choice, "only if a friend");
    assert_eq!(
        lookups[2]
            .allowed_place
            .as_ref()
            .map(|place| (place.app.as_str(), place.kind.as_str())),
        Some(("instagram", "public_post"))
    );
    assert_eq!(lookups[3].app_kind, "instagram:story");
    assert_eq!(lookups[3].choice, "always");
    assert_eq!(
        lookups[3]
            .allowed_place
            .as_ref()
            .map(|place| (place.app.as_str(), place.kind.as_str())),
        Some(("instagram", "story"))
    );
    assert_eq!(lookups[4].app_kind, "instagram:reel");
    assert_eq!(lookups[4].choice, "ask me");
    assert_eq!(
        lookups[4]
            .allowed_place
            .as_ref()
            .map(|place| (place.app.as_str(), place.kind.as_str())),
        Some(("instagram", "reel"))
    );
    assert_ne!(lookups[0].choice, lookups[1].choice);
    assert_ne!(lookups[1].choice, lookups[2].choice);
    assert_ne!(lookups[0].choice, lookups[2].choice);
}
