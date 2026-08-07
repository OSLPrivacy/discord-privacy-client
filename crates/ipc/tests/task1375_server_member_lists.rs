use ipc::commands::{
    cmd_osl_add_server_member, cmd_osl_list_server_members, cmd_osl_remove_server_member_by_name,
    cmd_osl_write_server_member_list,
};
use ipc::state::AppState;

const SERVER_ID: &str = "server-1375";
const OWNER: &str = "Ari Owner";
const OWNER_JOINED_AT: &str = "2026-08-06T09:00:00Z";
const FIRST_MEMBER: &str = "Bea Member";
const FIRST_MEMBER_JOINED_AT: &str = "2026-08-06T09:05:00Z";
const SECOND_MEMBER: &str = "Cal Member";
const SECOND_MEMBER_JOINED_AT: &str = "2026-08-06T09:10:00Z";

#[test]
fn task1375_direct_commands_list_owner_members_join_times_and_refuse_owner_removal() {
    let state = AppState::new();

    let created = cmd_osl_write_server_member_list(
        &state,
        SERVER_ID.to_owned(),
        OWNER.to_owned(),
        OWNER_JOINED_AT.to_owned(),
    )
    .expect("direct command writes owner row");
    let count_before_duplicate = created.member_count();

    let first_add = cmd_osl_add_server_member(
        &state,
        SERVER_ID.to_owned(),
        FIRST_MEMBER.to_owned(),
        FIRST_MEMBER_JOINED_AT.to_owned(),
    )
    .expect("first add succeeds");
    let duplicate_add = cmd_osl_add_server_member(
        &state,
        SERVER_ID.to_owned(),
        FIRST_MEMBER.to_owned(),
        FIRST_MEMBER_JOINED_AT.to_owned(),
    )
    .expect("duplicate add is a no-op");
    let after_duplicate = cmd_osl_list_server_members(&state, SERVER_ID.to_owned())
        .expect("direct command lists members after duplicate add");
    let duplicate_count_delta = after_duplicate.member_count() - count_before_duplicate;

    let second_add = cmd_osl_add_server_member(
        &state,
        SERVER_ID.to_owned(),
        SECOND_MEMBER.to_owned(),
        SECOND_MEMBER_JOINED_AT.to_owned(),
    )
    .expect("second member add succeeds");

    let listed = cmd_osl_list_server_members(&state, SERVER_ID.to_owned())
        .expect("direct command lists final members");
    let members = listed.members();
    let owner = members
        .iter()
        .find(|member| member.owner)
        .expect("owner is listed");
    let non_owner_members: Vec<_> = members.iter().filter(|member| !member.owner).collect();

    let owner_removal_refusal =
        cmd_osl_remove_server_member_by_name(&state, SERVER_ID.to_owned(), OWNER.to_owned())
            .expect_err("owner removal must be refused");

    println!("TASK1375 direct_command=cmd_osl_list_server_members");
    println!("TASK1375 server_id={}", listed.server_id);
    println!(
        "TASK1375 owner={} joined_at={}",
        owner.name, owner.joined_at
    );
    for member in &non_owner_members {
        println!(
            "TASK1375 member={} joined_at={}",
            member.name, member.joined_at
        );
    }
    println!(
        "TASK1375 owner_count={} member_count={}",
        usize::from(owner.owner),
        non_owner_members.len()
    );
    println!(
        "TASK1375 duplicate_add.first_delta={} second_delta={} count_delta={}",
        first_add, duplicate_add, duplicate_count_delta
    );
    println!("TASK1375 second_member_delta={second_add}");
    println!("TASK1375 remove_owner_refusal={owner_removal_refusal}");
    println!("TASK1375 remove_owner_refused_by_name={OWNER}");

    assert_eq!(owner.name, OWNER);
    assert_eq!(owner.joined_at, OWNER_JOINED_AT);
    assert_eq!(non_owner_members.len(), 2);
    assert_eq!(non_owner_members[0].name, FIRST_MEMBER);
    assert_eq!(non_owner_members[0].joined_at, FIRST_MEMBER_JOINED_AT);
    assert_eq!(non_owner_members[1].name, SECOND_MEMBER);
    assert_eq!(non_owner_members[1].joined_at, SECOND_MEMBER_JOINED_AT);
    assert_eq!(first_add, 1);
    assert_eq!(duplicate_add, 0);
    assert_eq!(duplicate_count_delta, 1);
    assert_eq!(second_add, 1);
    assert_eq!(
        owner_removal_refusal,
        format!("OSL: cannot remove server owner {OWNER}")
    );
}
