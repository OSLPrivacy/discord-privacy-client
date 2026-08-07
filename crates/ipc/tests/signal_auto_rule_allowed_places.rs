use ipc::allowed_places::{add_allowed_place_record, allowed_place_is_allowed, AllowedPlaceQuery};
use ipc::commands::{
    cmd_osl_read_signal_auto_whitelist_rule, cmd_osl_save_signal_auto_whitelist_rule,
};
use ipc::state::AppState;
use tempfile::TempDir;

#[test]
fn task_0151_two_signal_kind_lookups_return_independently_saved_choices() {
    let state = AppState::new();
    let dir = TempDir::new().unwrap();
    let account = "signal-account-0151";

    cmd_osl_save_signal_auto_whitelist_rule(
        &state,
        "direct message".to_owned(),
        account.to_owned(),
        "signal-peer-0151".to_owned(),
        "always".to_owned(),
        None,
    )
    .unwrap();
    cmd_osl_save_signal_auto_whitelist_rule(
        &state,
        "group chat".to_owned(),
        account.to_owned(),
        "signal-group-0151".to_owned(),
        "only if a friend".to_owned(),
        None,
    )
    .unwrap();
    cmd_osl_save_signal_auto_whitelist_rule(
        &state,
        "story".to_owned(),
        account.to_owned(),
        "signal-story-0151".to_owned(),
        "ask me".to_owned(),
        None,
    )
    .unwrap();

    let direct = cmd_osl_read_signal_auto_whitelist_rule(
        &state,
        "direct_message".to_owned(),
        account.to_owned(),
        "signal-peer-0151".to_owned(),
    )
    .unwrap();
    let group = cmd_osl_read_signal_auto_whitelist_rule(
        &state,
        "group_chat".to_owned(),
        account.to_owned(),
        "signal-group-0151".to_owned(),
    )
    .unwrap();
    let story = cmd_osl_read_signal_auto_whitelist_rule(
        &state,
        "story".to_owned(),
        account.to_owned(),
        "signal-story-0151".to_owned(),
    )
    .unwrap();

    add_allowed_place_record(dir.path(), direct.allowed_place.clone()).unwrap();
    add_allowed_place_record(dir.path(), group.allowed_place.clone()).unwrap();
    add_allowed_place_record(dir.path(), story.allowed_place.clone()).unwrap();

    assert_eq!(direct.signal_kind, "direct_message");
    assert_eq!(direct.auto_rule_app_kind, "signal_direct_message");
    assert_eq!(direct.allowed_place.app, "signal");
    assert_eq!(direct.allowed_place.kind, "direct_message");
    assert_eq!(
        direct.allowed_place.stable_id,
        "signal:signal-account-0151:direct_message:signal-peer-0151"
    );
    assert_eq!(direct.choice, "always");
    assert!(allowed_place_is_allowed(
        dir.path(),
        &AllowedPlaceQuery::from(direct.allowed_place.clone())
    )
    .unwrap());

    assert_eq!(group.signal_kind, "group_chat");
    assert_eq!(group.auto_rule_app_kind, "signal_group_chat");
    assert_eq!(group.allowed_place.app, "signal");
    assert_eq!(group.allowed_place.kind, "group_chat");
    assert_eq!(
        group.allowed_place.stable_id,
        "signal:signal-account-0151:group_chat:signal-group-0151"
    );
    assert_eq!(group.choice, "only if a friend");
    assert!(allowed_place_is_allowed(
        dir.path(),
        &AllowedPlaceQuery::from(group.allowed_place.clone())
    )
    .unwrap());

    assert_eq!(story.signal_kind, "story");
    assert_eq!(story.auto_rule_app_kind, "signal_story");
    assert_eq!(story.allowed_place.app, "signal");
    assert_eq!(story.allowed_place.kind, "story");
    assert_eq!(
        story.allowed_place.stable_id,
        "signal:signal-account-0151:story:signal-story-0151"
    );
    assert_eq!(story.choice, "ask me");
    assert!(allowed_place_is_allowed(
        dir.path(),
        &AllowedPlaceQuery::from(story.allowed_place.clone())
    )
    .unwrap());

    assert_ne!(direct.auto_rule_app_kind, group.auto_rule_app_kind);
    assert_ne!(direct.auto_rule_app_kind, story.auto_rule_app_kind);
    assert_ne!(group.auto_rule_app_kind, story.auto_rule_app_kind);
    assert_ne!(direct.allowed_place.kind, group.allowed_place.kind);
    assert_ne!(direct.allowed_place.kind, story.allowed_place.kind);
    assert_ne!(group.allowed_place.kind, story.allowed_place.kind);
    assert_ne!(direct.choice, group.choice);
    assert_ne!(direct.choice, story.choice);
    assert_ne!(group.choice, story.choice);

    println!(
        "TASK 0151 direct lookup: kind={} auto_rule={} allowed_place_kind={} stable_id={} choice={}",
        direct.signal_kind,
        direct.auto_rule_app_kind,
        direct.allowed_place.kind,
        direct.allowed_place.stable_id,
        direct.choice
    );
    println!(
        "TASK 0151 direct lookup: kind={} auto_rule={} allowed_place_kind={} stable_id={} choice={}",
        group.signal_kind,
        group.auto_rule_app_kind,
        group.allowed_place.kind,
        group.allowed_place.stable_id,
        group.choice
    );
    println!(
        "TASK 0151 direct lookup: kind={} auto_rule={} allowed_place_kind={} stable_id={} choice={}",
        story.signal_kind,
        story.auto_rule_app_kind,
        story.allowed_place.kind,
        story.allowed_place.stable_id,
        story.choice
    );
}

