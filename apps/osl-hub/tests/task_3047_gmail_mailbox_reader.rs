#![cfg(feature = "core")]

use osl_privacy_hub::service_connections::{
    open_gmail_shared_mailbox_message, read_gmail_shared_mailbox_labels,
    read_gmail_shared_mailbox_messages, SharedMailLabel, SharedMailMessageRecord,
    SharedMailboxSnapshot,
};

const SIGNED_IN: &str = "scrub-owner@gmail.test";

fn seeded_gmail_test_mailbox() -> SharedMailboxSnapshot {
    SharedMailboxSnapshot::new(
        SIGNED_IN,
        [
            SharedMailLabel::new("Inbox", "Inbox"),
            SharedMailLabel::new("Sent", "Sent"),
            SharedMailLabel::new("[Gmail]/All Mail", "All Mail"),
            SharedMailLabel::new("[Gmail]/Trash", "Trash"),
        ],
        [
            SharedMailMessageRecord::new(
                "Sent",
                "sent-scrub-gm-mine",
                "SCRUB-GM-MINE",
                1_786_190_400,
                SIGNED_IN,
                "The seeded Gmail message marked SCRUB-GM-MINE opens as yours.",
            ),
            SharedMailMessageRecord::new(
                "Sent",
                "sent-scrub-gm-plan",
                "Scrub deletion plan",
                1_786_194_000,
                SIGNED_IN,
                "Second sent scrub fixture.",
            ),
            SharedMailMessageRecord::new(
                "Sent",
                "sent-scrub-gm-receipt",
                "Scrub receipt check",
                1_786_197_600,
                SIGNED_IN,
                "Third sent scrub fixture.",
            ),
            SharedMailMessageRecord::new(
                "Inbox",
                "inbox-scrub-gm-one",
                "Friend cleanup request",
                1_786_201_200,
                "friend-one@gmail.test",
                "First inbox fixture.",
            ),
            SharedMailMessageRecord::new(
                "Inbox",
                "inbox-scrub-gm-two",
                "Team scrub note",
                1_786_204_800,
                "team@gmail.test",
                "Second inbox fixture.",
            ),
        ],
    )
}

#[test]
fn task_3047_gmail_reader_lists_labels_sent_messages_and_opens_marked_message() {
    let mailbox = seeded_gmail_test_mailbox();

    let labels = read_gmail_shared_mailbox_labels(&mailbox).expect("Gmail labels list");
    let sent =
        read_gmail_shared_mailbox_messages(&mailbox, "Sent").expect("Gmail Sent messages list");
    let opened = open_gmail_shared_mailbox_message(&mailbox, "Sent", "sent-scrub-gm-mine")
        .expect("open the marked Gmail message");
    let inbox =
        read_gmail_shared_mailbox_messages(&mailbox, "Inbox").expect("Gmail Inbox messages list");
    let inbox_not_yours_count = inbox
        .iter()
        .filter(|message| message.called == "not_yours")
        .count();

    println!("TASK3047 direct_reader=gmail_shared_mailbox_reader");
    println!("TASK3047 label_count={}", labels.len());
    for label in &labels {
        println!("TASK3047 label id={} name={}", label.label_id, label.name);
    }
    println!("TASK3047 sent_message_count={}", sent.len());
    for message in &sent {
        println!(
            "TASK3047 sent_message id={} subject={} time={} sender={} called={}",
            message.message_id, message.subject, message.time, message.sender, message.called
        );
    }
    println!("TASK3047 opened_message_id={}", opened.message_id);
    println!("TASK3047 opened_subject={}", opened.subject);
    println!("TASK3047 opened_called={}", opened.called);
    println!("TASK3047 inbox_message_count={}", inbox.len());
    for message in &inbox {
        println!(
            "TASK3047 inbox_message id={} subject={} time={} sender={} called={}",
            message.message_id, message.subject, message.time, message.sender, message.called
        );
    }
    println!("TASK3047 inbox_not_yours_count={inbox_not_yours_count}");

    assert_eq!(labels.len(), 4);
    assert_eq!(
        labels
            .iter()
            .map(|label| label.label_id.as_str())
            .collect::<Vec<_>>(),
        vec!["Inbox", "Sent", "[Gmail]/All Mail", "[Gmail]/Trash"]
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
            ("SCRUB-GM-MINE", 1_786_190_400, SIGNED_IN),
            ("Scrub deletion plan", 1_786_194_000, SIGNED_IN),
            ("Scrub receipt check", 1_786_197_600, SIGNED_IN),
        ]
    );
    assert_eq!(opened.subject, "SCRUB-GM-MINE");
    assert_eq!(opened.called, "yours");
    assert_eq!(inbox.len(), 2);
    assert_eq!(inbox_not_yours_count, 2);
    assert!(inbox.iter().all(|message| message.called == "not_yours"));
}
