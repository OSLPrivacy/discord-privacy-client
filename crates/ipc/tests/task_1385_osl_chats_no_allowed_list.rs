use ipc::commands::{
    cmd_osl_load_channel_history, cmd_osl_persist_inbound, cmd_osl_persist_outbound,
    cmd_osl_select_chat_server_channel, cmd_osl_set_guild_list, GuildDto,
};
use ipc::peer_map::PeerEntry;
use ipc::state::AppState;
use keystore::generate_identity;
use store::MessageStore;
use tempfile::TempDir;

const SERVER_ID: &str = "task-1385-server";
const CHANNEL_ID: &str = "task-1385-channel";
const SELF_ID: &str = "task-1385-self";
const MEMBER_ID: &str = "task-1385-member-no-allowed-list";

fn state_with_store(dir: &std::path::Path) -> AppState {
    let identity = generate_identity(SELF_ID.to_string());
    let secret_bytes: [u8; 32] = *identity.x25519_secret.as_bytes();
    let state = AppState::new();
    *state.identity.lock().unwrap() = Some(identity);
    *state.message_store.lock().unwrap() =
        Some(MessageStore::open(dir, &secret_bytes).expect("open message store"));
    state
}

#[test]
fn server_member_without_allowed_list_entry_reads_and_sends_in_osl_chats() {
    let tmp = TempDir::new().unwrap();
    let state = state_with_store(tmp.path());

    cmd_osl_set_guild_list(
        &state,
        vec![GuildDto {
            id: SERVER_ID.to_string(),
            name: "Task 1385 Server".to_string(),
            member_ids: vec![MEMBER_ID.to_string()],
            channel_ids: Vec::new(),
        }],
    )
    .expect("server member is listed without adding an allowed-list grant");
    let selected =
        cmd_osl_select_chat_server_channel(&state, SERVER_ID.to_string(), CHANNEL_ID.to_string())
            .expect("OSL Chats server/channel selection works without an allowed-list grant");
    state
        .scope_membership
        .lock()
        .unwrap()
        .note_server_channel_member(SERVER_ID, CHANNEL_ID, MEMBER_ID);
    state.peer_map.lock().unwrap().insert(
        MEMBER_ID.to_string(),
        PeerEntry {
            discord_id: Some(MEMBER_ID.to_string()),
            osl_user_id: Some(MEMBER_ID.to_string()),
            ..PeerEntry::default()
        },
    );

    let server = selected
        .servers
        .iter()
        .find(|server| server.id == SERVER_ID)
        .expect("selected server remains listed");
    let is_server_member = state
        .scope_membership
        .lock()
        .unwrap()
        .is_server_member(SERVER_ID, MEMBER_ID);
    let outgoing_whitelist_count = state
        .peer_map
        .lock()
        .unwrap()
        .get(MEMBER_ID)
        .map(|entry| entry.outgoing_whitelists.len())
        .unwrap_or_default();
    let whitelist_scope_count = state.whitelist_state.lock().unwrap().len();
    let allowed_places_db_exists = ipc::allowed_places::allowed_places_db_path(tmp.path()).exists();

    assert_eq!(server.member_ids, vec![MEMBER_ID.to_string()]);
    assert_eq!(server.channel_ids, vec![CHANNEL_ID.to_string()]);
    assert!(is_server_member);
    assert_eq!(outgoing_whitelist_count, 0);
    assert_eq!(whitelist_scope_count, 0);
    assert!(!allowed_places_db_exists);

    cmd_osl_persist_inbound(
        &state,
        CHANNEL_ID.to_string(),
        "task-1385-inbound".to_string(),
        MEMBER_ID.to_string(),
        "member can read into OSL Chats".to_string(),
    )
    .expect("server member inbound persists without an allowed-list grant");
    cmd_osl_persist_outbound(
        &state,
        CHANNEL_ID.to_string(),
        "task-1385-outbound".to_string(),
        "self can send in OSL Chats".to_string(),
        None,
    )
    .expect("server member send path persists without an allowed-list grant");

    let history =
        cmd_osl_load_channel_history(&state, CHANNEL_ID.to_string(), Some(10)).expect("history");
    let inbound = history
        .iter()
        .find(|row| row.discord_message_id == "task-1385-inbound")
        .expect("member inbound row is readable");
    let outbound = history
        .iter()
        .find(|row| row.discord_message_id == "task-1385-outbound")
        .expect("self outbound row is readable");

    println!("TASK1385_SERVER_MEMBER={is_server_member}");
    println!("TASK1385_OUTGOING_ALLOWED_LIST_ENTRIES={outgoing_whitelist_count}");
    println!("TASK1385_WHITELIST_SCOPE_COUNT={whitelist_scope_count}");
    println!("TASK1385_ALLOWED_PLACES_DB_EXISTS={allowed_places_db_exists}");
    println!("TASK1385_HISTORY_ROWS={}", history.len());
    println!("TASK1385_INBOUND_READ={}", inbound.plaintext);
    println!("TASK1385_OUTBOUND_SEND={}", outbound.plaintext);

    assert_eq!(inbound.sender_discord_id, MEMBER_ID);
    assert_eq!(inbound.plaintext, "member can read into OSL Chats");
    assert_eq!(outbound.sender_discord_id, SELF_ID);
    assert_eq!(outbound.plaintext, "self can send in OSL Chats");
}
