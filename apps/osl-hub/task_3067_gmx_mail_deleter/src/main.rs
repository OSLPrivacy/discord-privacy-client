//! TASK 3067 - the direct run of the shared mail deleter, filled in for GMX.
//!
//! Seeds the GMX mailbox the finish line is stated against - Sent holding three
//! messages matching `SCRUB-GX-DEL`, Trash holding one unrelated message that
//! was already there - runs the shared deleter once on the marked message, and
//! prints what the GMX folders hold afterwards, counted by walking the mailbox
//! rather than remembered by the deleter.
//!
//! The modules below are the real files under `apps/osl-hub/src/`, compiled by
//! path. Nothing here is a copy of them.
//!
//! Direct run:
//!   cargo run --manifest-path \
//!     apps/osl-hub/task_3067_gmx_mail_deleter/Cargo.toml
//!
//! The checks (they live beside the fill-in, in `gmx_mail_deleter.rs`):
//!   cargo test --manifest-path \
//!     apps/osl-hub/task_3067_gmx_mail_deleter/Cargo.toml task_3067 \
//!     -- --test-threads=1 --nocapture

#[path = "../../src/gmx_mail_deleter.rs"]
mod gmx_mail_deleter;
#[path = "../../src/mail_owner_check.rs"]
mod mail_owner_check;
#[path = "../../src/row_who_wrote_it.rs"]
mod row_who_wrote_it;
#[path = "../../src/shared_mail_deleter.rs"]
mod shared_mail_deleter;

use gmx_mail_deleter::{
    delete_marked_gmx_message, seeded_gmx_delete_mailbox, GMX_DELETE_SUBJECT_MARK,
    GMX_MARKED_MESSAGE_ID, GMX_SENT_FOLDER, GMX_SIGNED_IN_ADDRESS, GMX_TRASH_FOLDER,
    GMX_UNRELATED_TRASH_MESSAGE_ID,
};
use shared_mail_deleter::SharedMailTrashSurface;

fn main() {
    let mut mailbox = seeded_gmx_delete_mailbox();

    let sent_before = mailbox.count_in(GMX_SENT_FOLDER);
    let sent_marked_before = mailbox.count_matching_in(GMX_SENT_FOLDER, GMX_DELETE_SUBJECT_MARK);
    let trash_before = mailbox.count_in(GMX_TRASH_FOLDER);
    let trash_marked_before = mailbox.count_matching_in(GMX_TRASH_FOLDER, GMX_DELETE_SUBJECT_MARK);
    let trash_unrelated_before = trash_before - trash_marked_before;

    println!("TASK3067 service_id={}", mailbox.service_id());
    println!("TASK3067 folders={}", mailbox.folders().join(","));
    println!("TASK3067 trash_folder_id={}", mailbox.trash_folder_id());
    println!("TASK3067 signed_in_address={GMX_SIGNED_IN_ADDRESS}");
    println!("TASK3067 marked_message_id={GMX_MARKED_MESSAGE_ID}");
    println!("TASK3067 unrelated_trash_message_id={GMX_UNRELATED_TRASH_MESSAGE_ID}");
    println!("TASK3067 before_sent_count={sent_before}");
    println!("TASK3067 before_sent_matching_{GMX_DELETE_SUBJECT_MARK}={sent_marked_before}");
    println!(
        "TASK3067 before_sent_subjects=[{}]",
        mailbox.subjects_in(GMX_SENT_FOLDER).join(",")
    );
    println!("TASK3067 before_trash_count={trash_before}");
    println!("TASK3067 before_trash_matching_{GMX_DELETE_SUBJECT_MARK}={trash_marked_before}");
    println!("TASK3067 before_trash_unrelated_count={trash_unrelated_before}");
    println!(
        "TASK3067 before_trash_subjects=[{}]",
        mailbox.subjects_in(GMX_TRASH_FOLDER).join(",")
    );

    let receipt = match delete_marked_gmx_message(
        &mut mailbox,
        GMX_SIGNED_IN_ADDRESS,
        GMX_SENT_FOLDER,
        GMX_MARKED_MESSAGE_ID,
    ) {
        Ok(receipt) => receipt,
        Err(error) => {
            println!(
                "TASK3067 direct_run_refused={} {}",
                error.code(),
                error.reason()
            );
            std::process::exit(1);
        }
    };

    let sent_after = mailbox.count_in(GMX_SENT_FOLDER);
    let sent_marked_after = mailbox.count_matching_in(GMX_SENT_FOLDER, GMX_DELETE_SUBJECT_MARK);
    let trash_after = mailbox.count_in(GMX_TRASH_FOLDER);
    let trash_marked_after = mailbox.count_matching_in(GMX_TRASH_FOLDER, GMX_DELETE_SUBJECT_MARK);
    let trash_unrelated_after = trash_after - trash_marked_after;

    println!("TASK3067 steps={}", receipt.step_names().join(","));
    println!("TASK3067 after_sent_count={sent_after}");
    println!("TASK3067 after_sent_matching_{GMX_DELETE_SUBJECT_MARK}={sent_marked_after}");
    println!(
        "TASK3067 after_sent_subjects=[{}]",
        mailbox.subjects_in(GMX_SENT_FOLDER).join(",")
    );
    println!("TASK3067 after_trash_count={trash_after}");
    println!("TASK3067 after_trash_matching_{GMX_DELETE_SUBJECT_MARK}={trash_marked_after}");
    println!("TASK3067 after_trash_unrelated_count={trash_unrelated_after}");
    println!(
        "TASK3067 after_trash_subjects=[{}]",
        mailbox.subjects_in(GMX_TRASH_FOLDER).join(",")
    );
    println!(
        "TASK3067 after_trash_message_ids=[{}]",
        mailbox.message_ids_in(GMX_TRASH_FOLDER).join(",")
    );
    println!(
        "TASK3067 move_to_trash_calls=[{}]",
        mailbox.move_calls().join(",")
    );
    println!(
        "TASK3067 remove_from_trash_calls=[{}]",
        mailbox.remove_calls().join(",")
    );
    println!(
        "TASK3067 whole_trash_emptied={}",
        receipt.whole_trash_emptied
    );

    // The direct run reports a failure rather than printing a clean line it did
    // not earn.
    let mut failures = Vec::new();
    if sent_marked_before != 3 {
        failures.push(format!(
            "Sent held {sent_marked_before} messages matching {GMX_DELETE_SUBJECT_MARK} before the run"
        ));
    }
    if trash_unrelated_before != 1 {
        failures.push(format!(
            "Trash held {trash_unrelated_before} unrelated messages before the run"
        ));
    }
    if sent_after != 2 {
        failures.push(format!("Sent holds {sent_after} after the run"));
    }
    if trash_after != 1 {
        failures.push(format!("Trash holds {trash_after} after the run"));
    }
    if mailbox.message_ids_in(GMX_TRASH_FOLDER) != vec![GMX_UNRELATED_TRASH_MESSAGE_ID.to_owned()] {
        failures.push("the one message left in Trash is not the unrelated one".to_owned());
    }
    if receipt.whole_trash_emptied {
        failures.push("the whole trash was emptied".to_owned());
    }
    if failures.is_empty() {
        println!("TASK3067 direct_run=ok");
    } else {
        println!("TASK3067 direct_run_failed={}", failures.join("; "));
        std::process::exit(1);
    }
}
