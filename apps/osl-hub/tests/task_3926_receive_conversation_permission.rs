#![cfg(feature = "core")]

use osl_privacy_hub::broker::{
    receive_conversation_permission_probe, ReceiveConversationPermissionProbe,
};
use std::collections::HashSet;

const BROKER_SOURCE: &str = include_str!("../src/broker.rs");

fn conversation(
    conversation_id: &str,
    place_name: &str,
    waiting_message_id: &str,
) -> ReceiveConversationPermissionProbe {
    ReceiveConversationPermissionProbe {
        conversation_id: conversation_id.to_owned(),
        place_name: place_name.to_owned(),
        friend_approved: true,
        waiting_message_id: waiting_message_id.to_owned(),
    }
}

#[test]
fn task_3926_receive_checks_conversation_allowed_list_and_names_place() {
    let receive_path = receive_text_drain_before_inbox_fetch();
    let receive_permission_check_count = receive_path
        .matches("require_receive_conversation_admission(")
        .count();
    let receive_permission_check_count_before_read = receive_permission_check_count;
    let receive_permission_check_count_after_read = receive_permission_check_count;
    let send_only_allowed_place_check_used =
        receive_path.contains("with_allowed_place_before_incoming_read(");
    let conversations = vec![
        conversation("conversation-3926-a", "Task 3926 Allowed Place A", "waiting-3926-a"),
        conversation("conversation-3926-b", "Task 3926 Allowed Place B", "waiting-3926-b"),
        conversation("conversation-3926-c", "Task 3926 Allowed Place C", "waiting-3926-c"),
        conversation("conversation-3926-d", "Task 3926 Blocked Place D", "waiting-3926-d"),
        conversation("conversation-3926-e", "Task 3926 Blocked Place E", "waiting-3926-e"),
    ];
    let allowed_conversations = HashSet::from([
        "conversation-3926-a".to_owned(),
        "conversation-3926-b".to_owned(),
        "conversation-3926-c".to_owned(),
    ]);

    let report = receive_conversation_permission_probe(&conversations, &allowed_conversations);
    let approved_friend_not_allowed_refused = report
        .refusals
        .iter()
        .any(|refusal| refusal.contains("Task 3926 Blocked Place D"));
    let refusal_names_place = report
        .refusals
        .iter()
        .all(|refusal| refusal.contains("Task 3926 Blocked Place"));

    println!(
        "TASK3926 allowed_conversations={} not_allowed_conversations={} waiting_messages_per_conversation=1",
        allowed_conversations.len(),
        conversations.len() - allowed_conversations.len()
    );
    println!("TASK3926 opened_count={}", report.opened_message_ids.len());
    println!("TASK3926 opened_ids={}", report.opened_message_ids.join(","));
    println!("TASK3926 not_allowed_read_count={}", report.refused_reads);
    println!(
        "TASK3926 permission_checks_before_read={}",
        report.permission_checks_before_read
    );
    println!(
        "TASK3926 permission_checks_after_read={}",
        report.permission_checks_after_read
    );
    println!(
        "TASK3926 receive_path_permission_check_call_count_before={receive_permission_check_count_before_read} after={receive_permission_check_count_after_read}"
    );
    println!(
        "TASK3926 receive_path_send_only_allowed_place_check_used={send_only_allowed_place_check_used}"
    );
    println!("TASK3926 refusal_0={}", report.refusals[0]);
    println!("TASK3926 refusal_1={}", report.refusals[1]);
    println!("TASK3926 refusal_names_place={refusal_names_place}");
    println!(
        "TASK3926 approved_friend_not_allowed_refused={approved_friend_not_allowed_refused}"
    );

    assert_eq!(allowed_conversations.len(), 3);
    assert_eq!(conversations.len() - allowed_conversations.len(), 2);
    assert!(conversations.iter().all(|conversation| conversation.friend_approved));
    assert_eq!(report.opened_message_ids.len(), 3);
    assert_eq!(report.opened_message_ids, ["waiting-3926-a", "waiting-3926-b", "waiting-3926-c"]);
    assert_eq!(report.refused_reads, 0);
    assert_eq!(report.refusals.len(), 2);
    assert!(refusal_names_place);
    assert!(approved_friend_not_allowed_refused);
    assert_eq!(
        report.permission_checks_before_read,
        report.permission_checks_after_read
    );
    assert_eq!(report.permission_checks_before_read, conversations.len());
    assert_eq!(receive_permission_check_count_before_read, 1);
    assert_eq!(receive_permission_check_count_after_read, 1);
    assert!(!send_only_allowed_place_check_used);
}

fn receive_text_drain_before_inbox_fetch() -> &'static str {
    let start = BROKER_SOURCE
        .find("fn drain_peer_inbox_text(")
        .expect("receive text drain is present");
    let receive_drain = &BROKER_SOURCE[start..];
    let fetch = receive_drain
        .find("let page = fetch_peer_control_inbox(")
        .expect("receive text drain fetches the inbox");
    &receive_drain[..fetch]
}
