use ipc::commands::cmd_osl_create_channel_message_thread;
use ipc::state::AppState;

#[test]
fn direct_command_creates_thread_with_parent_message_and_channel_ids() {
    let state = AppState::new();

    let created = cmd_osl_create_channel_message_thread(
        &state,
        "task-1325-channel".to_owned(),
        "task-1325-parent-message".to_owned(),
        "task-1325-thread".to_owned(),
    )
    .expect("direct channel-thread command");

    assert_eq!(created.channel_id, "task-1325-channel");
    assert_eq!(created.parent_message_id, "task-1325-parent-message");
    assert_eq!(created.thread_id, "task-1325-thread");
    assert_eq!(created.channel_message_count, 1);
    assert_eq!(created.thread_count, 1);
    assert_eq!(created.parent_thread_count, 1);

    let messages = state
        .channel_messages
        .lock()
        .expect("channel_messages store");
    assert_eq!(messages.len(), 1);
    let parent = messages
        .get("task-1325-parent-message")
        .expect("parent message record");
    assert_eq!(parent.message_id, "task-1325-parent-message");
    assert_eq!(parent.channel_id, "task-1325-channel");
    assert_eq!(parent.thread_ids, vec!["task-1325-thread".to_owned()]);
    drop(messages);

    let threads = state.channel_threads.lock().expect("channel_threads store");
    assert_eq!(threads.len(), 1);
    let thread = threads.get("task-1325-thread").expect("thread record");
    assert_eq!(thread.thread_id, "task-1325-thread");
    assert_eq!(thread.channel_id, "task-1325-channel");
    assert_eq!(thread.parent_message_id, "task-1325-parent-message");

    println!(
        "TASK1325_COMMAND=cmd_osl_create_channel_message_thread TASK1325_THREAD_COUNT={} TASK1325_CHANNEL_ID={} TASK1325_PARENT_MESSAGE_ID={} TASK1325_THREAD_ID={} TASK1325_CHANNEL_MESSAGE_COUNT={} TASK1325_PARENT_THREAD_COUNT={}",
        created.thread_count,
        created.channel_id,
        created.parent_message_id,
        created.thread_id,
        created.channel_message_count,
        created.parent_thread_count
    );
}
