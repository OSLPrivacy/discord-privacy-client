use ipc::commands::{cmd_osl_create_server_channel, cmd_osl_get_guild_list};
use ipc::state::AppState;

const SERVER_ID: &str = "task-1320-server-identity";
const CHANNEL_ID: &str = "task-1320-channel-identity";

#[test]
fn direct_command_creates_one_channel_linked_to_its_server_identity() {
    let state = AppState::new();

    let created =
        cmd_osl_create_server_channel(&state, SERVER_ID.to_owned(), CHANNEL_ID.to_owned())
            .expect("direct command creates a server channel");

    let servers = cmd_osl_get_guild_list(&state).expect("direct read returns server list");
    let server = servers
        .iter()
        .find(|candidate| candidate.id == SERVER_ID)
        .expect("created channel is linked to its server identity");

    println!("TASK1320_COMMAND=cmd_osl_create_server_channel");
    println!("TASK1320_SERVER_ID={}", created.server_id);
    println!("TASK1320_CHANNEL_ID={}", created.channel_id);
    println!("TASK1320_SERVER_ROW_COUNT={}", servers.len());
    println!("TASK1320_CHANNEL_COUNT={}", server.channel_ids.len());
    println!(
        "TASK1320_LINKED_CHANNEL={}:{}",
        server.id, server.channel_ids[0]
    );

    assert_eq!(created.server_id, SERVER_ID);
    assert_eq!(created.channel_id, CHANNEL_ID);
    assert_eq!(servers.len(), 1);
    assert_eq!(server.channel_ids, vec![CHANNEL_ID.to_owned()]);

    cmd_osl_create_server_channel(&state, SERVER_ID.to_owned(), CHANNEL_ID.to_owned())
        .expect("stable channel identity remains idempotent");
    let servers = cmd_osl_get_guild_list(&state).expect("direct read returns stable server list");
    let server = servers
        .iter()
        .find(|candidate| candidate.id == SERVER_ID)
        .expect("server identity remains present");
    println!(
        "TASK1320_STABLE_CHANNEL_COUNT_AFTER_REPLAY={}",
        server.channel_ids.len()
    );
    assert_eq!(server.channel_ids, vec![CHANNEL_ID.to_owned()]);
}
