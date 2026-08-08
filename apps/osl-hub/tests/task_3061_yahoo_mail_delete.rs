#![cfg(feature = "core")]
//! TASK 3061 - delete marked Yahoo messages, driven from a seeded Yahoo mailbox.
//!
//! Finish line: Sent holds 3 messages matching SCRUB-YH-DEL and Trash holds 1
//! unrelated message before the run, Sent holds 2 and Trash holds 1 after.
//!
//! This is the same check the standalone crate
//! `apps/osl-hub/task_3061_yahoo_mail_delete/` runs against the same source
//! files, taken one step further back: the folders come from the mailbox
//! snapshot gate 3059's reader reads, through
//! `yahoo_trash_surface_from_mailbox`. It lives here so it runs under
//! `cargo test -p osl-hub` as soon as this branch's pre-existing merge damage
//! (27 files under `apps/osl-hub/src` do not parse, `services.rs` among them)
//! is repaired.

use osl_privacy_hub::scrub_hosted::yahoo_mail::yahoo_trash_surface_from_mailbox;
use osl_privacy_hub::scrub_hosted::yahoo_mail_delete::{
    delete_marked_yahoo_mail_message, YAHOO_MAIL_SENT_FOLDER_ID, YAHOO_MAIL_TRASH_FOLDER_ID,
};
use osl_privacy_hub::services::{
    MailboxFolderCandidate, MailboxMessageCandidate, MailboxReaderSnapshot,
};

const OWNER: &str = "osl_task_3061_owner";
const ACCOUNT: &str = "acct-task-3061-yahoo";
const SIGNED_IN_ADDRESS: &str = "owner@yahoo.example.test";
const MARKER: &str = "SCRUB-YH-DEL";
const MARKED_MESSAGE: &str = "sent-yh-3061-002";
const UNRELATED_TRASH_MESSAGE: &str = "trash-yh-3061-was-already-here";

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
                YAHOO_MAIL_SENT_FOLDER_ID,
                "sent-yh-3061-001",
                "SCRUB-YH-DEL renewal receipt",
                1_786_104_000,
                SIGNED_IN_ADDRESS,
                "Yahoo sent body one.",
            ),
            MailboxMessageCandidate::new(
                YAHOO_MAIL_SENT_FOLDER_ID,
                MARKED_MESSAGE,
                "SCRUB-YH-DEL address confirmation",
                1_786_107_600,
                SIGNED_IN_ADDRESS,
                "Yahoo sent body two - the one the review marked.",
            ),
            MailboxMessageCandidate::new(
                YAHOO_MAIL_SENT_FOLDER_ID,
                "sent-yh-3061-003",
                "SCRUB-YH-DEL travel plan",
                1_786_111_200,
                SIGNED_IN_ADDRESS,
                "Yahoo sent body three.",
            ),
            MailboxMessageCandidate::new(
                YAHOO_MAIL_TRASH_FOLDER_ID,
                UNRELATED_TRASH_MESSAGE,
                "Yahoo newsletter binned last week",
                1_786_010_400,
                "news@example.test",
                "An unrelated message that was already in Trash.",
            ),
        ],
    )
}

#[test]
fn task_3061_sent_holds_three_marked_and_trash_one_before_and_sent_two_and_trash_one_after() {
    let mut surface = yahoo_trash_surface_from_mailbox(OWNER, ACCOUNT, &seeded_yahoo_mailbox())
        .expect("the seeded Yahoo mailbox reads");

    let sent_before = surface.subject_matches_in_folder(YAHOO_MAIL_SENT_FOLDER_ID, MARKER);
    let trash_before = surface.message_ids_in(YAHOO_MAIL_TRASH_FOLDER_ID);
    println!(
        "TASK3061 before_sent_matching_{MARKER}_count={}",
        sent_before.len()
    );
    println!("TASK3061 before_trash_count={}", trash_before.len());

    assert_eq!(sent_before.len(), 3, "Sent holds 3 matching {MARKER}");
    assert_eq!(surface.message_ids_in(YAHOO_MAIL_SENT_FOLDER_ID).len(), 3);
    assert_eq!(
        trash_before,
        vec![UNRELATED_TRASH_MESSAGE.to_owned()],
        "Trash holds 1 unrelated message"
    );

    let receipt = delete_marked_yahoo_mail_message(
        &mut surface,
        SIGNED_IN_ADDRESS,
        YAHOO_MAIL_SENT_FOLDER_ID,
        MARKED_MESSAGE,
    )
    .expect("the marked Yahoo message is deleted");

    let sent_after = surface.subject_matches_in_folder(YAHOO_MAIL_SENT_FOLDER_ID, MARKER);
    let trash_after = surface.message_ids_in(YAHOO_MAIL_TRASH_FOLDER_ID);
    println!(
        "TASK3061 after_sent_matching_{MARKER}_count={}",
        sent_after.len()
    );
    println!("TASK3061 after_trash_count={}", trash_after.len());

    assert_eq!(sent_after.len(), 2, "Sent holds 2 after the run");
    assert!(!sent_after.contains(&MARKED_MESSAGE.to_owned()));
    assert_eq!(
        trash_after,
        vec![UNRELATED_TRASH_MESSAGE.to_owned()],
        "Trash holds 1 after the run, untouched"
    );
    assert_eq!(
        surface.count_in_folder(YAHOO_MAIL_TRASH_FOLDER_ID, MARKED_MESSAGE),
        0
    );
    assert_eq!(
        receipt.step_names(),
        vec!["move_to_trash", "remove_one_message_from_trash"]
    );
    assert!(!receipt.whole_trash_emptied);
}
