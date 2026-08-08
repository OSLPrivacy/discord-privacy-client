//! TASK 3064 - direct run of the AOL Mail fill-in of the shared mail deleter.
//!
//! The modules below are the real backend files under `apps/osl-hub/src/`,
//! compiled by path. The runner seeds three matching Sent messages and one
//! unrelated Trash message, deletes one marked message, then reads both folders
//! back and fails unless the exact task finish line is present.

#![allow(dead_code)]

#[path = "../../src/scrub_hosted/aol_mail_deleter.rs"]
mod aol_mail_deleter;
#[path = "../../src/mail_owner_check.rs"]
mod mail_owner_check;
#[path = "../../src/row_who_wrote_it.rs"]
mod row_who_wrote_it;
#[path = "../../src/shared_mail_deleter.rs"]
mod shared_mail_deleter;

use aol_mail_deleter::{
    aol_mail_marked_delete_request, delete_marked_aol_mail_message, AolMailbox, AolMailboxMessage,
    AOL_MAIL_TRASH_FOLDER_ID,
};
use shared_mail_deleter::SharedMailTrashSurface;

const SIGNED_IN: &str = "scrub.owner@aol.example.test";
const SENT: &str = "Sent";
const MARKER: &str = "SCRUB-AO-DEL";
const MARKED_MESSAGE: &str = "aol-3064-sent-002";
const UNRELATED_IN_TRASH: &str = "aol-3064-trash-was-already-here";

fn seeded_aol_mailbox() -> AolMailbox {
    AolMailbox::new()
        .with_message(AolMailboxMessage::new(
            SENT,
            "aol-3064-sent-001",
            "SCRUB-AO-DEL export request",
            SIGNED_IN,
        ))
        .with_message(AolMailboxMessage::new(
            SENT,
            MARKED_MESSAGE,
            "SCRUB-AO-DEL erasure demand",
            SIGNED_IN,
        ))
        .with_message(AolMailboxMessage::new(
            SENT,
            "aol-3064-sent-003",
            "SCRUB-AO-DEL confirmation note",
            SIGNED_IN,
        ))
        .with_message(AolMailboxMessage::new(
            AOL_MAIL_TRASH_FOLDER_ID,
            UNRELATED_IN_TRASH,
            "Holiday photos from last summer",
            SIGNED_IN,
        ))
}

fn main() {
    let mut mailbox = seeded_aol_mailbox();

    println!("TASK3064 service_id={}", mailbox.service_id());
    println!("TASK3064 trash_folder_id={}", mailbox.trash_folder_id());
    println!("TASK3064 folders={}", mailbox.folder_ids().join(","));
    println!("TASK3064 marker={MARKER}");
    println!("TASK3064 marked_message_id={MARKED_MESSAGE}");

    let sent_matching_before = mailbox.count_matching_in(SENT, MARKER);
    let trash_ids_before = mailbox.message_ids_in(AOL_MAIL_TRASH_FOLDER_ID);
    let unrelated_trash_before =
        mailbox.count_of_message_in(AOL_MAIL_TRASH_FOLDER_ID, UNRELATED_IN_TRASH);
    println!("TASK3064 before_sent_messages_matching_{MARKER}={sent_matching_before}");
    println!("TASK3064 before_trash_count={}", trash_ids_before.len());
    println!("TASK3064 before_trash_ids=[{}]", trash_ids_before.join(","));
    println!("TASK3064 before_trash_unrelated_copies={unrelated_trash_before}");

    let request = aol_mail_marked_delete_request(SIGNED_IN, SENT, MARKED_MESSAGE);
    let receipt = match delete_marked_aol_mail_message(&mut mailbox, &request) {
        Ok(receipt) => receipt,
        Err(error) => {
            println!(
                "TASK3064 direct_run_refused={} {}",
                error.code(),
                error.reason()
            );
            std::process::exit(1);
        }
    };

    let sent_matching_after = mailbox.count_matching_in(SENT, MARKER);
    let trash_ids_after = mailbox.message_ids_in(AOL_MAIL_TRASH_FOLDER_ID);
    let unrelated_trash_after =
        mailbox.count_of_message_in(AOL_MAIL_TRASH_FOLDER_ID, UNRELATED_IN_TRASH);
    println!(
        "TASK3064 direct_run_steps={}",
        receipt.step_names().join(",")
    );
    println!("TASK3064 after_sent_messages_matching_{MARKER}={sent_matching_after}");
    println!("TASK3064 after_trash_count={}", trash_ids_after.len());
    println!("TASK3064 after_trash_ids=[{}]", trash_ids_after.join(","));
    println!("TASK3064 after_trash_unrelated_copies={unrelated_trash_after}");
    println!(
        "TASK3064 after_sent_copies_of_marked_message={}",
        mailbox.count_of_message_in(SENT, MARKED_MESSAGE)
    );
    println!(
        "TASK3064 after_trash_copies_of_marked_message={}",
        mailbox.count_of_message_in(AOL_MAIL_TRASH_FOLDER_ID, MARKED_MESSAGE)
    );
    println!(
        "TASK3064 move_to_trash_calls=[{}]",
        mailbox.move_calls().join(",")
    );
    println!(
        "TASK3064 flag_deleted_calls=[{}]",
        mailbox.flag_deleted_calls().join(",")
    );
    println!(
        "TASK3064 expunge_calls=[{}]",
        mailbox.expunge_calls().join(",")
    );
    println!(
        "TASK3064 whole_trash_emptied={}",
        receipt.whole_trash_emptied
    );

    let mut failures = Vec::new();
    if sent_matching_before != 3 {
        failures.push(format!(
            "Sent held {sent_matching_before} messages matching {MARKER} before, not 3"
        ));
    }
    if trash_ids_before != [UNRELATED_IN_TRASH] {
        failures.push(format!(
            "Trash held [{}] before, not the one unrelated message",
            trash_ids_before.join(",")
        ));
    }
    if sent_matching_after != 2 {
        failures.push(format!(
            "Sent held {sent_matching_after} messages matching {MARKER} after, not 2"
        ));
    }
    if trash_ids_after != [UNRELATED_IN_TRASH] {
        failures.push(format!(
            "Trash held [{}] after, not the one unrelated message",
            trash_ids_after.join(",")
        ));
    }
    if unrelated_trash_before != 1 || unrelated_trash_after != 1 {
        failures.push("the unrelated Trash message was not preserved exactly once".to_owned());
    }
    if receipt.whole_trash_emptied {
        failures.push("the whole trash was emptied".to_owned());
    }
    if mailbox.flag_deleted_calls() != [MARKED_MESSAGE] {
        failures.push("AOL did not flag exactly the marked message by id".to_owned());
    }
    if mailbox.expunge_calls() != [AOL_MAIL_TRASH_FOLDER_ID] {
        failures.push("AOL did not expunge exactly Trash".to_owned());
    }

    if failures.is_empty() {
        println!("TASK3064 direct_run=ok");
    } else {
        println!("TASK3064 direct_run_failed={}", failures.join("; "));
        std::process::exit(1);
    }
}
