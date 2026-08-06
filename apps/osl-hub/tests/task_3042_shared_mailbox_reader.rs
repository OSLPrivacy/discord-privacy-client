#![cfg(feature = "core")]

use osl_privacy_hub::services::{
    open_shared_mailbox_message, read_shared_mailbox_folders, read_shared_mailbox_messages,
    MailboxFolderCandidate, MailboxMessageCandidate, MailboxReaderSnapshot,
};

const OWNER: &str = "osl_task_3042_owner";
const SERVICE: &str = "gmail";
const ACCOUNT: &str = "acct-task-3042-mailbox";

fn test_mailbox() -> MailboxReaderSnapshot {
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
                "sent-task-3042-001",
                "Task 3042 alpha",
                1_786_017_600,
                "owner@example.test",
                "Alpha sent body\nopened by the shared mailbox reader.",
            ),
            MailboxMessageCandidate::new(
                "Sent",
                "sent-task-3042-002",
                "Task 3042 beta",
                1_786_021_200,
                "owner@example.test",
                "Beta sent body",
            ),
            MailboxMessageCandidate::new(
                "Sent",
                "sent-task-3042-003",
                "Task 3042 gamma",
                1_786_024_800,
                "delegate@example.test",
                "Gamma sent body",
            ),
            MailboxMessageCandidate::new(
                "Inbox",
                "inbox-task-3042-001",
                "Task 3042 received",
                1_786_028_400,
                "friend@example.test",
                "Inbox body",
            ),
        ],
    )
}

#[test]
fn task_3042_shared_mailbox_reader_lists_folders_messages_and_errors_on_unknown_folder() {
    let mailbox = test_mailbox();

    let folders = read_shared_mailbox_folders(OWNER, SERVICE, ACCOUNT, &mailbox)
        .expect("list shared mailbox folders");
    let sent = read_shared_mailbox_messages(OWNER, SERVICE, ACCOUNT, "Sent", &mailbox)
        .expect("list Sent messages");
    let opened = open_shared_mailbox_message(
        OWNER,
        SERVICE,
        ACCOUNT,
        "Sent",
        "sent-task-3042-001",
        &mailbox,
    )
    .expect("open one shared mailbox message");
    let unknown_folder_error =
        read_shared_mailbox_messages(OWNER, SERVICE, ACCOUNT, "Spam", &mailbox)
            .expect_err("unknown folder must error rather than return an empty list");

    println!("TASK3042_DIRECT_READER=shared_mailbox_reader");
    println!("TASK3042_FOLDER_COUNT={}", folders.len());
    for folder in &folders {
        println!(
            "TASK3042_FOLDER id={} label={} service={} account={}",
            folder.folder_id, folder.label, folder.service_id, folder.account_id
        );
    }
    println!("TASK3042_SENT_FOLDER_MESSAGE_COUNT={}", sent.len());
    for message in &sent {
        println!(
            "TASK3042_SENT_MESSAGE id={} subject={} time={} sender={}",
            message.message_id, message.subject, message.time, message.sender
        );
    }
    println!("TASK3042_OPENED_MESSAGE_ID={}", opened.message_id);
    println!("TASK3042_OPENED_MESSAGE_SUBJECT={}", opened.subject);
    println!("TASK3042_UNKNOWN_FOLDER_ERROR={unknown_folder_error}");
    println!(
        "TASK3042_UNKNOWN_FOLDER_EMPTY_LIST={}",
        read_shared_mailbox_messages(OWNER, SERVICE, ACCOUNT, "Spam", &mailbox).is_ok()
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
            ("Task 3042 alpha", 1_786_017_600, "owner@example.test"),
            ("Task 3042 beta", 1_786_021_200, "owner@example.test"),
            ("Task 3042 gamma", 1_786_024_800, "delegate@example.test"),
        ]
    );
    assert_eq!(opened.message_id, "sent-task-3042-001");
    assert_eq!(
        opened.body,
        "Alpha sent body\nopened by the shared mailbox reader."
    );
    assert_eq!(unknown_folder_error, "mailbox folder not found");
}
