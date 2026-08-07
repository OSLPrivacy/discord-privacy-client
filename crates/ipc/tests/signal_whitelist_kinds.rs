use ipc::commands::{cmd_osl_list_signal_whitelist_kinds, cmd_osl_read_signal_auto_whitelist_rule};
use ipc::state::AppState;

#[test]
fn signal_kinds_command_returns_exactly_three_named_kinds() {
    let kinds = cmd_osl_list_signal_whitelist_kinds().expect("signal whitelist kinds");
    let names: Vec<&str> = kinds.iter().map(|kind| kind.name).collect();
    println!(
        "TASK3741_SIGNAL_KIND_LIST count={} names={}",
        names.len(),
        names.join(", ")
    );
    assert_eq!(names, vec!["direct message", "group chat", "story"]);
    assert_eq!(
        kinds
            .iter()
            .map(|kind| kind.auto_rule_app_kind)
            .collect::<Vec<_>>(),
        vec!["signal_direct_message", "signal_group_chat", "signal_story"]
    );
    assert_eq!(
        kinds
            .iter()
            .map(|kind| kind.allowed_place_kind)
            .collect::<Vec<_>>(),
        vec!["direct_message", "group_chat", "story"]
    );
}

#[test]
fn task_3741_all_three_signal_kinds_resolve_and_fourth_is_refused_by_name() {
    let state = AppState::new();
    let account = "signal-account-3741".to_owned();
    let place = "signal-place-3741".to_owned();
    let mut resolved = Vec::new();

    for signal_kind in ["direct_message", "group_chat", "story"] {
        let lookup = cmd_osl_read_signal_auto_whitelist_rule(
            &state,
            signal_kind.to_owned(),
            account.clone(),
            place.clone(),
        )
        .expect("known Signal kind resolves");
        resolved.push(lookup.signal_kind);
    }

    let refusal = cmd_osl_read_signal_auto_whitelist_rule(
        &state,
        "invented_signal_place_kind_3741".to_owned(),
        account,
        place,
    )
    .expect_err("invented Signal kind must be refused by name");

    println!(
        "TASK3741_SIGNAL_KIND_RESOLVE resolved_count={} resolved={} invented_refusal={}",
        resolved.len(),
        resolved.join(","),
        refusal
    );
    assert_eq!(resolved, vec!["direct_message", "group_chat", "story"]);
    assert!(refusal.contains("invented_signal_place_kind_3741"));
}

#[test]
#[ignore = "TASK 3741 negative control: run by exact name to prove invented kind refusal"]
fn task_3741_fourth_invented_kind_exits_1_by_name() {
    let state = AppState::new();
    let account = "signal-account-3741".to_owned();
    let place = "signal-place-3741".to_owned();
    cmd_osl_read_signal_auto_whitelist_rule(
        &state,
        "invented_signal_place_kind_3741".to_owned(),
        account,
        place,
    )
    .unwrap_or_else(|error| {
        panic!(
            "TASK3741_INVENTED_KIND_EXIT_BY_NAME name=invented_signal_place_kind_3741 refusal={error}"
        )
    });
}

#[test]
fn task_1055_signal_story_against_kind_list_returns_allowed() {
    let kinds = cmd_osl_list_signal_whitelist_kinds().expect("signal whitelist kinds");
    let allowed = kinds
        .iter()
        .any(|kind| kind.id == "story" && kind.allowed_place_kind == "story");
    let result = if allowed { "allowed" } else { "refusal" };

    println!("TASK1055_SIGNAL_STORY_KIND_LIST result={result}");
    assert_eq!(result, "allowed");
}
