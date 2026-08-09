use ipc::allowed_places::{add_allowed_place_record, allowed_place_is_allowed, AllowedPlaceQuery};
use ipc::commands::{
    cmd_osl_get_whatsapp_whitelist_kinds, cmd_osl_save_whatsapp_auto_whitelist_rule,
};
use ipc::state::AppState;
use tempfile::TempDir;

const COMMUNITY_FIXTURE: &str = "task-1089a-whatsapp-community";
const COMMUNITY_GROUP_FIXTURE: &str = "task-1089a-whatsapp-community-group";

#[test]
fn task_1089a_directly_inspects_only_the_allowed_whatsapp_community_fixture() {
    let kinds =
        cmd_osl_get_whatsapp_whitelist_kinds().expect("inspect WhatsApp allow-list controls");
    let community_controls = kinds
        .iter()
        .filter(|kind| kind.id == "community")
        .collect::<Vec<_>>();
    assert_eq!(
        community_controls.len(),
        1,
        "exactly one control may return the community kind"
    );
    let community = community_controls[0];
    assert_eq!(community.name, "community");
    assert_eq!(community.auto_rule_app_kind, "whatsapp:community");
    assert_eq!(community.allowed_place_kind, "community");

    let state = AppState::new();
    let store = TempDir::new().expect("create isolated allowed-place store");
    let allowed = cmd_osl_save_whatsapp_auto_whitelist_rule(
        &state,
        community.id.clone(),
        "whatsapp-account-1089a".to_owned(),
        COMMUNITY_FIXTURE.to_owned(),
        "always".to_owned(),
        None,
    )
    .expect("community fixture must return its allowed controls");
    add_allowed_place_record(store.path(), allowed.allowed_place.clone())
        .expect("store allowed community");
    assert!(allowed_place_is_allowed(
        store.path(),
        &AllowedPlaceQuery::from(allowed.allowed_place.clone())
    )
    .expect("read allowed community"));
    assert_eq!(allowed.whatsapp_kind, "community");
    assert_eq!(allowed.auto_rule_app_kind, "whatsapp:community");
    assert_eq!(allowed.allowed_place.kind, "community");

    let community_group = cmd_osl_save_whatsapp_auto_whitelist_rule(
        &state,
        "community_group".to_owned(),
        "whatsapp-account-1089a".to_owned(),
        COMMUNITY_GROUP_FIXTURE.to_owned(),
        "always".to_owned(),
        None,
    )
    .expect("community-group fixture must be inspectable");
    assert_eq!(community_group.whatsapp_kind, "community_group");
    assert_ne!(community_group.whatsapp_kind, allowed.whatsapp_kind);
    assert_ne!(
        community_group.allowed_place.kind,
        allowed.allowed_place.kind
    );

    let other_fixture_community_count = [
        (COMMUNITY_FIXTURE, allowed.whatsapp_kind.as_str()),
        (
            COMMUNITY_GROUP_FIXTURE,
            community_group.whatsapp_kind.as_str(),
        ),
        ("task-1089a-whatsapp-direct-message", "direct_message"),
        ("task-1089a-whatsapp-group-chat", "group_chat"),
        ("task-1089a-whatsapp-channel", "channel"),
    ]
    .into_iter()
    .filter(|(fixture, kind)| *fixture != COMMUNITY_FIXTURE && *kind == "community")
    .count();
    assert_eq!(other_fixture_community_count, 0);

    println!("TASK1089A_COMMUNITY_FIXTURE={COMMUNITY_FIXTURE}");
    println!("TASK1089A_PLACE_KIND={}", allowed.whatsapp_kind);
    println!(
        "TASK1089A_COMMUNITY_CONTROLS=id:{} name:{} auto_rule:{} allowed_place:{}",
        community.id, community.name, community.auto_rule_app_kind, community.allowed_place_kind
    );
    println!(
        "TASK1089A_COMMUNITY_GROUP_FIXTURE={COMMUNITY_GROUP_FIXTURE} kind={}",
        community_group.whatsapp_kind
    );
    println!("TASK1089A_OTHER_FIXTURES_WITH_COMMUNITY_KIND={other_fixture_community_count}");
}
