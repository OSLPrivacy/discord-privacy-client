#![cfg(feature = "core")]

use osl_privacy_hub::services::{
    receive_shared_mailbox_messages, shared_mailbox_thread_name, MailboxFolderCandidate,
    MailboxMessageCandidate, MailboxReaderSnapshot, SharedMailboxReceiveMeter,
};

const OWNER: &str = "osl_task_4120_owner";
const SERVICE: &str = "gmail";
const ACCOUNT: &str = "acct-task-4120-mailbox";
const ALICE: &str = "alice@example.test";
const BOB: &str = "bob@example.test";
const SUBJECT: &str = "Project lighthouse";

fn arrived_mailbox() -> MailboxReaderSnapshot {
    let mut messages = (0_i64..10)
        .map(|ordinal| {
            let subject = if ordinal == 9 {
                "RE: Project lighthouse"
            } else {
                SUBJECT
            };
            MailboxMessageCandidate::new(
                "Inbox",
                format!("task-4120-arrived-{ordinal:02}"),
                subject,
                1_786_300_000 + ordinal,
                BOB,
                format!("arrived text {ordinal}"),
            )
        })
        .collect::<Vec<_>>();
    // These provider rows are deliberately outside the requested incoming
    // conversation and must remain untouched by the receiving read.
    messages.push(MailboxMessageCandidate::new(
        "Inbox",
        "task-4120-unrelated",
        "Unrelated mail",
        1_786_300_100,
        "stranger@example.test",
        "This is not Bob's conversation.",
    ));
    messages.push(MailboxMessageCandidate::new(
        "Sent",
        "task-4120-sent",
        SUBJECT,
        1_786_300_101,
        ALICE,
        "Alice's sent copy remains in Sent.",
    ));

    MailboxReaderSnapshot::new_for_signed_in_address(
        ALICE,
        [
            MailboxFolderCandidate::new("Inbox", "Inbox"),
            MailboxFolderCandidate::new("Sent", "Sent"),
        ],
        messages,
    )
}

#[test]
fn task_4120_receiving_read_returns_arrived_messages_without_changing_mailbox() {
    let mailbox = arrived_mailbox();
    let mailbox_before = mailbox.clone();
    let meter = SharedMailboxReceiveMeter::default();
    let count_before = meter.completed_reads();

    let read =
        receive_shared_mailbox_messages(OWNER, SERVICE, ACCOUNT, ALICE, BOB, &mailbox, &meter)
            .expect("the arrived Inbox messages from Bob are readable");

    let first_thread = shared_mailbox_thread_name(ALICE, BOB, SUBJECT)
        .expect("Alice's copy has a stable thread name");
    let second_thread = shared_mailbox_thread_name(BOB, ALICE, "re: Project lighthouse")
        .expect("Bob's copy has the same stable thread name");
    let different_thread = shared_mailbox_thread_name(ALICE, BOB, "Quarterly plan")
        .expect("a distinct subject has a distinct thread name");

    assert_eq!(read.inbox.len(), 10);
    for (ordinal, message) in read.inbox.iter().enumerate() {
        assert_eq!(message.sender, BOB);
        assert_eq!(
            message.time,
            1_786_300_000 + i64::try_from(ordinal).unwrap()
        );
        assert_eq!(message.text, format!("arrived text {ordinal}"));
        assert_eq!(message.thread_name, first_thread);
    }
    assert_eq!(first_thread, second_thread);
    assert_ne!(first_thread, different_thread);
    assert_eq!(read.inbox[9].thread_name, first_thread);
    assert_eq!(mailbox, mailbox_before);
    assert_eq!(count_before, 0);
    assert_eq!(meter.completed_reads(), 1);

    println!("TASK4120_ARRIVED_MESSAGE_COUNT={}", read.inbox.len());
    println!("TASK4120_MESSAGE_FIELDS=sender,time,text");
    println!("TASK4120_THREAD_COPY_ALICE={first_thread}");
    println!("TASK4120_THREAD_COPY_BOB={second_thread}");
    println!("TASK4120_DIFFERENT_THREAD={different_thread}");
    println!("TASK4120_REPLY_THREAD={}", read.inbox[9].thread_name);
    println!("TASK4120_MAILBOX_UNCHANGED={}", mailbox == mailbox_before);
    println!("TASK4120_SHARED_MAILBOX_READS_BEFORE={count_before}");
    println!(
        "TASK4120_SHARED_MAILBOX_READS_AFTER={}",
        meter.completed_reads()
    );
}
