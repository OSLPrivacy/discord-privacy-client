use ipc::server_membership::{
    accept_server_mention_text, honest_mention_ring_decision, ServerMentionKind, ServerPermission,
    ServerPermissionStore,
};

const SERVER_ID: &str = "server-4880";
const UNPERMITTED_MEMBER: &str = "member-without-mention-everyone";
const PERMITTED_MEMBER: &str = "member-with-mention-everyone";
const HONEST_RECEIVING_APPS: usize = 6;

#[test]
fn task4880_mentions_are_trust_and_ring_only_from_receiver_verified_permission() {
    let sent = accept_server_mention_text("@everyone").expect("exact @everyone text is sendable");
    println!(
        "TASK4880_SENT_WITHOUT_PERMISSION accepted={} text={}",
        sent.accepted, sent.text
    );

    let mut permissions = ServerPermissionStore::default();
    let unpermitted = honest_mention_ring_decision(
        &permissions,
        SERVER_ID,
        UNPERMITTED_MEMBER,
        ServerMentionKind::Everyone,
        HONEST_RECEIVING_APPS,
        false,
    );
    println!(
        "TASK4880_UNPERMITTED_RING_COUNT={}/{} highlight_count={}",
        unpermitted.ringing_apps, unpermitted.honest_receiving_apps, unpermitted.highlighted_apps
    );

    permissions
        .set_person_permissions(
            SERVER_ID.to_owned(),
            PERMITTED_MEMBER.to_owned(),
            vec![ServerPermission::MentionEveryone],
        )
        .expect("grant mention-everyone permission");
    let permitted = honest_mention_ring_decision(
        &permissions,
        SERVER_ID,
        PERMITTED_MEMBER,
        ServerMentionKind::Everyone,
        HONEST_RECEIVING_APPS,
        false,
    );
    println!(
        "TASK4880_PERMITTED_RING_COUNT={}/{} highlight_count={}",
        permitted.ringing_apps, permitted.honest_receiving_apps, permitted.highlighted_apps
    );

    let forced_by_modified_sender = honest_mention_ring_decision(
        &permissions,
        SERVER_ID,
        UNPERMITTED_MEMBER,
        ServerMentionKind::Everyone,
        HONEST_RECEIVING_APPS,
        true,
    );
    println!(
        "TASK4880_MODIFIED_SENDER_FORCED_RING_COUNT={} sender_claimed_permission=true",
        forced_by_modified_sender.ringing_apps
    );

    let trust_rows: Vec<_> = ServerMentionKind::ALL_TRUST_ROWS
        .iter()
        .map(|kind| format!("{}={}", kind.row_name(), kind.trust_row().label()))
        .collect();
    println!("TASK4880_MENTION_ROWS {}", trust_rows.join(" "));

    assert!(sent.accepted);
    assert_eq!(sent.text, "@everyone");
    assert_eq!(unpermitted.ringing_apps, 0);
    assert_eq!(unpermitted.highlighted_apps, 0);
    assert_eq!(unpermitted.honest_receiving_apps, 6);
    assert_eq!(permitted.ringing_apps, 6);
    assert_eq!(permitted.highlighted_apps, 6);
    assert_eq!(permitted.honest_receiving_apps, 6);
    assert_eq!(forced_by_modified_sender.ringing_apps, 0);
    assert_eq!(
        trust_rows,
        vec!["everyone=TRUST", "here=TRUST", "role=TRUST"]
    );
}
