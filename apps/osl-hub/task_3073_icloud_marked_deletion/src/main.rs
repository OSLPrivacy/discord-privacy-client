//! TASK 3073 - the direct run of the shared mail deleter, filled in for iCloud.
//!
//! Seeds one iCloud mailbox: Sent holding three messages matching
//! `SCRUB-IC-DEL`, and Trash holding one unrelated message put there before the
//! run. Marks the middle Sent message, runs the shared deleter once, and prints
//! what Sent and Trash hold afterwards - read back through gate 3071's iCloud
//! reader, not remembered by the deleter.
//!
//! The modules below are the real files under `apps/osl-hub/src/`, compiled by
//! path. Nothing here is a copy of them.
//!
//! Direct run:
//!   cargo run --manifest-path \
//!     apps/osl-hub/task_3073_icloud_marked_deletion/Cargo.toml
//!
//! The checks (they live beside the deleter, in `icloud_mail_deleter.rs`):
//!   cargo test --manifest-path \
//!     apps/osl-hub/task_3073_icloud_marked_deletion/Cargo.toml task_3073 \
//!     -- --test-threads=1 --nocapture

#[path = "../../src/icloud_mail_deleter.rs"]
mod icloud_mail_deleter;
#[path = "../../src/icloud_mailbox_reader.rs"]
mod icloud_mailbox_reader;
#[path = "../../src/mail_owner_check.rs"]
mod mail_owner_check;
#[path = "../../src/row_who_wrote_it.rs"]
mod row_who_wrote_it;
#[path = "../../src/shared_mail_deleter.rs"]
mod shared_mail_deleter;
#[path = "../../src/shared_mail_reader_types.rs"]
mod shared_mail_reader_types;

// Gate 3071's test file, compiled here unmodified, so the iCloud reader that
// moved into `icloud_mailbox_reader.rs` is still checked by the test that was
// written against it. The test says `use osl_privacy_hub::service_connections::…`;
// this stands in for that path with the very modules the lib now re-exports
// from `service_connections`.
#[cfg(test)]
extern crate self as osl_privacy_hub;
#[cfg(test)]
pub mod service_connections {
    pub use crate::icloud_mailbox_reader::{
        open_icloud_shared_mailbox_message, read_icloud_shared_mailbox_folders,
        read_icloud_shared_mailbox_messages, ICLOUD_SERVICE_ID,
    };
    pub use crate::shared_mail_reader_types::{
        OpenedSharedMailMessage, SharedMailLabel, SharedMailMessageRecord,
        SharedMailMessageSummary, SharedMailboxReaderError, SharedMailboxSnapshot,
    };
}
#[cfg(test)]
#[path = "../../tests/task_3071_icloud_mailbox_reader.rs"]
mod task_3071_icloud_mailbox_reader;

use icloud_mail_deleter::{
    delete_marked_icloud_message, icloud_delete_request, icloud_messages_matching_subject,
    ICLOUD_TRASH_FOLDER_ID,
};
use icloud_mailbox_reader::{
    read_icloud_shared_mailbox_folders, read_icloud_shared_mailbox_messages, ICLOUD_SERVICE_ID,
};
use shared_mail_reader_types::{SharedMailLabel, SharedMailMessageRecord, SharedMailboxSnapshot};

const SIGNED_IN: &str = "scrub-owner@icloud.test";
const SENT: &str = "Sent";
const TRASH: &str = "Trash";
const MARKED_SUBJECT: &str = "SCRUB-IC-DEL";
const MARKED_MESSAGE: &str = "sent-scrub-ic-del-two";
const UNRELATED_IN_TRASH: &str = "trash-icloud-holiday-photos";

fn seeded_icloud_mailbox() -> SharedMailboxSnapshot {
    SharedMailboxSnapshot::new(
        SIGNED_IN,
        [
            SharedMailLabel::new("Inbox", "Inbox"),
            SharedMailLabel::new(SENT, "Sent"),
            SharedMailLabel::new("Archive", "Archive"),
            SharedMailLabel::new(TRASH, "Trash"),
        ],
        [
            SharedMailMessageRecord::new(
                SENT,
                "sent-scrub-ic-del-one",
                MARKED_SUBJECT,
                1_786_276_800,
                SIGNED_IN,
                "First iCloud message matching SCRUB-IC-DEL.",
            ),
            SharedMailMessageRecord::new(
                SENT,
                MARKED_MESSAGE,
                MARKED_SUBJECT,
                1_786_280_400,
                SIGNED_IN,
                "Second iCloud message matching SCRUB-IC-DEL; this is the marked one.",
            ),
            SharedMailMessageRecord::new(
                SENT,
                "sent-scrub-ic-del-three",
                MARKED_SUBJECT,
                1_786_284_000,
                SIGNED_IN,
                "Third iCloud message matching SCRUB-IC-DEL.",
            ),
            SharedMailMessageRecord::new(
                TRASH,
                UNRELATED_IN_TRASH,
                "Holiday photos",
                1_786_200_000,
                SIGNED_IN,
                "An unrelated iCloud message the user put in Trash before the run.",
            ),
        ],
    )
}

