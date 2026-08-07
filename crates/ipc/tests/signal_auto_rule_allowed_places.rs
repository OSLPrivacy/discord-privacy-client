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
