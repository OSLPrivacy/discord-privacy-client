#![cfg(feature = "core")]

use osl_privacy_hub::service_connections::{
    open_icloud_shared_mailbox_message, read_icloud_shared_mailbox_folders,
    read_icloud_shared_mailbox_messages, SharedMailLabel, SharedMailMessageRecord,
    SharedMailboxSnapshot,
};

const SIGNED_IN: &str = "scrub-owner@icloud.test";

fn seeded_icloud_test_mailbox() -> SharedMailboxSnapshot {
    SharedMailboxSnapshot::new(
        SIGNED_IN,
        [
            SharedMailLabel::new("Inbox", "Inbox"),
            SharedMailLabel::new("Sent", "Sent"),
            SharedMailLabel::new("Archive", "Archive"),
            SharedMailLabel::new("Trash", "Trash"),
        ],
        [
            SharedMailMessageRecord::new(
                "Sent",
                "sent-scrub-ic-mine",
                "SCRUB-IC-MINE",
                1_786_276_800,
                SIGNED_IN,
                "The seeded iCloud message marked SCRUB-IC-MINE opens as yours.",
            ),
            SharedMailMessageRecord::new(
                "Sent",
                "sent-scrub-ic-plan",
                "iCloud scrub deletion plan",
                1_786_280_400,
                SIGNED_IN,
                "Second sent iCloud scrub fixture.",
            ),
            SharedMailMessageRecord::new(
                "Sent",
                "sent-scrub-ic-receipt",
                "iCloud scrub receipt check",
                1_786_284_000,
                SIGNED_IN,
                "Third sent iCloud scrub fixture.",
            ),
            SharedMailMessageRecord::new(
                "Inbox",
                "inbox-scrub-ic-one",
                "Friend iCloud cleanup request",
                1_786_287_600,
                "friend-one@icloud.test",
                "First iCloud inbox fixture.",
            ),
            SharedMailMessageRecord::new(
                "Inbox",
                "inbox-scrub-ic-two",
                "Team iCloud scrub note",
                1_786_291_200,
                "team@icloud.test",
                "Second iCloud inbox fixture.",
            ),
        ],
    )
}

#[test]
fn task_3071_icloud_reader_lists_folders_sent_messages_and_stable_second_read() {
    let mailbox = seeded_icloud_test_mailbox();

    let folders = read_icloud_shared_mailbox_folders(&mailbox).expect("iCloud folders list");
    let sent =
        read_icloud_shared_mailbox_messages(&mailbox, "Sent").expect("iCloud Sent messages list");
    let inbox =
        read_icloud_shared_mailbox_messages(&mailbox, "Inbox").expect("iCloud Inbox messages list");
    let opened = open_icloud_shared_mailbox_message(&mailbox, "Sent", "sent-scrub-ic-mine")
        .expect("open the marked iCloud message");
    let second_folders =
        read_icloud_shared_mailbox_folders(&mailbox).expect("second iCloud folders list");
    let second_sent = read_icloud_shared_mailbox_messages(&mailbox, "Sent")
        .expect("second iCloud Sent messages list");
    let inbox_not_yours_count = inbox
        .iter()
        .filter(|message| message.called == "not_yours")
        .count();

    println!("TASK3071 direct_reader=icloud_shared_mailbox_reader");
    println!("TASK3071 folder_count={}", folders.len());
    for folder in &folders {
        println!(
            "TASK3071 folder id={} name={}",
            folder.label_id, folder.name
        );
    }
    println!("TASK3071 sent_message_count={}", sent.len());
    for message in &sent {
        println!(
            "TASK3071 sent_message id={} subject={} time={} sender={} called={}",
            message.message_id, message.subject, message.time, message.sender, message.called
        );
    }
    println!("TASK3071 opened_message_id={}", opened.message_id);
    println!("TASK3071 opened_subject={}", opened.subject);
    println!("TASK3071 opened_called={}", opened.called);
    println!("TASK3071 inbox_message_count={}", inbox.len());
    for message in &inbox {
        println!(
            "TASK3071 inbox_message id={} subject={} time={} sender={} called={}",
            message.message_id, message.subject, message.time, message.sender, message.called
        );
    }
    println!("TASK3071 inbox_not_yours_count={inbox_not_yours_count}");
    println!("TASK3071 second_read_folder_count={}", second_folders.len());
    println!(
        "TASK3071 second_read_sent_message_count={}",
        second_sent.len()
    );
    println!(
        "TASK3071 second_read_matches_first={}",
        second_folders == folders && second_sent == sent
    );

    assert_eq!(folders.len(), 4);
    assert_eq!(
        folders
            .iter()
            .map(|folder| folder.label_id.as_str())
            .collect::<Vec<_>>(),
        vec!["Inbox", "Sent", "Archive", "Trash"]
    );
    assert_eq!(sent.len(), 3);
    assert_eq!(
        sent.iter()
            .map(|message| (
                message.subject.as_str(),
                message.time,
                message.sender.as_str()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("SCRUB-IC-MINE", 1_786_276_800, SIGNED_IN),
            ("iCloud scrub deletion plan", 1_786_280_400, SIGNED_IN),
            ("iCloud scrub receipt check", 1_786_284_000, SIGNED_IN),
        ]
    );
    assert_eq!(opened.subject, "SCRUB-IC-MINE");
    assert_eq!(opened.called, "yours");
    assert_eq!(
        sent.iter()
            .find(|message| message.subject == "SCRUB-IC-MINE")
            .expect("marked message is in Sent")
            .called,
        "yours"
    );
    assert_eq!(inbox.len(), 2);
    assert_eq!(inbox_not_yours_count, 2);
    assert!(inbox.iter().all(|message| message.called == "not_yours"));
    assert_eq!(second_folders, folders);
    assert_eq!(second_sent, sent);
}
