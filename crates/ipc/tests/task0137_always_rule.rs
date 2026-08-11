use ipc::allowed_places::{
    add_allowed_place_record, count_allowed_place_records, list_allowed_place_records,
    read_allowed_place_record, AllowedPlaceRecord,
};
use ipc::app_preferences::load_app_preferences;
use ipc::commands::{
    cmd_osl_get_auto_whitelist_rule, cmd_osl_get_discord_whitelist_kinds,
    cmd_osl_save_auto_whitelist_rule_for_place,
};
use ipc::main_password::set_file_storage_key;
use ipc::provider_discovery::{
    observe_shipping_provider_discovery, ProviderDiscoveryEvent, SignedInWindowsProviderSession,
};
use ipc::AppState;
use std::sync::atomic::{AtomicU64, Ordering};

static NONCE: AtomicU64 = AtomicU64::new(0);

fn fresh(label: &str) -> String {
    let sequence = NONCE.fetch_add(1, Ordering::Relaxed);
    format!("task0137-{label}-{sequence}")
}

fn stored_ids(app_data_dir: &std::path::Path) -> Vec<String> {
    list_allowed_place_records(app_data_dir)
        .expect("read allowed-place store")
        .into_iter()
        .map(|record| record.stable_id)
        .collect()
}

fn assert_sentinels_unchanged(app_data_dir: &std::path::Path, sentinels: &[String]) {
    let records = stored_ids(app_data_dir);
    assert!(
        sentinels
            .iter()
            .all(|stable_id| records.contains(stable_id)),
        "every sentinel stable ID must remain in the allowed-place store"
    );
}

