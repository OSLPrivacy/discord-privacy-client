use ipc::commands::{
    cmd_osl_burn_scope_data, cmd_osl_load_channel_history, cmd_osl_set_guild_list, GuildDto,
};
use ipc::state::AppState;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const SECRET: &[u8; 32] = &[0x13; 32];
const SERVER: &str = "SERVER-1383";
const OTHER: &str = "OTHER-1383";

fn channel(server: &str, index: usize) -> String {
    format!("{server}-channel-{index}")
}

fn marked_message(server: &str, channel_id: &str, index: usize) -> StoredMessage {
    StoredMessage {
        discord_message_id: format!("{server}-{channel_id}-marked-{index}"),
        channel_id: channel_id.to_string(),
        sender_discord_id: format!("{server}-sender"),
        sender_osl_user_id: format!("{server}-osl"),
        plaintext: format!("marked {server} message {index}"),
        decrypted_at: 1_700_001_383 + index as i64,
        burned: false,
    }
}

fn state_with_store(dir: &std::path::Path) -> AppState {
    let state = AppState::new();
    let store = MessageStore::open(dir, SECRET).expect("open message store");
    *state.message_store.lock().unwrap() = Some(store);
    state
}

fn put_marked_messages(state: &AppState, server: &str, channel_id: &str) {
    let guard = state.message_store.lock().unwrap();
    let store = guard.as_ref().expect("message store installed");
    for index in 1..=2 {
        store
            .put(&marked_message(server, channel_id, index))
            .expect("put marked message");
    }
}

fn marked_count(state: &AppState, channel_id: &str) -> usize {
    cmd_osl_load_channel_history(state, channel_id.to_string(), None)
        .expect("load channel history")
        .into_iter()
        .filter(|row| row.plaintext.starts_with("marked "))
        .count()
}

fn counts_for(state: &AppState, channels: &[String]) -> Vec<usize> {
    channels
        .iter()
        .map(|channel_id| marked_count(state, channel_id))
        .collect()
}

fn slash_counts(counts: &[usize]) -> String {
    counts
        .iter()
        .map(|count| count.to_string())
        .collect::<Vec<_>>()
        .join("/")
}

#[test]
fn task_1383_burn_scopes_cover_servers_and_channels() {
    let tmp = TempDir::new().unwrap();
    let state = state_with_store(tmp.path());
    let server_channels = vec![channel(SERVER, 1), channel(SERVER, 2), channel(SERVER, 3)];
    let other_channel = channel(OTHER, 1);

    cmd_osl_set_guild_list(
        &state,
        vec![
            GuildDto {
                id: SERVER.to_string(),
                name: SERVER.to_string(),
                member_ids: Vec::new(),
                channel_ids: server_channels.clone(),
            },
            GuildDto {
                id: OTHER.to_string(),
                name: OTHER.to_string(),
                member_ids: Vec::new(),
                channel_ids: vec![other_channel.clone()],
            },
        ],
    )
    .expect("install guild channel inventory");

    for channel_id in &server_channels {
        put_marked_messages(&state, SERVER, channel_id);
    }
    put_marked_messages(&state, OTHER, &other_channel);

    let start_server = counts_for(&state, &server_channels);
    let start_other = marked_count(&state, &other_channel);
    println!(
        "TASK-1383 start: {SERVER} marked counts={} {OTHER} marked count={}",
        slash_counts(&start_server),
        start_other
    );
    assert_eq!(start_server, vec![2, 2, 2]);
    assert_eq!(start_other, 2);

    let first_channel = &server_channels[0];
    let channel_burn = cmd_osl_burn_scope_data(
        &state,
        "server_channel".to_string(),
        format!("{SERVER}:{first_channel}"),
        Some(SERVER.to_string()),
    )
    .expect("burn first server channel");
    assert_eq!(channel_burn.rows_destroyed, 2);
    assert_eq!(channel_burn.channel_id, *first_channel);

    let after_channel_burn = counts_for(&state, &server_channels);
    let other_after_channel_burn = marked_count(&state, &other_channel);
    println!(
        "TASK-1383 after first-channel burn: {SERVER} marked counts={} {OTHER} marked count={}",
        slash_counts(&after_channel_burn),
        other_after_channel_burn
    );
    assert_eq!(after_channel_burn, vec![0, 2, 2]);
    assert_eq!(other_after_channel_burn, 2);

    let server_burn = cmd_osl_burn_scope_data(
        &state,
        "server_full".to_string(),
        SERVER.to_string(),
        Some(SERVER.to_string()),
    )
    .expect("burn whole server");
    assert_eq!(server_burn.rows_destroyed, 4);
    assert_eq!(server_burn.channel_id, SERVER);

    let after_server_burn = counts_for(&state, &server_channels);
    let other_after_server_burn = marked_count(&state, &other_channel);
    println!(
        "TASK-1383 after server burn: {SERVER} marked counts={} {OTHER} marked count={}",
        slash_counts(&after_server_burn),
        other_after_server_burn
    );
    assert_eq!(after_server_burn, vec![0, 0, 0]);
    assert_eq!(other_after_server_burn, 2);
}
