use ipc::commands::{
    cmd_osl_accept_voice_join, cmd_osl_accept_voice_speak_packet, cmd_osl_add_server_member,
    cmd_osl_check_server_person_allowed, cmd_osl_set_server_person_permissions,
    cmd_osl_set_server_person_timed_out, cmd_osl_write_server_member_list,
};
use ipc::server_membership::ServerPermission;
use ipc::state::AppState;

const SERVER_ID: &str = "server-4877";
const ROOM_ID: &str = "voice-room-4877";
const OWNER: &str = "Ari Owner";
const MEMBER: &str = "Bea Voice";

#[test]
fn task4877_voice_actions_use_text_permission_resolver_and_timeout_denies_media() {
    let state = AppState::new();
    cmd_osl_write_server_member_list(
        &state,
        SERVER_ID.to_owned(),
        OWNER.to_owned(),
        "2026-08-07T09:00:00Z".to_owned(),
    )
    .expect("owner row can be written");
    cmd_osl_add_server_member(
        &state,
        SERVER_ID.to_owned(),
        MEMBER.to_owned(),
        "2026-08-07T09:05:00Z".to_owned(),
    )
    .expect("voice member can be added");

    let voice_permissions = [
        ServerPermission::JoinVoice,
        ServerPermission::SpeakVoice,
        ServerPermission::MoveVoicePeople,
        ServerPermission::DisconnectVoicePeople,
    ];
    cmd_osl_set_server_person_permissions(
        &state,
        SERVER_ID.to_owned(),
        MEMBER.to_owned(),
        vec![
            ServerPermission::Read,
            ServerPermission::Send,
            ServerPermission::JoinVoice,
            ServerPermission::SpeakVoice,
            ServerPermission::MoveVoicePeople,
            ServerPermission::DisconnectVoicePeople,
        ],
    )
    .expect("same grant list saves text and voice permissions");

    let text_resolutions: Vec<_> = [ServerPermission::Read, ServerPermission::Send]
        .into_iter()
        .map(|permission| {
            cmd_osl_check_server_person_allowed(
                &state,
                SERVER_ID.to_owned(),
                MEMBER.to_owned(),
                permission,
            )
            .expect("text permission resolves through server permission store")
        })
        .collect();
    let voice_resolutions: Vec<_> = voice_permissions
        .into_iter()
        .map(|permission| {
            cmd_osl_check_server_person_allowed(
                &state,
                SERVER_ID.to_owned(),
                MEMBER.to_owned(),
                permission,
            )
            .expect("voice permission resolves through server permission store")
        })
        .collect();

    let accepted_join_before_timeout = cmd_osl_accept_voice_join(
        &state,
        SERVER_ID.to_owned(),
        ROOM_ID.to_owned(),
        MEMBER.to_owned(),
    )
    .expect("join voice is accepted before timeout");
    let accepted_speak_before_timeout = cmd_osl_accept_voice_speak_packet(
        &state,
        SERVER_ID.to_owned(),
        ROOM_ID.to_owned(),
        MEMBER.to_owned(),
    )
    .expect("speak packet is accepted before timeout");

    cmd_osl_set_server_person_timed_out(&state, SERVER_ID.to_owned(), MEMBER.to_owned())
        .expect("member can be timed out");
    let timed_out_join_accepts = usize::from(
        cmd_osl_accept_voice_join(
            &state,
            SERVER_ID.to_owned(),
            ROOM_ID.to_owned(),
            MEMBER.to_owned(),
        )
        .is_ok(),
    );
    let timed_out_speak_packets_accepted = [
        cmd_osl_accept_voice_speak_packet(
            &state,
            SERVER_ID.to_owned(),
            ROOM_ID.to_owned(),
            MEMBER.to_owned(),
        ),
        cmd_osl_accept_voice_speak_packet(
            &state,
            SERVER_ID.to_owned(),
            ROOM_ID.to_owned(),
            MEMBER.to_owned(),
        ),
    ]
    .into_iter()
    .filter(Result::is_ok)
    .count();

    let voice_names: Vec<_> = voice_resolutions
        .iter()
        .map(|resolution| resolution.permission_name.as_str())
        .collect();
    println!(
        "TASK4877 text_permissions_resolved_through_same_resolver={}",
        text_resolutions.len()
    );
    println!(
        "TASK4877 voice_permissions_resolved_through_same_resolver={}",
        voice_resolutions.len()
    );
    println!("TASK4877 voice_permission_names={}", voice_names.join("|"));
    println!(
        "TASK4877 accepted_before_timeout join={} speak_packets={}",
        usize::from(accepted_join_before_timeout.accepted),
        usize::from(accepted_speak_before_timeout.accepted)
    );
    println!("TASK4877 timed_out_voice_joins_accepted={timed_out_join_accepts}");
    println!("TASK4877 timed_out_voice_speak_packets_accepted={timed_out_speak_packets_accepted}");

    assert_eq!(text_resolutions.len(), 2);
    assert_eq!(voice_resolutions.len(), 4);
    assert_eq!(
        voice_names,
        vec![
            "join voice",
            "speak in voice",
            "move people in voice",
            "disconnect people from voice"
        ]
    );
    assert_eq!(timed_out_join_accepts, 0);
    assert_eq!(timed_out_speak_packets_accepted, 0);
}
