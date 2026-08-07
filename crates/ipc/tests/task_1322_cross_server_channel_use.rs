use ipc::commands::{
    cmd_osl_count_server_channel_messages, cmd_osl_create_server_channel,
    cmd_osl_post_server_channel_message, cmd_osl_read_server_channel_message,
};
use ipc::state::AppState;

const RED_SERVER: &str = "Red";
const BLUE_SERVER: &str = "Blue";
const RED_CHANNEL: &str = "red-chat";
const MESSAGE_ID: &str = "maple-post";
const PLAINTEXT: &str = "MAPLE-4172";

#[test]
fn cross_server_channel_use_is_refused_without_mutating_the_red_channel() {
    let state = AppState::new();
    cmd_osl_create_server_channel(&state, RED_SERVER.to_owned(), RED_CHANNEL.to_owned())
        .expect("Red owns red-chat");

    let before_count = cmd_osl_count_server_channel_messages(
        &state,
        RED_SERVER.to_owned(),
        RED_CHANNEL.to_owned(),
    )
    .expect("count Red red-chat before send");
    println!("TASK1322_DIRECT_ACTION=cmd_osl_post_server_channel_message");
    println!("TASK1322_RED_CHAT_BEFORE_COUNT={before_count}");
    assert_eq!(before_count, 0);

    let posted = cmd_osl_post_server_channel_message(
        &state,
        RED_SERVER.to_owned(),
        RED_CHANNEL.to_owned(),
        MESSAGE_ID.to_owned(),
        PLAINTEXT.to_owned(),
    )
    .expect("Red can post to red-chat");
    println!(
        "TASK1322_POST server={} channel={} message_id={} plaintext={} channel_message_count={}",
        posted.server_id,
        posted.channel_id,
        posted.message_id,
        posted.plaintext,
        posted.channel_message_count
    );
    assert_eq!(posted.server_id, RED_SERVER);
    assert_eq!(posted.channel_id, RED_CHANNEL);
    assert_eq!(posted.message_id, MESSAGE_ID);
    assert_eq!(posted.plaintext, PLAINTEXT);
    assert_eq!(posted.channel_message_count, 1);

    let blue_refusal = cmd_osl_post_server_channel_message(
        &state,
        BLUE_SERVER.to_owned(),
        RED_CHANNEL.to_owned(),
        MESSAGE_ID.to_owned(),
        "SHOULD-NOT-POST".to_owned(),
    )
    .expect_err("changing only the server to Blue must be refused");
    println!("TASK1322_BLUE_REFUSAL={blue_refusal}");
    assert_eq!(
        blue_refusal,
        "OSL: channel 'red-chat' does not belong to server 'Blue'"
    );

    let read = cmd_osl_read_server_channel_message(
        &state,
        RED_SERVER.to_owned(),
        RED_CHANNEL.to_owned(),
        MESSAGE_ID.to_owned(),
    )
    .expect("maple-post still reads from Red red-chat");
    let final_count = cmd_osl_count_server_channel_messages(
        &state,
        RED_SERVER.to_owned(),
        RED_CHANNEL.to_owned(),
    )
    .expect("count Red red-chat after refused Blue send");
    println!(
        "TASK1322_READ message_id={} plaintext={} channel_message_count={}",
        read.message_id, read.plaintext, read.channel_message_count
    );
    println!("TASK1322_RED_CHAT_FINAL_COUNT={final_count}");

    assert_eq!(read.message_id, MESSAGE_ID);
    assert_eq!(read.plaintext, PLAINTEXT);
    assert_eq!(read.channel_message_count, 1);
    assert_eq!(final_count, 1);
}
