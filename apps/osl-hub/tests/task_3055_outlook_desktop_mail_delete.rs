#![cfg(feature = "core")]
//! TASK 3055: the review mailbox and deleter operate on the same Outlook rows.

use osl_privacy_hub::native_outlook_adapter::{
    outlook_desktop_trash_surface_from_mailbox, OutlookDesktopMailbox,
};
use osl_privacy_hub::native_outlook_desktop_mail_delete::{
    delete_marked_outlook_desktop_mail_message, OUTLOOK_DESKTOP_DELETED_ITEMS_FOLDER_ID,
    OUTLOOK_DESKTOP_DEL_MARKER, OUTLOOK_DESKTOP_SENT_ITEMS_FOLDER_ID,
};
use osl_privacy_hub::services::{
    MailboxFolderCandidate, MailboxMessageCandidate, MailboxReaderSnapshot,
};

const OWNER: &str = "osl_task_3055_owner";
const ACCOUNT: &str = "acct-task-3055-outlook-desktop";
const SIGNED_IN_ADDRESS: &str = "scrub.owner@example.test";
const MARKED_MESSAGE: &str = "outlook-desktop-sent-3055-002";
const UNRELATED_DELETED_ITEMS_MESSAGE: &str = "outlook-desktop-deleted-3055-was-already-here";

fn seeded_outlook_desktop_mailbox() -> OutlookDesktopMailbox {
    OutlookDesktopMailbox::new(
        OWNER,
        ACCOUNT,
        SIGNED_IN_ADDRESS,
        MailboxReaderSnapshot::new(
            [
                MailboxFolderCandidate::new("Inbox", "Inbox"),
                MailboxFolderCandidate::new(
                    OUTLOOK_DESKTOP_SENT_ITEMS_FOLDER_ID,
                    OUTLOOK_DESKTOP_SENT_ITEMS_FOLDER_ID,
                ),
                MailboxFolderCandidate::new(
                    OUTLOOK_DESKTOP_DELETED_ITEMS_FOLDER_ID,
                    OUTLOOK_DESKTOP_DELETED_ITEMS_FOLDER_ID,
                ),
            ],
            [
                MailboxMessageCandidate::new(
                    OUTLOOK_DESKTOP_SENT_ITEMS_FOLDER_ID,
                    "outlook-desktop-sent-3055-001",
                    "SCRUB-OD-DEL renewal receipt",
                    1_786_104_000,
                    SIGNED_IN_ADDRESS,
                    "Outlook desktop Sent Items body one.",
                ),
                MailboxMessageCandidate::new(
                    OUTLOOK_DESKTOP_SENT_ITEMS_FOLDER_ID,
                    MARKED_MESSAGE,
                    "SCRUB-OD-DEL address confirmation",
                    1_786_107_600,
                    SIGNED_IN_ADDRESS,
                    "Outlook desktop Sent Items body two - the one the review marked.",
                ),
                MailboxMessageCandidate::new(
                    OUTLOOK_DESKTOP_SENT_ITEMS_FOLDER_ID,
                    "outlook-desktop-sent-3055-003",
                    "SCRUB-OD-DEL travel plan",
                    1_786_111_200,
                    SIGNED_IN_ADDRESS,
                    "Outlook desktop Sent Items body three.",
                ),
                MailboxMessageCandidate::new(
                    OUTLOOK_DESKTOP_DELETED_ITEMS_FOLDER_ID,
                    UNRELATED_DELETED_ITEMS_MESSAGE,
                    "Outlook desktop newsletter deleted last week",
                    1_786_010_400,
                    "news@example.test",
                    "An unrelated message that was already in Deleted Items.",
                ),
            ],
        ),
    )
}

#[test]
fn task_3055_preserves_exact_deleted_items_count_and_removes_only_the_named_message() {
    let mailbox = seeded_outlook_desktop_mailbox();
    let mut surface = outlook_desktop_trash_surface_from_mailbox(&mailbox)
        .expect("the seeded Outlook desktop mailbox reads");

    let sent_before = surface.subject_matches_in_folder(
        OUTLOOK_DESKTOP_SENT_ITEMS_FOLDER_ID,
        OUTLOOK_DESKTOP_DEL_MARKER,
    );
    let deleted_before = surface.message_ids_in(OUTLOOK_DESKTOP_DELETED_ITEMS_FOLDER_ID);
    println!(
        "TASK3055 before_sent_items_matching_{OUTLOOK_DESKTOP_DEL_MARKER}_count={}",
        sent_before.len()
    );
    println!(
        "TASK3055 before_deleted_items_count={}",
        deleted_before.len()
    );
    assert_eq!(
        sent_before.len(),
        3,
        "Sent Items contains exactly 3 marked messages before"
    );
    assert_eq!(
        deleted_before.len(),
        1,
        "Deleted Items contains exactly 1 before"
    );
    assert_eq!(
        deleted_before,
        vec![UNRELATED_DELETED_ITEMS_MESSAGE.to_owned()]
    );

    let receipt = delete_marked_outlook_desktop_mail_message(
        &mut surface,
        mailbox.signed_in_address(),
        OUTLOOK_DESKTOP_SENT_ITEMS_FOLDER_ID,
        MARKED_MESSAGE,
    )
    .expect("the marked Outlook desktop message is deleted");

    let sent_after = surface.subject_matches_in_folder(
        OUTLOOK_DESKTOP_SENT_ITEMS_FOLDER_ID,
        OUTLOOK_DESKTOP_DEL_MARKER,
    );
    let deleted_after = surface.message_ids_in(OUTLOOK_DESKTOP_DELETED_ITEMS_FOLDER_ID);
    println!("TASK3055 run_steps={}", receipt.step_names().join(","));
    println!(
        "TASK3055 remove_from_deleted_items_calls=[{}]",
        surface.remove_calls().join(",")
    );
    println!(
        "TASK3055 after_sent_items_matching_{OUTLOOK_DESKTOP_DEL_MARKER}_count={}",
        sent_after.len()
    );
    println!("TASK3055 after_deleted_items_count={}", deleted_after.len());
    assert_eq!(
        sent_after.len(),
        2,
        "Sent Items contains exactly 2 marked messages after"
    );
    assert_eq!(
        deleted_after.len(),
        1,
        "Deleted Items contains exactly 1 after"
    );
    assert_eq!(
        deleted_after,
        vec![UNRELATED_DELETED_ITEMS_MESSAGE.to_owned()]
    );
    assert!(!sent_after.contains(&MARKED_MESSAGE.to_owned()));
    assert_eq!(
        surface.count_in_folder(OUTLOOK_DESKTOP_DELETED_ITEMS_FOLDER_ID, MARKED_MESSAGE),
        0
    );
    assert_eq!(surface.remove_calls(), [MARKED_MESSAGE.to_owned()]);
    assert_eq!(
        receipt.step_names(),
        vec!["move_to_trash", "remove_one_message_from_trash"]
    );
    assert_eq!(receipt.other_trash_messages_before, 1);
    assert_eq!(receipt.other_trash_messages_after, 1);
    assert!(!receipt.whole_trash_emptied);
}
