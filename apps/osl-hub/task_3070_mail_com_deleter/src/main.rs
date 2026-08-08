//! TASK 3070 - the direct run of the shared mail deleter for Mail.com.
//!
//! Seeds Mail.com's Sent folder with three `SCRUB-MC-DEL` messages, one of them
//! marked by the review, and Mail.com's Trash with one unrelated message that
//! was already sitting there. Runs the shared deleter (TASK 3045) once through
//! Mail.com's fill-in, and prints what the folders hold before and after - read
//! back out of the mailbox, not remembered by the deleter.
//!
//! The modules below are the real files under `apps/osl-hub/src/`, compiled by
//! path. Nothing here is a copy of them.
//!
//! Direct run:
//!   cargo run --manifest-path \
//!     apps/osl-hub/task_3070_mail_com_deleter/Cargo.toml
//!
//! The checks (they live beside the fill-in, in `mail_com_mail_deleter.rs`):
//!   cargo test --manifest-path \
//!     apps/osl-hub/task_3070_mail_com_deleter/Cargo.toml task_3070 \
//!     -- --test-threads=1 --nocapture

#[path = "../../src/mail_com_mail_deleter.rs"]
mod mail_com_mail_deleter;
#[path = "../../src/mail_owner_check.rs"]
mod mail_owner_check;
#[path = "../../src/row_who_wrote_it.rs"]
mod row_who_wrote_it;
#[path = "../../src/shared_mail_deleter.rs"]
mod shared_mail_deleter;

use mail_com_mail_deleter::{
    mail_com_marked_delete_request, seeded_mail_com_mailbox_for_deletion,
    MAIL_COM_DELETION_MARKER, MAIL_COM_MARKED_MESSAGE_ID, MAIL_COM_SENT_FOLDER_ID,
    MAIL_COM_SERVICE_ID, MAIL_COM_TRASH_FOLDER_ID, MAIL_COM_UNRELATED_TRASH_MESSAGE_ID,
};
use shared_mail_deleter::delete_marked_mail_message;

fn main() {
    let mut mailbox = seeded_mail_com_mailbox_for_deletion();

    println!("TASK3070 service_id={MAIL_COM_SERVICE_ID}");
    println!("TASK3070 folder_id={MAIL_COM_SENT_FOLDER_ID}");
    println!("TASK3070 trash_folder_id={MAIL_COM_TRASH_FOLDER_ID}");
    println!("TASK3070 marker={MAIL_COM_DELETION_MARKER}");
    println!("TASK3070 marked_message_id={MAIL_COM_MARKED_MESSAGE_ID}");
    println!("TASK3070 unrelated_trash_message_id={MAIL_COM_UNRELATED_TRASH_MESSAGE_ID}");
    println!(
        "TASK3070 before_sent_matching_{MAIL_COM_DELETION_MARKER}={}",
        mailbox.folder_count_matching(MAIL_COM_SENT_FOLDER_ID, MAIL_COM_DELETION_MARKER)
    );
    println!(
        "TASK3070 before_sent_ids={}",
        mailbox.folder_message_ids(MAIL_COM_SENT_FOLDER_ID).join(",")
    );
    println!(
        "TASK3070 before_trash_count={}",
        mailbox.folder_count(MAIL_COM_TRASH_FOLDER_ID)
    );
    println!(
        "TASK3070 before_trash_matching_{MAIL_COM_DELETION_MARKER}={}",
        mailbox.folder_count_matching(MAIL_COM_TRASH_FOLDER_ID, MAIL_COM_DELETION_MARKER)
    );
    println!(
        "TASK3070 before_trash_ids={}",
        mailbox
            .folder_message_ids(MAIL_COM_TRASH_FOLDER_ID)
            .join(",")
    );

    match delete_marked_mail_message(&mut mailbox, &mail_com_marked_delete_request()) {
        Ok(receipt) => {
            println!("TASK3070 direct_run_steps={}", receipt.step_names().join(","));
            println!(
                "TASK3070 after_sent_matching_{MAIL_COM_DELETION_MARKER}={}",
                mailbox.folder_count_matching(MAIL_COM_SENT_FOLDER_ID, MAIL_COM_DELETION_MARKER)
            );
            println!(
                "TASK3070 after_sent_ids={}",
                mailbox.folder_message_ids(MAIL_COM_SENT_FOLDER_ID).join(",")
            );
            println!(
                "TASK3070 after_trash_count={}",
                mailbox.folder_count(MAIL_COM_TRASH_FOLDER_ID)
            );
            println!(
                "TASK3070 after_trash_ids={}",
                mailbox
                    .folder_message_ids(MAIL_COM_TRASH_FOLDER_ID)
                    .join(",")
            );
            println!(
                "TASK3070 after_sent_copies_of_marked_message={}",
                mailbox.count_in_folder(MAIL_COM_SENT_FOLDER_ID, MAIL_COM_MARKED_MESSAGE_ID)
            );
            println!(
                "TASK3070 after_trash_copies_of_marked_message={}",
                mailbox.count_in_folder(MAIL_COM_TRASH_FOLDER_ID, MAIL_COM_MARKED_MESSAGE_ID)
            );
            println!(
                "TASK3070 after_trash_copies_of_unrelated_message={}",
                mailbox.count_in_folder(
                    MAIL_COM_TRASH_FOLDER_ID,
                    MAIL_COM_UNRELATED_TRASH_MESSAGE_ID
                )
            );
            println!("TASK3070 move_calls={:?}", mailbox.move_calls());
            println!("TASK3070 remove_from_trash_calls={:?}", mailbox.remove_calls());
            println!(
                "TASK3070 whole_trash_emptied={}",
                receipt.whole_trash_emptied
            );
            println!(
                "TASK3070 other_trash_messages_before={} other_trash_messages_after={}",
                receipt.other_trash_messages_before, receipt.other_trash_messages_after
            );
            println!("TASK3070 direct_run=ok");
        }
        Err(error) => {
            println!("TASK3070 direct_run=refused code={}", error.code());
            println!("TASK3070 direct_run_reason={}", error.reason());
            std::process::exit(1);
        }
    }
}