#[test]
fn shipping_windows_discovery_adds_once_for_the_saved_always_kind_only() {
    // The production preferences writer is encrypted.  Supply the test's
    // device key so the restarted state must decrypt the rule it saved.
    set_file_storage_key(Some([0x37; 32]));
    let dirs = tempfile::tempdir().expect("task 0137 temp dirs");
    let app_data_dir = dirs.path().join("allowed-store");
    let prefs_dir = dirs.path().join("preferences");
    std::fs::create_dir_all(&prefs_dir).expect("create preference store");

    // The concrete shipping connector, not a fixture or direct new-place
    // command, owns the account and derives every observed stable ID.
    let session = SignedInWindowsProviderSession::discord(fresh("signed-in-account"))
        .expect("signed-in Discord Windows provider session");
    let provider_kinds = session
        .provider()
        .supported_place_kinds()
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let command_kinds = cmd_osl_get_discord_whitelist_kinds()
        .expect("enumerate the shipping Discord allowed kinds")
        .into_iter()
        .map(|kind| kind.id)
        .collect::<Vec<_>>();
    assert_eq!(provider_kinds, command_kinds);
    assert_eq!(
        provider_kinds,
        vec![
            "direct_message".to_string(),
            "group_chat".to_string(),
            "server".to_string(),
            "server_channel".to_string(),
            "thread".to_string(),
        ]
    );

    // Pick the supported kind from that independently enumerated set.  The
    // selected rule is scoped; another supported kind remains at its default.
    let selected_kind = provider_kinds[provider_kinds.len() / 2].to_string();
    let wrong_kind = provider_kinds
        .iter()
        .find(|kind| selected_kind.as_str() != *kind)
        .expect("a distinct supported kind")
        .to_string();
    let saving_state = AppState::new();
    let saved = cmd_osl_save_auto_whitelist_rule_for_place(
        &saving_state,
        "discord".to_string(),
        selected_kind.clone(),
        "always".to_string(),
        Some(prefs_dir.clone()),
    )
    .expect("persist selected-kind always rule");
    assert_eq!(saved.choice, "always");
    assert_eq!(saved.app_kind, format!("discord:{selected_kind}"));

    // A new process state reads the durable saved setting; it cannot borrow the
    // state used to save the choice.
    let discovery_state = AppState::new();
    *discovery_state
        .app_preferences
        .lock()
        .expect("preferences lock") = load_app_preferences(&prefs_dir.join("app_preferences.json"));
    assert_eq!(
        cmd_osl_get_auto_whitelist_rule(&discovery_state, format!("discord:{selected_kind}"))
            .expect("read persisted selected-kind rule"),
        "always"
    );
    assert_eq!(
        cmd_osl_get_auto_whitelist_rule(&discovery_state, format!("discord:{wrong_kind}"))
            .expect("read distinct-kind rule"),
        "never"
    );

    // Sentinel records exercise the real allowed-place store and establish the
    // complete pre-discovery contents that must remain untouched.
    for kind in [&wrong_kind, "thread"] {
        let sentinel_id = fresh("sentinel");
        add_allowed_place_record(
            &app_data_dir,
            AllowedPlaceRecord::from_parts(
                "discord",
                session.account(),
                kind,
                format!("discord:{}:{kind}:{sentinel_id}", session.account()),
            ),
        )
        .expect("seed sentinel through allowed-place store");
    }
    let sentinels_before = stored_ids(&app_data_dir);
    let count_before = count_allowed_place_records(&app_data_dir).expect("count sentinels");
    assert_eq!(count_before, 2);

    let target_event = ProviderDiscoveryEvent {
        kind: selected_kind.clone(),
        provider_place_id: fresh("fresh-provider-place"),
        place_name: fresh("place-name"),
        person_name: fresh("person-name"),
    };

    // A connector deprived of the live provider session is a hard refusal and
    // cannot mutate the allowed store.
    let starved = observe_shipping_provider_discovery(
        &discovery_state,
        &app_data_dir,
        None,
        target_event.clone(),
    )
    .expect_err("a production discovery must require its signed-in session");
    assert!(starved.contains("signed-in Windows provider session"));
    assert_eq!(stored_ids(&app_data_dir), sentinels_before);

    let observed = observe_shipping_provider_discovery(
        &discovery_state,
        &app_data_dir,
        Some(&session),
        target_event.clone(),
    )
    .expect("production provider-discovery event");
    let count_after_first = count_allowed_place_records(&app_data_dir).expect("count added target");
    assert_eq!(observed.provider, "discord");
    assert!(observed.added);
    assert_eq!(observed.decision.status, "allowed");
    assert!(!observed.decision.prompt);
    assert_eq!(count_after_first - count_before, 1);
    let target = read_allowed_place_record(&app_data_dir, &observed.stable_id)
        .expect("read exact provider-derived stable ID")
        .expect("target must be present exactly by its stable ID");
    assert_eq!(target.app, "discord");
    assert_eq!(target.account, session.account());
    assert_eq!(target.kind, selected_kind);
    assert_sentinels_unchanged(&app_data_dir, &sentinels_before);

    // Delivery can repeat a provider event.  The connector must preserve the
    // exact single stable-ID record and never turn that retry into a prompt.
    let repeated = observe_shipping_provider_discovery(
        &discovery_state,
        &app_data_dir,
        Some(&session),
        target_event,
    )
    .expect("duplicate provider discovery is idempotent");
    let count_after_repeat = count_allowed_place_records(&app_data_dir).expect("count retry");
    assert!(!repeated.added);
    assert_eq!(repeated.stable_id, observed.stable_id);
    assert_eq!(repeated.decision.status, "already_allowed");
    assert!(!repeated.decision.prompt);
    assert_eq!(count_after_repeat, count_after_first);

    // An event of a different supported kind reaches the same production
    // connector, but its unsaved rule must not inherit the selected kind.
    let wrong = observe_shipping_provider_discovery(
        &discovery_state,
        &app_data_dir,
        Some(&session),
        ProviderDiscoveryEvent {
            kind: wrong_kind.clone(),
            provider_place_id: fresh("wrong-kind-place"),
            place_name: fresh("wrong-kind-name"),
            person_name: fresh("wrong-kind-person"),
        },
    )
    .expect("wrong-kind provider event is observed safely");
    let count_after_wrong = count_allowed_place_records(&app_data_dir).expect("count wrong kind");
    assert_eq!(wrong.decision.status, "unlisted");
    assert!(!wrong.decision.prompt);
    assert!(!wrong.added);
    assert_eq!(count_after_wrong, count_after_first);
    assert_sentinels_unchanged(&app_data_dir, &sentinels_before);

    println!(
        "TASK0137 provider=discord-windows signed_in_account={}",
        session.account()
    );
    println!("TASK0137 allowed_kinds={}", provider_kinds.join(","));
    println!(
        "TASK0137 selected_kind={selected_kind} saved_rule={}",
        saved.choice
    );
    println!("TASK0137 exact_stable_id={}", observed.stable_id);
    println!(
        "TASK0137 prompts first={} repeat={} wrong={}",
        observed.decision.prompt, repeated.decision.prompt, wrong.decision.prompt
    );
    println!("TASK0137 allowed_store before={count_before} after_first={count_after_first} after_repeat={count_after_repeat} after_wrong={count_after_wrong}");
    println!(
        "TASK0137 sentinels_unchanged=true wrong_kind={} wrong_status={}",
        wrong_kind, wrong.decision.status
    );
    set_file_storage_key(None);
}
