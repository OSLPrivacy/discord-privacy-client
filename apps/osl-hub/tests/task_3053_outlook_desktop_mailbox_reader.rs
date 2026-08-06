#![cfg(feature = "core")]

use osl_privacy_hub::native_outlook_adapter::{
    seeded_outlook_desktop_scrub_mailbox, OUTLOOK_DESKTOP_MAIL_READER_ID,
    OUTLOOK_DESKTOP_MINE_MARKER,
};
use osl_privacy_hub::service_connections::{
    mail_message_is_owned_by_signed_in_address, VisibleMailMessage,
};

fn ownership_label(owned: bool) -> &'static str {
    if owned {
        "yours"
    } else {
        "not-yours"
    }
}

#[test]
fn task_3053_outlook_desktop_seeded_mailbox_returns_folders_sent_messages_and_mine_verdicts() {
    let mailbox = seeded_outlook_desktop_scrub_mailbox();
    let folders = mailbox
        .read_folders()
        .expect("seeded Outlook desktop mailbox folders are readable");
    let sent = mailbox
        .read_messages("Sent Items")
        .expect("seeded Outlook desktop Sent Items is readable");
    let inbox = mailbox
        .read_messages("Inbox")
        .expect("seeded Outlook desktop Inbox is readable");

    println!("TASK3053_DIRECT_READER={OUTLOOK_DESKTOP_MAIL_READER_ID}");
    println!("TASK3053_SERVICE=outlook-desktop");
    println!("TASK3053_FOLDER_COUNT={}", folders.len());
    for folder in &folders {
        println!(
            "TASK3053_FOLDER id={} label={} service={} account={}",
            folder.folder_id, folder.label, folder.service_id, folder.account_id
        );
    }

    println!("TASK3053_SENT_ITEMS_MESSAGE_COUNT={}", sent.len());
    for message in &sent {
        println!(
            "TASK3053_SENT_ITEMS_MESSAGE id={} subject={} time={} sender={}",
            message.message_id, message.subject, message.time, message.sender
        );
    }

    let mine = sent
        .iter()
        .find(|message| message.subject == OUTLOOK_DESKTOP_MINE_MARKER)
        .expect("seeded Sent Items contains the SCRUB-OD-MINE marker");
    let mine_opened = mailbox
        .open_message("Sent Items", &mine.message_id)
        .expect("marked Outlook desktop message opens");
    let mine_owned = mail_message_is_owned_by_signed_in_address(
        mailbox.signed_in_address(),
        &VisibleMailMessage {
            message_id: mine_opened.message_id.clone(),
            mailbox: mine_opened.folder_id.clone(),
            sender_address: Some(mine_opened.sender.clone()),
        },
    )
    .expect("marked Outlook desktop message sender is readable");
    println!(
        "TASK3053_MARKED_MESSAGE subject={} ownership={}",
        mine_opened.subject,
        ownership_label(mine_owned)
    );

    let inbox_ownership = inbox
        .iter()
        .map(|message| {
            let owned = mail_message_is_owned_by_signed_in_address(
                mailbox.signed_in_address(),
                &VisibleMailMessage {
                    message_id: message.message_id.clone(),
                    mailbox: message.folder_id.clone(),
                    sender_address: Some(message.sender.clone()),
                },
            )
            .expect("seeded Inbox message sender is readable");
            println!(
                "TASK3053_INBOX_MESSAGE id={} subject={} sender={} ownership={}",
                message.message_id,
                message.subject,
                message.sender,
                ownership_label(owned)
            );
            owned
        })
        .collect::<Vec<_>>();
    let inbox_not_yours = inbox_ownership.iter().filter(|owned| !**owned).count();
    println!("TASK3053_INBOX_MESSAGE_COUNT={}", inbox.len());
    println!("TASK3053_INBOX_NOT_YOURS_COUNT={inbox_not_yours}");

    assert_eq!(folders.len(), 4);
    assert_eq!(
        folders
            .iter()
            .map(|folder| folder.folder_id.as_str())
            .collect::<Vec<_>>(),
        vec!["Inbox", "Sent Items", "Archive", "Deleted Items"]
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
            ("SCRUB-OD-MINE", 1_786_032_000, "scrub.owner@example.test"),
            (
                "Outlook desktop cleanup receipt",
                1_786_035_600,
                "delegate@example.test",
            ),
            (
                "Outlook desktop account notice",
                1_786_039_200,
                "noreply@example.test",
            ),
        ]
    );
    assert!(mine_owned, "SCRUB-OD-MINE must be called yours");
    assert_eq!(inbox.len(), 2);
    assert_eq!(inbox_not_yours, 2);
}
