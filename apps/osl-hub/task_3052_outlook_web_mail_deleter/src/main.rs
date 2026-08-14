//! TASK 3052 - the direct run of the Outlook web fill-in of the shared mail
//! deleter.
//!
//! Seeds the Outlook web mailbox gate 3050 reads -- Inbox, Sent Items, Archive,
//! Deleted Items -- with three Sent Items messages matching `SCRUB-OW-DEL` and
//! one unrelated message already sitting in Deleted Items, deletes one marked
//! message, and prints what each folder holds before and after. Every count is
//! read back out of the surface, not remembered by the deleter.
//!
//! The modules below are the real files under `apps/osl-hub/src/`, compiled by
//! path. Nothing here is a copy of them.
//!
//! Direct run:
//!   cargo run --manifest-path \
//!     apps/osl-hub/task_3052_outlook_web_mail_deleter/Cargo.toml
//!
//! The checks (they live beside the fill-in, in `outlook_web_mail_deleter.rs`):
//!   cargo test --manifest-path \
//!     apps/osl-hub/task_3052_outlook_web_mail_deleter/Cargo.toml task_3052 \
//!     -- --test-threads=1 --nocapture

#[path = "../../src/mail_owner_check.rs"]
mod mail_owner_check;
#[path = "../../src/outlook_web_mail_deleter.rs"]
mod outlook_web_mail_deleter;
#[path = "../../src/row_who_wrote_it.rs"]
mod row_who_wrote_it;
#[path = "../../src/shared_mail_deleter.rs"]
mod shared_mail_deleter;

use outlook_web_mail_deleter::{
    delete_marked_outlook_web_message, seeded_outlook_web_scrub_mailbox,
    OUTLOOK_WEB_ALREADY_IN_DELETED_ITEMS_ID, OUTLOOK_WEB_DELETED_ITEMS_FOLDER,
    OUTLOOK_WEB_DELETE_MARKER, OUTLOOK_WEB_MARKED_MESSAGE_ID, OUTLOOK_WEB_SCRUB_SIGNED_IN_ADDRESS,
    OUTLOOK_WEB_SENT_ITEMS_FOLDER,
};
use shared_mail_deleter::SharedMailTrashSurface;

const SENT: &str = OUTLOOK_WEB_SENT_ITEMS_FOLDER;
const DELETED: &str = OUTLOOK_WEB_DELETED_ITEMS_FOLDER;

