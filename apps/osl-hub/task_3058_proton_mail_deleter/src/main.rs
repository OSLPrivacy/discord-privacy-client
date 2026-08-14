//! TASK 3058 - the direct run of the Proton Mail fill-in of the shared mail
//! deleter.
//!
//! Seeds one Proton mailbox the way gate 3056's seeded Proton mailbox is shaped
//! (folders Inbox, Sent, Archive, Trash; sender `scrub.owner@proton.test`) with
//! three Sent messages matching SCRUB-PR-DEL and one unrelated message already
//! sitting in Trash. Runs the shared deleter once against Proton on one of the
//! three, and prints what the folders hold afterwards, read back out of the
//! mailbox rather than remembered.
//!
//! The modules below are the real files under `apps/osl-hub/src/`, compiled by
//! path. Nothing here is a copy of them.
//!
//! Direct run:
//!   cargo run --manifest-path \
//!     apps/osl-hub/task_3058_proton_mail_deleter/Cargo.toml
//!
//! The checks (they live beside the fill-in, in
//! `src/scrub_hosted/proton_mail_deleter.rs`):
//!   cargo test --manifest-path \
//!     apps/osl-hub/task_3058_proton_mail_deleter/Cargo.toml task_3058 \
//!     -- --test-threads=1 --nocapture

// The shared modules are compiled whole, by path; this one command uses part of
// what they offer, so the rest is dead code here and nowhere else.
#![allow(dead_code)]

#[path = "../../src/mail_owner_check.rs"]
mod mail_owner_check;
#[path = "../../src/scrub_hosted/proton_mail_deleter.rs"]
mod proton_mail_deleter;
#[path = "../../src/row_who_wrote_it.rs"]
mod row_who_wrote_it;
#[path = "../../src/shared_mail_deleter.rs"]
mod shared_mail_deleter;

use proton_mail_deleter::{
    delete_marked_proton_mail_message, proton_mail_marked_delete_request, ProtonMailbox,
    ProtonMailboxMessage, PROTON_MAIL_TRASH_FOLDER_ID,
};
use shared_mail_deleter::SharedMailTrashSurface;

const SIGNED_IN: &str = "scrub.owner@proton.test";
const SENT: &str = "Sent";
const MARKER: &str = "SCRUB-PR-DEL";
const MARKED_MESSAGE: &str = "proton-3058-sent-002";
const UNRELATED_IN_TRASH: &str = "proton-3058-trash-was-already-here";
const UNRELATED_SUBJECT: &str = "Holiday photos from last summer";

fn seeded_proton_mailbox() -> ProtonMailbox {
    ProtonMailbox::new()
        .with_message(ProtonMailboxMessage::new(
            SENT,
            "proton-3058-sent-001",
            "SCRUB-PR-DEL export request",
            SIGNED_IN,
        ))
        .with_message(ProtonMailboxMessage::new(
            SENT,
            MARKED_MESSAGE,
            "SCRUB-PR-DEL erasure demand",
            SIGNED_IN,
        ))
        .with_message(ProtonMailboxMessage::new(
            SENT,
            "proton-3058-sent-003",
            "SCRUB-PR-DEL confirmation note",
            SIGNED_IN,
        ))
        .with_message(ProtonMailboxMessage::new(
            PROTON_MAIL_TRASH_FOLDER_ID,
            UNRELATED_IN_TRASH,
            UNRELATED_SUBJECT,
            SIGNED_IN,
        ))
}

