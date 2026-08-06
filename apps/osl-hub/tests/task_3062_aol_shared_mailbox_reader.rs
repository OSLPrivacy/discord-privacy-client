#![cfg(feature = "core")]

use osl_privacy_hub::services::{
    read_shared_mailbox_folders, read_shared_mailbox_messages, MailboxFolderCandidate,
    MailboxMessageCandidate, MailboxReaderSnapshot, SharedMailboxOwnership,
};

const OWNER: &str = "osl_task_3062_owner";
const SERVICE: &str = "aol";
const ACCOUNT: &str = "acct-task-3062-aol";
const SIGNED_IN_ADDRESS: &str = "scrub.owner@aol.example.test";

fn seeded_aol_mailbox() -> MailboxReaderSnapshot {
    MailboxReaderSnapshot::new_for_signed_in_address(
        SIGNED_IN_ADDRESS,
        [
            MailboxFolderCandidate::new("Inbox", "Inbox"),
            MailboxFolderCandidate::new("Sent", "Sent"),
            MailboxFolderCandidate::new("Archive", "Archive"),
            MailboxFolderCandidate::new("Trash", "Trash"),
        ],
        [
            MailboxMessageCandidate::new(
                "Sent",
                "aol-sent-task-3062-001",
                "SCRUB-AO-MINE",
                1_786_104_000,
                SIGNED_IN_ADDRESS,
                "Owner-authored AOL message for Scrub.",
            ),
            MailboxMessageCandidate::new(
                "Sent",
                "aol-sent-task-3062-002",
                "SCRUB-AO-OTHER-1",
                1_786_107_600,
                "friend.one@example.test",
                "Non-owner Sent fixture row.",
            ),
            MailboxMessageCandidate::new(
                "Sent",
                "aol-sent-task-3062-003",
                "SCRUB-AO-OTHER-2",
                1_786_111_200,
                "delegate@example.test",
                "Delegate Sent fixture row.",
            ),
            MailboxMessageCandidate::new(
                "Inbox",
                "aol-inbox-task-3062-001",
                "SCRUB-AO-INBOX-1",
                1_786_114_800,
                "friend.one@example.test",
                "Inbox fixture row one.",
            ),
            MailboxMessageCandidate::new(
                "Inbox",
                "aol-inbox-task-3062-002",
                "SCRUB-AO-INBOX-2",
                1_786_118_400,
                "friend.two@example.test",
                "Inbox fixture row two.",
            ),
        ],
    )
}

#[test]
fn task_3062_aol_reader_returns_seeded_folders_messages_ownership_and_stable_second_read() {
    let mailbox = seeded_aol_mailbox();

    let folders = read_shared_mailbox_folders(OWNER, SERVICE, ACCOUNT, &mailbox)
        .expect("read seeded AOL folders");
    let sent = read_shared_mailbox_messages(OWNER, SERVICE, ACCOUNT, "Sent", &mailbox)
        .expect("read seeded AOL Sent messages");
    let inbox = read_shared_mailbox_messages(OWNER, SERVICE, ACCOUNT, "Inbox", &mailbox)
        .expect("read seeded AOL Inbox messages");

    let folders_again = read_shared_mailbox_folders(OWNER, SERVICE, ACCOUNT, &mailbox)
        .expect("read seeded AOL folders again");
    let sent_again = read_shared_mailbox_messages(OWNER, SERVICE, ACCOUNT, "Sent", &mailbox)
        .expect("read seeded AOL Sent messages again");

    let mine = sent
        .iter()
        .find(|message| message.subject == "SCRUB-AO-MINE")
        .expect("seeded owner-authored AOL message is visible in Sent");
    let inbox_not_yours = inbox
        .iter()
        .filter(|message| message.ownership == SharedMailboxOwnership::NotYours)
        .count();

    println!("TASK3062_AOL_READER=shared_mailbox_reader");
    println!("TASK3062_AOL_FOLDER_COUNT={}", folders.len());
    for folder in &folders {
        println!(
            "TASK3062_AOL_FOLDER id={} label={}",
            folder.folder_id, folder.label
        );
    }
    println!("TASK3062_AOL_SENT_MESSAGE_COUNT={}", sent.len());
    for message in &sent {
        println!(
            "TASK3062_AOL_SENT_MESSAGE subject={} time={} sender={} ownership={}",
            message.subject,
            message.time,
            message.sender,
            message.ownership.as_str()
        );
    }
    println!(
        "TASK3062_AOL_MINE subject={} ownership={}",
        mine.subject,
        mine.ownership.as_str()
    );
    println!("TASK3062_AOL_INBOX_MESSAGE_COUNT={}", inbox.len());
    for message in &inbox {
        println!(
            "TASK3062_AOL_INBOX_MESSAGE subject={} ownership={}",
            message.subject,
            message.ownership.as_str()
        );
    }
    println!("TASK3062_AOL_INBOX_NOT_YOURS_COUNT={inbox_not_yours}");
    println!("TASK3062_AOL_SECOND_FOLDER_COUNT={}", folders_again.len());
    println!("TASK3062_AOL_SECOND_SENT_COUNT={}", sent_again.len());
    println!(
        "TASK3062_AOL_SECOND_READ_SAME={}",
        folders_again == folders && sent_again == sent
    );

    assert_eq!(folders.len(), 4);
    assert_eq!(
        folders
            .iter()
            .map(|folder| folder.folder_id.as_str())
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
            ("SCRUB-AO-MINE", 1_786_104_000, SIGNED_IN_ADDRESS),
            ("SCRUB-AO-OTHER-1", 1_786_107_600, "friend.one@example.test"),
            ("SCRUB-AO-OTHER-2", 1_786_111_200, "delegate@example.test"),
        ]
    );
    assert_eq!(mine.ownership, SharedMailboxOwnership::Yours);
    assert_eq!(inbox.len(), 2);
    assert_eq!(inbox_not_yours, 2);
    assert!(inbox
        .iter()
        .all(|message| message.ownership == SharedMailboxOwnership::NotYours));
    assert_eq!(folders_again, folders);
    assert_eq!(sent_again, sent);
}