fn main() {
    let mut surface = seeded_outlook_web_scrub_mailbox();

    println!("TASK3052 service_id={}", surface.service_id());
    println!("TASK3052 folder_pane=[{}]", surface.folders().join(","));
    println!(
        "TASK3052 trash_folder_id_from_surface={}",
        surface.trash_folder_id()
    );
    println!("TASK3052 signed_in_address={OUTLOOK_WEB_SCRUB_SIGNED_IN_ADDRESS}");
    println!("TASK3052 marker={OUTLOOK_WEB_DELETE_MARKER}");
    println!("TASK3052 marked_message_id={OUTLOOK_WEB_MARKED_MESSAGE_ID}");

    let sent_matching_before = surface.count_matching_subject(SENT, OUTLOOK_WEB_DELETE_MARKER);
    let sent_total_before = surface.rows_in_folder(SENT).len();
    let deleted_items_before = surface.rows_in_folder(DELETED).len();
    let deleted_items_matching_before =
        surface.count_matching_subject(DELETED, OUTLOOK_WEB_DELETE_MARKER);

    println!("TASK3052 before sent_items_messages={sent_total_before}");
    println!(
        "TASK3052 before sent_items_matching_{OUTLOOK_WEB_DELETE_MARKER}={sent_matching_before}"
    );
    println!(
        "TASK3052 before sent_items_subjects=[{}]",
        surface.subjects_in(SENT).join(",")
    );
    println!("TASK3052 before deleted_items_messages={deleted_items_before}");
    println!(
        "TASK3052 before deleted_items_matching_{OUTLOOK_WEB_DELETE_MARKER}={deleted_items_matching_before}"
    );
    println!(
        "TASK3052 before deleted_items_ids=[{}]",
        surface.message_ids_in(DELETED).join(",")
    );
    println!(
        "TASK3052 before deleted_items_subjects=[{}]",
        surface.subjects_in(DELETED).join(",")
    );

    let receipt = match delete_marked_outlook_web_message(
        &mut surface,
        OUTLOOK_WEB_SCRUB_SIGNED_IN_ADDRESS,
        SENT,
        OUTLOOK_WEB_MARKED_MESSAGE_ID,
    ) {
        Ok(receipt) => receipt,
        Err(error) => {
            println!(
                "TASK3052 direct_run_refused={} {}",
                error.code(),
                error.reason()
            );
            std::process::exit(1);
        }
    };

    let sent_matching_after = surface.count_matching_subject(SENT, OUTLOOK_WEB_DELETE_MARKER);
    let deleted_items_after = surface.rows_in_folder(DELETED).len();

    println!("TASK3052 steps={}", receipt.step_names().join(","));
    println!(
        "TASK3052 receipt_trash_folder_id={}",
        receipt.trash_folder_id
    );
    println!(
        "TASK3052 after sent_items_matching_{OUTLOOK_WEB_DELETE_MARKER}={sent_matching_after}"
    );
    println!(
        "TASK3052 after sent_items_subjects=[{}]",
        surface.subjects_in(SENT).join(",")
    );
    println!("TASK3052 after deleted_items_messages={deleted_items_after}");
    println!(
        "TASK3052 after deleted_items_ids=[{}]",
        surface.message_ids_in(DELETED).join(",")
    );
    println!(
        "TASK3052 after copies_of_marked_message_in_sent_items={}",
        surface.count_in_folder(SENT, OUTLOOK_WEB_MARKED_MESSAGE_ID)
    );
    println!(
        "TASK3052 after copies_of_marked_message_in_deleted_items={}",
        surface.count_in_folder(DELETED, OUTLOOK_WEB_MARKED_MESSAGE_ID)
    );
    println!(
        "TASK3052 move_to_deleted_items_calls=[{}]",
        surface.move_to_deleted_items_calls().join(",")
    );
    println!(
        "TASK3052 delete_from_deleted_items_calls=[{}]",
        surface.delete_from_deleted_items_calls().join(",")
    );
    println!(
        "TASK3052 whole_trash_emptied={}",
        receipt.whole_trash_emptied
    );
    println!(
        "TASK3052 other_deleted_items_before={} other_deleted_items_after={}",
        receipt.other_trash_messages_before, receipt.other_trash_messages_after
    );

    // The direct run reports a failure rather than printing a clean line it did
    // not earn.
    let mut failures = Vec::new();
    if sent_matching_before != 3 {
        failures.push(format!(
            "{SENT} held {sent_matching_before} messages matching {OUTLOOK_WEB_DELETE_MARKER} before the run, not 3"
        ));
    }
    if deleted_items_before != 1 || deleted_items_matching_before != 0 {
        failures.push(format!(
            "{DELETED} held {deleted_items_before} messages before the run ({deleted_items_matching_before} of them matching {OUTLOOK_WEB_DELETE_MARKER}), not 1 unrelated one"
        ));
    }
    if sent_matching_after != 2 {
        failures.push(format!(
            "{SENT} holds {sent_matching_after} messages matching {OUTLOOK_WEB_DELETE_MARKER} after the run, not 2"
        ));
    }
    if deleted_items_after != 1 {
        failures.push(format!(
            "{DELETED} holds {deleted_items_after} messages after the run, not 1"
        ));
    }
    if surface.message_ids_in(DELETED) != vec![OUTLOOK_WEB_ALREADY_IN_DELETED_ITEMS_ID.to_owned()] {
        failures.push(format!(
            "{DELETED} no longer holds exactly the message that was already there"
        ));
    }
    if receipt.whole_trash_emptied {
        failures.push(format!("the whole of {DELETED} was emptied"));
    }

    if failures.is_empty() {
        println!("TASK3052 direct_run=ok");
    } else {
        println!("TASK3052 direct_run_failed={}", failures.join("; "));
        std::process::exit(1);
    }
}
