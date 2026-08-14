//! TASK 3046 - direct run: the mail deleter removes the trash copy too.
//!
//! Seeds Sent with three messages matching SCRUB-MAIL-DEL, marks one of them,
//! and puts one unrelated message in trash first. Runs the shared deleter
//! (TASK 3045) once and prints what Sent and trash hold before and after,
//! read back out of the folder store.
//!
//! Direct run:
//!   cargo run --manifest-path \
//!     apps/osl-hub/task_3046_mail_deleter_trash/Cargo.toml

#[path = "../../src/mail_owner_check.rs"]
mod mail_owner_check;
#[path = "../../src/shared_mail_deleter.rs"]
mod shared_mail_deleter;

use shared_mail_deleter::{
    delete_marked_mail_message, SharedMailDeleteRequest, SharedMailFolderStore, StoredMailMessage,
};

const SERVICE_ID: &str = "gmail";
const SIGNED_IN: &str = "owner@example.test";
const SENT: &str = "Sent";
const TRASH: &str = "Trash";
const MARKED_MESSAGE: &str = "SCRUB-MAIL-DEL-2";
const KEPT_A: &str = "SCRUB-MAIL-DEL-1";
const KEPT_B: &str = "SCRUB-MAIL-DEL-3";
const UNRELATED_TRASH_MESSAGE: &str = "trash-task-3046-unrelated";

fn main() {
    let mut store = SharedMailFolderStore::new(SERVICE_ID, TRASH)
        .with_message(StoredMailMessage::new(SENT, KEPT_A, SIGNED_IN))
        .with_message(StoredMailMessage::new(SENT, MARKED_MESSAGE, SIGNED_IN))
        .with_message(StoredMailMessage::new(SENT, KEPT_B, SIGNED_IN))
        .with_message(StoredMailMessage::new(
            TRASH,
            UNRELATED_TRASH_MESSAGE,
            SIGNED_IN,
        ));

    let sent_before = store.folder_message_ids(SENT);
    let trash_before = store.folder_message_ids(TRASH);
    println!("TASK3046 sent_before={} [{}]", sent_before.len(), sent_before.join(","));
    println!("TASK3046 trash_before={} [{}]", trash_before.len(), trash_before.join(","));

    let request = SharedMailDeleteRequest::marked(SERVICE_ID, SIGNED_IN, SENT, MARKED_MESSAGE);
    let receipt = match delete_marked_mail_message(&mut store, &request) {
        Ok(receipt) => receipt,
        Err(error) => {
            println!("TASK3046 direct_run_refused={} {}", error.code(), error.reason());
            std::process::exit(1);
        }
    };

    let sent_after = store.folder_message_ids(SENT);
    let trash_after = store.folder_message_ids(TRASH);
    println!("TASK3046 direct_run_steps={}", receipt.step_names().join(","));
    println!("TASK3046 sent_after={} [{}]", sent_after.len(), sent_after.join(","));
    println!("TASK3046 trash_after={} [{}]", trash_after.len(), trash_after.join(","));

    let mut failures = Vec::new();
    if sent_before.len() != 3 {
        failures.push(format!("sent held {} before the run, not 3", sent_before.len()));
    }
    if trash_before.len() != 1 {
        failures.push(format!("trash held {} before the run, not 1", trash_before.len()));
    }
    if sent_after.len() != 2 {
        failures.push(format!("sent holds {} after the run, not 2", sent_after.len()));
    }
    if trash_after.len() != 1 {
        failures.push(format!("trash holds {} after the run, not 1", trash_after.len()));
    }
    if sent_after.contains(&MARKED_MESSAGE.to_owned()) {
        failures.push("the marked message is still in Sent".to_owned());
    }
    if !sent_after.contains(&KEPT_A.to_owned()) || !sent_after.contains(&KEPT_B.to_owned()) {
        failures.push("an unmarked Sent message went missing".to_owned());
    }
    if trash_after != vec![UNRELATED_TRASH_MESSAGE.to_owned()] {
        failures.push("the unrelated trash message was touched".to_owned());
    }

    if failures.is_empty() {
        println!("TASK3046 direct_run=ok");
    } else {
        println!("TASK3046 direct_run_failed={}", failures.join("; "));
        std::process::exit(1);
    }
}