fn sent_matching(mailbox: &SharedMailboxSnapshot) -> Vec<String> {
    icloud_messages_matching_subject(mailbox, SENT, MARKED_SUBJECT).expect("read iCloud Sent")
}

fn trash_ids(mailbox: &SharedMailboxSnapshot) -> Vec<String> {
    read_icloud_shared_mailbox_messages(mailbox, TRASH)
        .expect("read iCloud Trash")
        .into_iter()
        .map(|message| message.message_id)
        .collect()
}

fn main() {
    let mut mailbox = seeded_icloud_mailbox();

    let folders = read_icloud_shared_mailbox_folders(&mailbox).expect("iCloud folders");
    let sent_before = sent_matching(&mailbox);
    let trash_before = trash_ids(&mailbox);

    println!("TASK3073 service_id={ICLOUD_SERVICE_ID}");
    println!("TASK3073 folder_id={SENT}");
    println!("TASK3073 trash_folder_id={ICLOUD_TRASH_FOLDER_ID}");
    println!("TASK3073 folder_count={}", folders.len());
    println!("TASK3073 marked_subject={MARKED_SUBJECT}");
    println!("TASK3073 marked_message_id={MARKED_MESSAGE}");
    println!("TASK3073 unrelated_message_already_in_trash_id={UNRELATED_IN_TRASH}");
    println!("TASK3073 before_sent_matching_count={}", sent_before.len());
    println!(
        "TASK3073 before_sent_matching_ids=[{}]",
        sent_before.join(",")
    );
    println!("TASK3073 before_trash_count={}", trash_before.len());
    println!("TASK3073 before_trash_ids=[{}]", trash_before.join(","));

    let request = icloud_delete_request(SIGNED_IN, SENT, MARKED_MESSAGE);
    let receipt = match delete_marked_icloud_message(&mut mailbox, &request) {
        Ok(receipt) => receipt,
        Err(error) => {
            println!(
                "TASK3073 direct_run_refused={} {}",
                error.code(),
                error.reason()
            );
            std::process::exit(1);
        }
    };

    let sent_after = sent_matching(&mailbox);
    let trash_after = trash_ids(&mailbox);

    println!(
        "TASK3073 direct_run_steps={}",
        receipt.step_names().join(",")
    );
    println!("TASK3073 after_sent_matching_count={}", sent_after.len());
    println!(
        "TASK3073 after_sent_matching_ids=[{}]",
        sent_after.join(",")
    );
    println!("TASK3073 after_trash_count={}", trash_after.len());
    println!("TASK3073 after_trash_ids=[{}]", trash_after.join(","));
    println!(
        "TASK3073 folder_copies_after={} trash_copies_after={}",
        receipt.folder_copies_after, receipt.trash_copies_after
    );
    println!(
        "TASK3073 whole_trash_emptied={}",
        receipt.whole_trash_emptied
    );
    println!(
        "TASK3073 other_trash_messages_before={} other_trash_messages_after={}",
        receipt.other_trash_messages_before, receipt.other_trash_messages_after
    );

    // The direct run reports a failure rather than printing a clean line it did
    // not earn.
    let mut failures = Vec::new();
    if sent_before.len() != 3 {
        failures.push(format!(
            "{SENT} held {} messages matching {MARKED_SUBJECT} before the run, not 3",
            sent_before.len()
        ));
    }
    if trash_before != vec![UNRELATED_IN_TRASH.to_owned()] {
        failures.push(format!(
            "{TRASH} held [{}] before the run, not just {UNRELATED_IN_TRASH}",
            trash_before.join(",")
        ));
    }
    if sent_after.len() != 2 {
        failures.push(format!(
            "{SENT} holds {} messages matching {MARKED_SUBJECT} after the run, not 2",
            sent_after.len()
        ));
    }
    if sent_after.contains(&MARKED_MESSAGE.to_owned()) {
        failures.push(format!(
            "the marked message {MARKED_MESSAGE} is still in {SENT}"
        ));
    }
    if trash_after != vec![UNRELATED_IN_TRASH.to_owned()] {
        failures.push(format!(
            "{TRASH} holds [{}] after the run, not just {UNRELATED_IN_TRASH}",
            trash_after.join(",")
        ));
    }
    if receipt.trash_copies_after != 0 {
        failures.push(format!(
            "{} copies of the marked message are still in {TRASH}",
            receipt.trash_copies_after
        ));
    }
    if receipt.whole_trash_emptied {
        failures.push("the whole trash was emptied".to_owned());
    }
    if failures.is_empty() {
        println!("TASK3073 direct_run=ok");
    } else {
        println!("TASK3073 direct_run_failed={}", failures.join("; "));
        std::process::exit(1);
    }
}