use ipc::commands::cmd_osl_list_signal_whitelist_kinds;
use std::process::{Command, Output};

fn run_fixture_place(kind: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_signal-fixture-place"))
        .arg(kind)
        .output()
        .expect("signal fixture-place helper must run")
}

#[test]
fn task_0152_signal_fixture_places_resolve_and_channel_exits_1() {
    let kinds = cmd_osl_list_signal_whitelist_kinds().expect("Signal kinds command");
    assert_eq!(kinds.len(), 3);

    let mut resolved_count = 0usize;
    for kind in kinds {
        let output = run_fixture_place(kind.id);
        let stdout = String::from_utf8(output.stdout).expect("fixture stdout is utf-8");
        let stderr = String::from_utf8(output.stderr).expect("fixture stderr is utf-8");
        assert!(
            output.status.success(),
            "Signal fixture place for {} failed: {stderr}",
            kind.id
        );
        assert!(
            stdout.contains("TASK 0152 fixture place resolved:"),
            "fixture place did not print a resolution line: {stdout}"
        );
        assert!(stdout.contains(&format!("kind={}", kind.id)));
        assert!(stdout.contains(&format!("auto_rule={}", kind.auto_rule_app_kind)));
        assert!(stdout.contains(&format!("allowed_place_kind={}", kind.allowed_place_kind)));
        print!("{stdout}");
        resolved_count += 1;
    }

    let rejected = run_fixture_place("channel");
    let rejected_code = rejected.status.code();
    let rejected_stderr =
        String::from_utf8(rejected.stderr).expect("fixture rejection stderr is utf-8");
    assert_eq!(rejected_code, Some(1), "{rejected_stderr}");
    assert!(
        rejected_stderr.contains("OSL: unknown Signal whitelist kind 'channel'"),
        "channel rejection did not name the refused kind: {rejected_stderr}"
    );
    println!(
        "TASK 0152 rejected extra kind: kind=channel exit_code={}",
        rejected_code.unwrap()
    );
    println!("TASK 0152 resolved Signal fixture places: {resolved_count}");
    assert_eq!(resolved_count, 2);
}
