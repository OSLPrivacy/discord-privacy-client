#![cfg(feature = "core")]

use osl_privacy_hub::scrub_hosted::yahoo_mail::{
    open_yahoo_mailbox_message_for_scrub, read_yahoo_mailbox_for_scrub,
};
use osl_privacy_hub::services::{
    MailboxFolderCandidate, MailboxMessageCandidate, MailboxReaderSnapshot,
};

const OWNER: &str = "osl_task_3059_owner";
const ACCOUNT: &str = "acct-task-3059-yahoo";
const SIGNED_IN_ADDRESS: &str = "owner@yahoo.example.test";

fn seeded_yahoo_mailbox() -> MailboxReaderSnapshot {
    MailboxReaderSnapshot::new(
        [
            MailboxFolderCandidate::new("Inbox", "Inbox"),
            MailboxFolderCandidate::new("Sent", "Sent"),
            MailboxFolderCandidate::new("Archive", "Archive"),
            MailboxFolderCandidate::new("Trash", "Trash"),
        ],
        [
            MailboxMessageCandidate::new(
                "Sent",
                "sent-yh-3059-001",
                "SCRUB-YH-MINE",
                1_786_104_000,
                SIGNED_IN_ADDRESS,
                "Yahoo sent body owned by the signed-in account.",
            ),
            MailboxMessageCandidate::new(
                "Sent",
                "sent-yh-3059-002",
                "Yahoo sent budget",
                1_786_107_600,
                "assistant@yahoo.example.test",
                "Yahoo sent budget body.",
            ),
            MailboxMessageCandidate::new(
                "Sent",
                "sent-yh-3059-003",
                "Yahoo sent travel",
                1_786_111_200,
                "delegate@yahoo.example.test",
                "Yahoo sent travel body.",
            ),
            MailboxMessageCandidate::new(
                "Inbox",
                "inbox-yh-3059-001",
                "Yahoo inbox hello",
                1_786_114_800,
                "friend@example.test",
                "Inbox hello body.",
            ),
            MailboxMessageCandidate::new(
                "Inbox",
                "inbox-yh-3059-002",
                "Yahoo inbox reminder",
                1_786_118_400,
                "alerts@example.test",
                "Inbox reminder body.",
            ),
        ],
    )
}

#[test]
fn task_3059_yahoo_reader_returns_seeded_folders_sent_summaries_and_ownership() {
    let mailbox = seeded_yahoo_mailbox();
    let read = read_yahoo_mailbox_for_scrub(OWNER, ACCOUNT, SIGNED_IN_ADDRESS, &mailbox)
        .expect("read seeded Yahoo mailbox");
    let opened =
        open_yahoo_mailbox_message_for_scrub(OWNER, ACCOUNT, "Sent", "sent-yh-3059-001", &mailbox)
            .expect("open marked Yahoo sent message through shared reader");

    let marked = read
        .sent
        .iter()
        .find(|message| message.summary.subject == "SCRUB-YH-MINE")
        .expect("seeded marked Yahoo message is present");
    let inbox_not_yours_count = read.inbox.iter().filter(|message| !message.yours).count();

    println!("TASK3059_PROVIDER=Yahoo Mail");
    println!("TASK3059_READER=shared_mailbox_reader");
    println!("TASK3059_FOLDER_COUNT={}", read.folders.len());
    for folder in &read.folders {
        println!(
            "TASK3059_FOLDER id={} label={} service={} account={}",
            folder.folder_id, folder.label, folder.service_id, folder.account_id
        );
    }
    println!("TASK3059_SENT_FOLDER_MESSAGE_COUNT={}", read.sent.len());
    for message in &read.sent {
        println!(
            "TASK3059_SENT_MESSAGE id={} subject={} time={} sender={} yours={}",
            message.summary.message_id,
            message.summary.subject,
            message.summary.time,
            message.summary.sender,
            message.yours
        );
    }
    println!(
        "TASK3059_MARKED_MESSAGE={} called={}",
        marked.summary.subject,
        if marked.yours { "yours" } else { "not-yours" }
    );
    for message in &read.inbox {
        println!(
            "TASK3059_INBOX_MESSAGE id={} subject={} sender={} called={}",
            message.summary.message_id,
            message.summary.subject,
            message.summary.sender,
            if message.yours { "yours" } else { "not-yours" }
        );
    }
    println!("TASK3059_INBOX_NOT_YOURS_COUNT={inbox_not_yours_count}");
    println!("TASK3059_OPENED_MESSAGE_ID={}", opened.message_id);
    println!("TASK3059_OPENED_MESSAGE_SUBJECT={}", opened.subject);

    assert_eq!(read.folders.len(), 4);
    assert_eq!(
        read.folders
            .iter()
            .map(|folder| folder.folder_id.as_str())
            .collect::<Vec<_>>(),
        vec!["Inbox", "Sent", "Archive", "Trash"]
    );
    assert_eq!(read.sent.len(), 3);
    assert_eq!(
        read.sent
            .iter()
            .map(|message| {
                (
                    message.summary.subject.as_str(),
                    message.summary.time,
                    message.summary.sender.as_str(),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            ("SCRUB-YH-MINE", 1_786_104_000, SIGNED_IN_ADDRESS),
            (
                "Yahoo sent budget",
                1_786_107_600,
                "assistant@yahoo.example.test",
            ),
            (
                "Yahoo sent travel",
                1_786_111_200,
                "delegate@yahoo.example.test",
            ),
        ]
    );
    assert!(marked.yours);
    assert_eq!(inbox_not_yours_count, 2);
    assert!(read.inbox.iter().all(|message| !message.yours));
    assert_eq!(opened.subject, "SCRUB-YH-MINE");
}
