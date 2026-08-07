use ipc::commands::{
    cmd_osl_add_server_member, cmd_osl_check_server_person_allowed,
    cmd_osl_set_server_person_permissions, cmd_osl_write_server_member_list,
};
use ipc::server_membership::ServerPermission;
use ipc::state::AppState;

const SERVER_ID: &str = "server-1377";
const OWNER: &str = "Ari Owner";
const OWNER_JOINED_AT: &str = "2026-08-06T09:00:00Z";
const PERSON: &str = "Bea Member";
const PERSON_JOINED_AT: &str = "2026-08-06T09:05:00Z";

#[test]
fn task1377_direct_command_grants_read_send_only_and_refuses_other_server_actions_by_name() {
    let state = AppState::new();
    cmd_osl_write_server_member_list(
        &state,
        SERVER_ID.to_owned(),
        OWNER.to_owned(),
        OWNER_JOINED_AT.to_owned(),
    )
    .expect("direct command writes owner row");
    cmd_osl_add_server_member(
        &state,
        SERVER_ID.to_owned(),
        PERSON.to_owned(),
        PERSON_JOINED_AT.to_owned(),
    )
    .expect("direct command adds person");

    let grant = cmd_osl_set_server_person_permissions(
        &state,
        SERVER_ID.to_owned(),
        PERSON.to_owned(),
        vec![ServerPermission::Read, ServerPermission::Send],
    )
    .expect("direct command gives read and send only");

    let successes: Vec<_> = [ServerPermission::Read, ServerPermission::Send]
        .into_iter()
        .map(|permission| {
            cmd_osl_check_server_person_allowed(
                &state,
                SERVER_ID.to_owned(),
                PERSON.to_owned(),
                permission,
            )
            .expect("read and send are allowed")
        })
        .collect();

    let refused_actions = [
        ServerPermission::MentionEveryone,
        ServerPermission::Invite,
        ServerPermission::MakeChannels,
        ServerPermission::RemoveMessages,
        ServerPermission::RemovePeople,
        ServerPermission::ChangeServer,
    ];
    let mut refusals = Vec::new();
    let mut unexpectedly_allowed = Vec::new();
    for permission in refused_actions {
        match cmd_osl_check_server_person_allowed(
            &state,
            SERVER_ID.to_owned(),
            PERSON.to_owned(),
            permission,
        ) {
            Ok(_) => unexpectedly_allowed.push(permission.name()),
            Err(error) => refusals.push((permission.name(), error)),
        }
    }

    let granted_names: Vec<_> = grant
        .permissions()
        .into_iter()
        .map(ServerPermission::name)
        .collect();
    let success_names: Vec<_> = successes
        .iter()
        .map(|success| success.permission_name.as_str())
        .collect();
    let refusal_names: Vec<_> = refusals.iter().map(|(name, _)| *name).collect();

    println!("TASK1377 direct_command=cmd_osl_set_server_person_permissions");
    println!("TASK1377 person={PERSON}");
    println!("TASK1377 granted={}", granted_names.join(","));
    println!("TASK1377 successes={}", successes.len());
    for success in &successes {
        println!(
            "TASK1377 success person={} action={}",
            success.person_name, success.permission_name
        );
    }
    println!("TASK1377 refusals={}", refusals.len());
    for (name, error) in &refusals {
        println!("TASK1377 refusal action={name} error={error}");
    }
    println!(
        "TASK1377 all_permission_names={}",
        ServerPermission::ALL
            .iter()
            .map(|permission| permission.name())
            .collect::<Vec<_>>()
            .join(",")
    );

    assert_eq!(ServerPermission::ALL.len(), 8);
    assert_eq!(granted_names, vec!["read", "send"]);
    assert_eq!(success_names, vec!["read", "send"]);
    assert!(
        unexpectedly_allowed.is_empty(),
        "actions should have been refused: {}",
        unexpectedly_allowed.join(",")
    );
    assert_eq!(
        refusal_names,
        vec![
            "mention-everyone",
            "invite",
            "make channels",
            "remove messages",
            "remove people",
            "change server"
        ]
    );
    assert_eq!(successes.len(), 2);
    assert_eq!(refusals.len(), 6);
    for (name, error) in refusals {
        assert_eq!(
            error,
            format!("OSL: {PERSON} is not allowed to {name}"),
            "refusal should name the person and action"
        );
    }
}