fn main() {
    let mut mailbox = seeded_proton_mailbox();

    println!("TASK3058 service_id={}", mailbox.service_id());
    println!("TASK3058 trash_folder_id={}", mailbox.trash_folder_id());
    println!("TASK3058 folders={}", mailbox.folder_ids().join(","));
    println!("TASK3058 signed_in_address={SIGNED_IN}");
    println!("TASK3058 marker={MARKER}");
    println!("TASK3058 marked_message_id={MARKED_MESSAGE}");

    let sent_matching_before = mailbox.count_matching_in(SENT, MARKER);
    let trash_ids_before = mailbox.message_ids_in(PROTON_MAIL_TRASH_FOLDER_ID);
    let unrelated_in_trash_before =
        mailbox.count_of_message_in(PROTON_MAIL_TRASH_FOLDER_ID, UNRELATED_IN_TRASH);
    println!("TASK3058 before_sent_messages_matching_{MARKER}={sent_matching_before}");
    for subject in mailbox.subjects_matching_in(SENT, MARKER) {
        println!("TASK3058 before_sent_subject={subject}");
    }
    println!("TASK3058 before_trash_count={}", trash_ids_before.len());
    println!("TASK3058 before_trash_ids=[{}]", trash_ids_before.join(","));
    println!("TASK3058 before_trash_unrelated_copies={unrelated_in_trash_before}");

    let request = proton_mail_marked_delete_request(SIGNED_IN, SENT, MARKED_MESSAGE);
    let receipt = match delete_marked_proton_mail_message(&mut mailbox, &request) {
        Ok(receipt) => receipt,
        Err(error) => {
            println!(
                "TASK3058 direct_run_refused={} {}",
                error.code(),
                error.reason()
            );
            std::process::exit(1);
        }
    };

    let sent_matching_after = mailbox.count_matching_in(SENT, MARKER);
    let trash_ids_after = mailbox.message_ids_in(PROTON_MAIL_TRASH_FOLDER_ID);
    let unrelated_in_trash_after =
        mailbox.count_of_message_in(PROTON_MAIL_TRASH_FOLDER_ID, UNRELATED_IN_TRASH);

    println!(
        "TASK3058 direct_run_steps={}",
        receipt.step_names().join(",")
    );
    println!("TASK3058 after_sent_messages_matching_{MARKER}={sent_matching_after}");
    for subject in mailbox.subjects_matching_in(SENT, MARKER) {
        println!("TASK3058 after_sent_subject={subject}");
    }
    println!("TASK3058 after_trash_count={}", trash_ids_after.len());
    println!("TASK3058 after_trash_ids=[{}]", trash_ids_after.join(","));
    println!("TASK3058 after_trash_unrelated_copies={unrelated_in_trash_after}");
    println!(
        "TASK3058 after_sent_copies_of_marked_message={}",
        mailbox.count_of_message_in(SENT, MARKED_MESSAGE)
    );
    println!(
        "TASK3058 after_trash_copies_of_marked_message={}",
        mailbox.count_of_message_in(PROTON_MAIL_TRASH_FOLDER_ID, MARKED_MESSAGE)
    );
    println!(
        "TASK3058 move_to_trash_calls=[{}]",
        mailbox.move_calls().join(",")
    );
    println!(
        "TASK3058 permanent_delete_calls=[{}]",
        mailbox.permanent_delete_calls().join(",")
    );
    println!(
        "TASK3058 whole_trash_emptied={}",
        receipt.whole_trash_emptied
    );

    // The direct run reports a failure rather than printing a clean line it did
    // not earn.
    let mut failures = Vec::new();
    if sent_matching_before != 3 {
        failures.push(format!(
            "Sent held {sent_matching_before} messages matching {MARKER} before the run, not 3"
        ));
    }
    if trash_ids_before != vec![UNRELATED_IN_TRASH.to_owned()] {
        failures.push(format!(
            "Trash held [{}] before the run, not the one unrelated message",
            trash_ids_before.join(",")
        ));
    }
    if sent_matching_after != 2 {
        failures.push(format!(
            "Sent holds {sent_matching_after} messages matching {MARKER} after the run, not 2"
        ));
    }
    if trash_ids_after != vec![UNRELATED_IN_TRASH.to_owned()] {
        failures.push(format!(
            "Trash holds [{}] after the run, not the one unrelated message",
            trash_ids_after.join(",")
        ));
    }
    if receipt.whole_trash_emptied {
        failures.push("the whole trash was emptied".to_owned());
    }
    if mailbox.permanent_delete_calls() != [MARKED_MESSAGE.to_owned()] {
        failures.push(format!(
            "Proton was asked for permanent deletes [{}], not the one marked message",
            mailbox.permanent_delete_calls().join(",")
        ));
    }
    if failures.is_empty() {
        println!("TASK3058 direct_run=ok");
    } else {
        println!("TASK3058 direct_run_failed={}", failures.join("; "));
        std::process::exit(1);
    }
}
