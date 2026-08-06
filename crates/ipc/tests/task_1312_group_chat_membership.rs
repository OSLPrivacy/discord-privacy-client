use ipc::commands::{cmd_osl_create_group_conversation, cmd_osl_membership_get};
use ipc::membership::gc_key;
use ipc::scope::ScopeKind;
use ipc::state::AppState;

#[test]
fn direct_command_creates_named_group_with_three_member_ids() {
    let state = AppState::new();
    let created = cmd_osl_create_group_conversation(
        &state,
        "Maple Group".to_owned(),
        vec![
            "member-ava-1312".to_owned(),
            "member-ben-1312".to_owned(),
            "member-cy-1312".to_owned(),
        ],
    )
    .expect("direct group creation command");

    assert_eq!(created.name, "Maple Group");
    assert_eq!(created.scope.kind, ScopeKind::Gc);
    assert_eq!(created.scope.id, created.group_id);
    assert_eq!(created.member_count, 3);
    assert_eq!(
        created.member_ids,
        vec!["member-ava-1312", "member-ben-1312", "member-cy-1312"]
    );

    let direct_membership =
        cmd_osl_membership_get(&state, created.group_id.clone()).expect("direct membership read");
    assert_eq!(direct_membership, created.member_ids);

    let stored_membership = state
        .scope_membership
        .lock()
        .expect("membership store")
        .members_for_key(&gc_key(&created.group_id));
    assert_eq!(stored_membership, created.member_ids);

    println!(
        "TASK_1312_GROUP_CHAT command=cmd_osl_create_group_conversation name=\"{}\" kind={:?} group_id={} member_count={} member_ids={} stored_member_count={}",
        created.name,
        created.scope.kind,
        created.group_id,
        created.member_count,
        created.member_ids.join(","),
        stored_membership.len()
    );
}
