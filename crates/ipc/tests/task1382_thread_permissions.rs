use std::collections::BTreeSet;

use ipc::commands::{
    cmd_osl_add_server_member, cmd_osl_add_thread_message, cmd_osl_create_thread,
    cmd_osl_list_thread_effective_readers, cmd_osl_read_thread,
    cmd_osl_set_limited_channel_members, cmd_osl_set_thread_permissions,
    cmd_osl_write_server_member_list,
};
use ipc::state::AppState;

const SERVER_ID: &str = "server-1382";
const CHANNEL_ID: &str = "limited-channel-1382";
const THREAD_ID: &str = "THREAD-1382";
const OWNER: &str = "Ari Owner";
const ALLOWED: &str = "Bea Allowed";
const OUTSIDE: &str = "Cy Outside";
const MARKED_MESSAGE: &str = "marked-message-1382";

#[test]
fn task1382_threads_inherit_channel_permissions_and_cannot_be_more_open() {
    let state = AppState::new();
    cmd_osl_write_server_member_list(
        &state,
        SERVER_ID.to_owned(),
        OWNER.to_owned(),
        "2026-08-06T09:00:00Z".to_owned(),
    )
    .expect("owner row can be written");
    cmd_osl_add_server_member(
        &state,
        SERVER_ID.to_owned(),
        ALLOWED.to_owned(),
        "2026-08-06T09:05:00Z".to_owned(),
    )
    .expect("allowed member can be added");
    cmd_osl_add_server_member(
        &state,
        SERVER_ID.to_owned(),
        OUTSIDE.to_owned(),
        "2026-08-06T09:10:00Z".to_owned(),
    )
    .expect("outside server member can be added");

    cmd_osl_set_limited_channel_members(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        vec![ALLOWED.to_owned()],
    )
    .expect("channel can be limited to one named member");
    cmd_osl_create_thread(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        THREAD_ID.to_owned(),
    )
    .expect("thread can be created inside limited channel");
    cmd_osl_add_thread_message(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        THREAD_ID.to_owned(),
        MARKED_MESSAGE.to_owned(),
        "marked body".to_owned(),
        true,
    )
    .expect("marked message can be added");
    cmd_osl_add_thread_message(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        THREAD_ID.to_owned(),
        "unmarked-message-1382".to_owned(),
        "unmarked body".to_owned(),
        false,
    )
    .expect("unmarked message can be added");

    let first_read = cmd_osl_read_thread(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        THREAD_ID.to_owned(),
        ALLOWED.to_owned(),
    )
    .expect("allowed channel member reads inherited thread");
    let first_marked_messages: Vec<_> = first_read
        .messages
        .iter()
        .map(|message| message.message_id.as_str())
        .collect();

    let outside_result = cmd_osl_read_thread(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        THREAD_ID.to_owned(),
        OUTSIDE.to_owned(),
    );
    let (outside_refusal, outside_message_count) = match outside_result {
        Ok(read) => ("unexpectedly allowed".to_owned(), read.messages.len()),
        Err(error) => (error, 0),
    };

    let before_readers: BTreeSet<_> = cmd_osl_list_thread_effective_readers(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        THREAD_ID.to_owned(),
    )
    .expect("effective readers can be listed before widening")
    .into_iter()
    .collect();
    let widen_result = cmd_osl_set_thread_permissions(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        THREAD_ID.to_owned(),
        vec![ALLOWED.to_owned(), OUTSIDE.to_owned()],
    );
    let widen_refusal = widen_result
        .as_ref()
        .err()
        .cloned()
        .unwrap_or_else(|| "unexpectedly changed thread permissions".to_owned());
    let after_readers: BTreeSet<_> = cmd_osl_list_thread_effective_readers(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        THREAD_ID.to_owned(),
    )
    .expect("effective readers can be listed after widening refusal")
    .into_iter()
    .collect();
    let changed_permissions = before_readers
        .symmetric_difference(&after_readers)
        .collect::<Vec<_>>()
        .len();

    let after_read = cmd_osl_read_thread(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        THREAD_ID.to_owned(),
        ALLOWED.to_owned(),
    )
    .expect("allowed channel member still reads inherited thread");
    let after_marked_messages: Vec<_> = after_read
        .messages
        .iter()
        .map(|message| message.message_id.as_str())
        .collect();

    println!("TASK1382 first_read.thread={}", first_read.thread_id);
    println!(
        "TASK1382 first_read.marked_message_count={}",
        first_read.messages.len()
    );
    println!(
        "TASK1382 first_read.marked_messages={}",
        first_marked_messages.join("|")
    );
    println!("TASK1382 outside_person={OUTSIDE}");
    println!("TASK1382 outside_read.refusal={outside_refusal}");
    println!("TASK1382 outside_read.message_count={outside_message_count}");
    println!("TASK1382 widen_refused={}", widen_result.is_err());
    println!("TASK1382 widen_refusal={widen_refusal}");
    println!("TASK1382 widen_changed_permissions={changed_permissions}");
    println!("TASK1382 after_read.thread={}", after_read.thread_id);
    println!(
        "TASK1382 after_read.marked_message_count={}",
        after_read.messages.len()
    );
    println!(
        "TASK1382 after_read.marked_messages={}",
        after_marked_messages.join("|")
    );

    assert_eq!(first_read.thread_id, THREAD_ID);
    assert_eq!(first_marked_messages, vec![MARKED_MESSAGE]);
    assert_eq!(
        outside_refusal,
        format!("OSL: channel read refused for {OUTSIDE}")
    );
    assert_eq!(outside_message_count, 0);
    assert!(widen_result.is_err());
    assert_eq!(
        widen_refusal,
        format!("OSL: thread {THREAD_ID} cannot be more open than channel {CHANNEL_ID}")
    );
    assert_eq!(changed_permissions, 0);
    assert_eq!(after_read.thread_id, THREAD_ID);
    assert_eq!(after_marked_messages, vec![MARKED_MESSAGE]);
}
