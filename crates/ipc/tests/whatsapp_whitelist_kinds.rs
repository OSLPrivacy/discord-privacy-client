use ipc::allowed_places::{add_allowed_place_record, allowed_place_is_allowed, AllowedPlaceQuery};
use ipc::commands::{
    cmd_osl_get_whatsapp_whitelist_kinds, cmd_osl_list_whatsapp_whitelist_kinds,
    cmd_osl_read_auto_whitelist_rule, cmd_osl_read_whatsapp_auto_whitelist_rule,
    cmd_osl_save_auto_whitelist_rule, cmd_osl_save_whatsapp_auto_whitelist_rule,
};
use ipc::state::AppState;
use tempfile::TempDir;

#[test]
fn whatsapp_kinds_command_returns_the_supported_named_kinds() {
    let kinds = cmd_osl_list_whatsapp_whitelist_kinds().expect("whatsapp whitelist kinds");
    let legacy_name =
        cmd_osl_get_whatsapp_whitelist_kinds().expect("legacy whatsapp whitelist kinds command");
    let names: Vec<&str> = kinds.iter().map(|kind| kind.name.as_str()).collect();
    println!(
        "TASK0155_WHATSAPP_KIND_LIST count={} legacy_count={} names={}",
        names.len(),
        legacy_name.len(),
        names.join(", ")
    );
    assert_eq!(kinds, legacy_name);
    assert_eq!(
        names,
        vec![
            "direct message",
            "group chat",
            "channel",
            "community",
            "community group"
        ]
    );
    assert_eq!(
        kinds
            .iter()
            .map(|kind| kind.auto_rule_app_kind.as_str())
            .collect::<Vec<_>>(),
        vec![
            "whatsapp:direct_message",
            "whatsapp:group_chat",
            "whatsapp:channel",
            "whatsapp:community",
            "whatsapp:community_group"
        ]
    );
    assert_eq!(
        kinds
            .iter()
            .map(|kind| kind.allowed_place_kind.as_str())
            .collect::<Vec<_>>(),
        vec![
            "direct_message",
            "group_chat",
            "channel",
            "community",
            "community_group"
        ]
    );
}

#[test]
fn whatsapp_generic_rule_keys_carry_allowed_place_metadata() {
    let state = AppState::new();
    let saved = [
        ("whatsapp:direct_message", "always", "direct_message"),
        ("whatsapp:group_chat", "ask me", "group_chat"),
        ("whatsapp:channel", "only if a friend", "channel"),
    ];

    for (rule_key, choice, _) in saved {
        let saved_rule =
            cmd_osl_save_auto_whitelist_rule(&state, rule_key.to_owned(), choice.to_owned(), None)
                .unwrap();
        assert_eq!(saved_rule.allowed_place.as_ref().unwrap().app, "whatsapp");
    }

    let lookups: Vec<_> = saved
        .iter()
        .map(|(rule_key, _, _)| {
            cmd_osl_read_auto_whitelist_rule(&state, (*rule_key).to_owned()).unwrap()
        })
        .collect();

    println!(
        "TASK0155_WHATSAPP_GENERIC_RULES count={} {}",
        lookups.len(),
        lookups
            .iter()
            .map(|rule| format!(
                "{}={} allowed_place={}:{}",
                rule.app_kind,
                rule.choice,
                rule.allowed_place.as_ref().unwrap().app,
                rule.allowed_place.as_ref().unwrap().kind
            ))
            .collect::<Vec<_>>()
            .join(" | ")
    );

    assert_eq!(lookups.len(), 3);
    for (lookup, (_, choice, allowed_place_kind)) in lookups.iter().zip(saved) {
        assert_eq!(lookup.choice, choice);
        assert_eq!(
            lookup
                .allowed_place
                .as_ref()
                .map(|place| place.kind.as_str()),
            Some(allowed_place_kind)
        );
    }
}

#[test]
fn task_0155_fixture_place_for_each_whatsapp_kind_resolves() {
    let state = AppState::new();
    let dir = TempDir::new().unwrap();
    let account = "whatsapp-account-0155".to_owned();
    let saved = [
        ("direct_message", "wa-peer-0155", "always"),
        ("group_chat", "wa-group-0155", "ask me"),
        ("channel", "wa-channel-0155", "only if a friend"),
        ("community", "wa-community-0155", "always"),
        ("community_group", "wa-community-group-0155", "ask me"),
    ];
    let mut resolved = Vec::new();

    for (kind, place, choice) in saved {
        let lookup = cmd_osl_save_whatsapp_auto_whitelist_rule(
            &state,
            kind.to_owned(),
            account.clone(),
            place.to_owned(),
            choice.to_owned(),
            None,
        )
        .expect("known WhatsApp kind resolves");
        add_allowed_place_record(dir.path(), lookup.allowed_place.clone()).unwrap();
        assert!(allowed_place_is_allowed(
            dir.path(),
            &AllowedPlaceQuery::from(lookup.allowed_place.clone())
        )
        .unwrap());
        println!(
            "TASK0155_WHATSAPP_FIXTURE_PLACE kind={} auto_rule={} allowed_place_kind={} stable_id={} choice={}",
            lookup.whatsapp_kind,
            lookup.auto_rule_app_kind,
            lookup.allowed_place.kind,
            lookup.allowed_place.stable_id,
            lookup.choice
        );
        resolved.push(lookup.whatsapp_kind);
    }

    let listed = ipc::allowed_places::list_allowed_place_records(dir.path()).unwrap();
    println!(
        "TASK0155_WHATSAPP_KIND_RESOLVE resolved_count={} resolved={} fixture_place_count={}",
        resolved.len(),
        resolved.join(","),
        listed.len()
    );
    assert_eq!(
        resolved,
        vec![
            "direct_message",
            "group_chat",
            "channel",
            "community",
            "community_group"
        ]
    );
    assert_eq!(listed.len(), 5);
}

#[test]
fn task_0155_public_post_is_refused_by_name() {
    let state = AppState::new();
    let refusal = cmd_osl_read_whatsapp_auto_whitelist_rule(
        &state,
        "public_post".to_owned(),
        "whatsapp-account-0155".to_owned(),
        "wa-public-post-0155".to_owned(),
    )
    .expect_err("public_post must be refused as a fourth WhatsApp kind");
    println!("TASK0155_PUBLIC_POST_REFUSAL name=public_post refusal={refusal}");
    assert!(refusal.contains("public_post"));
}

#[test]
#[ignore = "TASK 0155 negative control: run by exact name to prove public_post exits 1"]
fn task_0155_public_post_exits_1_by_name() {
    let state = AppState::new();
    cmd_osl_read_whatsapp_auto_whitelist_rule(
        &state,
        "public_post".to_owned(),
        "whatsapp-account-0155".to_owned(),
        "wa-public-post-0155".to_owned(),
    )
    .unwrap_or_else(|error| {
        panic!("TASK0155_PUBLIC_POST_EXIT_BY_NAME name=public_post refusal={error}")
    });
}
