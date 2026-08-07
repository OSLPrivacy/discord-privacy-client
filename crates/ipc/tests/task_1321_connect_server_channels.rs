use ipc::commands::cmd_osl_select_chat_server_channel;
use ipc::state::AppState;

const SERVER_ID: &str = "task-1321-server-identity";
const CHANNEL_ID: &str = "task-1321-channel-identity";

#[test]
fn direct_selection_action_lists_the_new_channel_under_its_server() {
    let state = AppState::new();

    let selected =
        cmd_osl_select_chat_server_channel(&state, SERVER_ID.to_owned(), CHANNEL_ID.to_owned())
            .expect("direct selection action creates and lists a Chats server channel");

    let server = selected
        .servers
        .iter()
        .find(|candidate| candidate.id == SERVER_ID)
        .expect("selected server is listed");

    println!("TASK1321_DIRECT_ACTION=cmd_osl_select_chat_server_channel");
    println!(
        "TASK1321_SELECTED_SERVER_ID={}",
        selected.selected.server_id
    );
    println!(
        "TASK1321_SELECTED_CHANNEL_ID={}",
        selected.selected.channel_id
    );
    println!("TASK1321_LISTED_SERVER_COUNT={}", selected.servers.len());
    println!(
        "TASK1321_LISTED_CHANNEL_COUNT_UNDER_SERVER={}",
        server.channel_ids.len()
    );
    println!(
        "TASK1321_LISTED_NEW_CHANNEL={}:{}",
        server.id, server.channel_ids[0]
    );

    assert_eq!(selected.selected.server_id, SERVER_ID);
    assert_eq!(selected.selected.channel_id, CHANNEL_ID);
    assert_eq!(selected.servers.len(), 1);
    assert_eq!(server.channel_ids, vec![CHANNEL_ID.to_owned()]);
}
