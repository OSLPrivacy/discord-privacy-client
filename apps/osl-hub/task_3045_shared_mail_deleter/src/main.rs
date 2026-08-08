//! TASK 3045 - the direct run of the shared mail deleter.
//!
//! Seeds one mail folder holding the marked message and a trash folder that
//! already holds a different message, runs the shared deleter once, and prints
//! what the folders hold afterwards - read back out of the folder store, not
//! remembered by the deleter.
//!
//! The modules below are the real files under `apps/osl-hub/src/`, compiled by
//! path. Nothing here is a copy of them.
//!
//! Direct run:
//!   cargo run --manifest-path \
//!     apps/osl-hub/task_3045_shared_mail_deleter/Cargo.toml
//!
//! The checks (they live beside the deleter, in `shared_mail_deleter.rs`):
//!   cargo test --manifest-path \
//!     apps/osl-hub/task_3045_shared_mail_deleter/Cargo.toml task_3045 \
//!     -- --test-threads=1 --nocapture

#[path = "../../src/mail_owner_check.rs"]
mod mail_owner_check;
#[path = "../../src/row_who_wrote_it.rs"]
mod row_who_wrote_it;
#[path = "../../src/shared_mail_deleter.rs"]
mod shared_mail_deleter;

use shared_mail_deleter::{
    delete_marked_mail_message, SharedMailDeleteRequest, SharedMailFolderStore, StoredMailMessage,
};

const SERVICE_ID: &str = "gmail";
const SIGNED_IN: &str = "owner@example.test";
const FOLDER: &str = "Sent";
const TRASH: &str = "Trash";
const MARKED_MESSAGE: &str = "sent-task-3045-marked";
const ALREADY_IN_TRASH: &str = "trash-task-3045-was-already-here";

fn main() {
    let mut store = SharedMailFolderStore::new(SERVICE_ID, TRASH)
        .with_message(StoredMailMessage::new(FOLDER, MARKED_MESSAGE, SIGNED_IN))
        .with_message(StoredMailMessage::new(TRASH, ALREADY_IN_TRASH, SIGNED_IN));

    println!("TASK3045 service_id={SERVICE_ID}");
    println!("TASK3045 folder_id={FOLDER}");
    println!("TASK3045 trash_folder_id={TRASH}");
    println!("TASK3045 marked_message_id={MARKED_MESSAGE}");
    println!("TASK3045 second_message_already_in_trash_id={ALREADY_IN_TRASH}");
    println!(
        "TASK3045 before_folder_copies_of_marked_message={}",
        store.count_in_folder(FOLDER, MARKED_MESSAGE)
    );
    println!(
        "TASK3045 before_trash_copies_of_marked_message={}",
        store.count_in_folder(TRASH, MARKED_MESSAGE)
    );
    println!(
        "TASK3045 before_trash_copies_of_second_message={}",
        store.count_in_folder(TRASH, ALREADY_IN_TRASH)
    );

    let request = SharedMailDeleteRequest::marked(SERVICE_ID, SIGNED_IN, FOLDER, MARKED_MESSAGE);
    let receipt = match delete_marked_mail_message(&mut store, &request) {
        Ok(receipt) => receipt,
        Err(error) => {
            println!(
                "TASK3045 direct_run_refused={} {}",
                error.code(),
                error.reason()
            );
            std::process::exit(1);
        }
    };

    let folder_copies_after = store.count_in_folder(FOLDER, MARKED_MESSAGE);
    let trash_copies_after = store.count_in_folder(TRASH, MARKED_MESSAGE);
    let second_message_after = store.count_in_folder(TRASH, ALREADY_IN_TRASH);

    println!(
        "TASK3045 direct_run_steps={}",
        receipt.step_names().join(",")
    );
    println!("TASK3045 after_folder_copies_of_marked_message={folder_copies_after}");
    println!("TASK3045 after_trash_copies_of_marked_message={trash_copies_after}");
    println!("TASK3045 after_trash_copies_of_second_message={second_message_after}");
    println!(
        "TASK3045 after_trash_message_ids=[{}]",
        store.folder_message_ids(TRASH).join(",")
    );
    println!(
        "TASK3045 move_to_trash_calls=[{}]",
        store.move_calls().join(",")
    );
    println!(
        "TASK3045 remove_from_trash_calls=[{}]",
        store.remove_calls().join(",")
    );
    println!(
        "TASK3045 whole_trash_emptied={}",
        receipt.whole_trash_emptied
    );
    println!(
        "TASK3045 other_trash_messages_before={} other_trash_messages_after={}",
        receipt.other_trash_messages_before, receipt.other_trash_messages_after
    );

    // The direct run reports a failure rather than printing a clean line it did
    // not earn.
    let mut failures = Vec::new();
    if folder_copies_after != 0 {
        failures.push(format!("{folder_copies_after} copies left in {FOLDER}"));
    }
    if trash_copies_after != 0 {
        failures.push(format!("{trash_copies_after} copies left in {TRASH}"));
    }
    if second_message_after != 1 {
        failures.push(format!(
            "the message already in trash is now at {second_message_after} copies"
        ));
    }
    if receipt.whole_trash_emptied {
        failures.push("the whole trash was emptied".to_owned());
    }
    if failures.is_empty() {
        println!("TASK3045 direct_run=ok");
    } else {
        println!("TASK3045 direct_run_failed={}", failures.join("; "));
        std::process::exit(1);
    }
}
