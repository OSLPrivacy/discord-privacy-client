//! TASK 3049 - the direct run of the Gmail fill-in of the shared mail deleter.
//!
//! Seeds one Gmail account: Sent holds three messages marked SCRUB-GM-DEL that
//! the signed-in account sent, Bin holds one unrelated message that was already
//! there, and Inbox holds one SCRUB-GM-DEL message somebody else sent. Then it
//!
//!   1. counts Sent and Bin by reading the mailbox,
//!   2. deletes one marked message - relabel into Bin, then remove that one
//!      message from Bin by id,
//!   3. counts Sent and Bin again, and
//!   4. asks for the message the account did not send and prints the refusal.
//!
//! The modules below are the real files under `apps/osl-hub/src/`, compiled by
//! path. Nothing here is a copy of them.
//!
//! Direct run:
//!   cargo run --manifest-path \
//!     apps/osl-hub/task_3049_gmail_mail_deleter/Cargo.toml
//!
//! The checks (they live beside the fill-in, in `gmail_mail_deleter.rs`):
//!   cargo test --manifest-path \
//!     apps/osl-hub/task_3049_gmail_mail_deleter/Cargo.toml task_3049 \
//!     -- --test-threads=1 --nocapture

// The shared files carry more than this one command uses - the TASK 3045
// provider-neutral folder store, the whole who-wrote-it vocabulary - so the
// unused halves are not warnings here.
#![allow(dead_code)]

#[path = "../../src/gmail_mail_deleter.rs"]
mod gmail_mail_deleter;
#[path = "../../src/mail_owner_check.rs"]
mod mail_owner_check;
#[path = "../../src/row_who_wrote_it.rs"]
mod row_who_wrote_it;
#[path = "../../src/shared_mail_deleter.rs"]
mod shared_mail_deleter;
#[path = "../../src/shared_mail_snapshot.rs"]
mod shared_mail_snapshot;

use gmail_mail_deleter::{
    delete_marked_gmail_message, gmail_label_message_ids, gmail_label_messages_matching,
    GMAIL_MAIL_SERVICE_ID,
};
use shared_mail_deleter::SharedMailDeleteRequest;
use shared_mail_snapshot::{SharedMailLabel, SharedMailMessageRecord, SharedMailboxSnapshot};

const SIGNED_IN: &str = "scrub-owner@gmail.test";
const SENT: &str = "Sent";
const BIN: &str = "[Gmail]/Bin";
const MARK: &str = "SCRUB-GM-DEL";
const MARKED_MESSAGE: &str = "sent-scrub-gm-del-one";
const BIN_UNRELATED: &str = "bin-kept-from-last-week";
const NOT_MINE: &str = "inbox-scrub-gm-del-not-mine";

fn seeded_gmail_mailbox() -> SharedMailboxSnapshot {
    SharedMailboxSnapshot::new(
        SIGNED_IN,
        [
            SharedMailLabel::new("Inbox", "Inbox"),
            SharedMailLabel::new(SENT, "Sent"),
            SharedMailLabel::new("[Gmail]/All Mail", "All Mail"),
            SharedMailLabel::new(BIN, "Bin"),
        ],
        [
            SharedMailMessageRecord::new(
                SENT,
                MARKED_MESSAGE,
                "SCRUB-GM-DEL one",
                1_786_190_400,
                SIGNED_IN,
                "First sent message marked SCRUB-GM-DEL.",
            ),
            SharedMailMessageRecord::new(
                SENT,
                "sent-scrub-gm-del-two",
                "SCRUB-GM-DEL two",
                1_786_194_000,
                SIGNED_IN,
                "Second sent message marked SCRUB-GM-DEL.",
            ),
            SharedMailMessageRecord::new(
                SENT,
                "sent-scrub-gm-del-three",
                "SCRUB-GM-DEL three",
                1_786_197_600,
                SIGNED_IN,
                "Third sent message marked SCRUB-GM-DEL.",
            ),
            SharedMailMessageRecord::new(
                BIN,
                BIN_UNRELATED,
                "Receipt from the shop",
                1_785_585_600,
                "shop@gmail.test",
                "The unrelated message already sitting in Bin before the run.",
            ),
            SharedMailMessageRecord::new(
                "Inbox",
                NOT_MINE,
                "SCRUB-GM-DEL sent by somebody else",
                1_786_201_200,
                "friend-one@gmail.test",
                "A message the signed-in account did not send.",
            ),
        ],
    )
}

fn main() {
    let mut mailbox = seeded_gmail_mailbox();
    let mut failures = Vec::new();

    println!("TASK3049 service_id={GMAIL_MAIL_SERVICE_ID}");
    println!("TASK3049 signed_in_address={SIGNED_IN}");
    println!("TASK3049 mark={MARK}");
    println!("TASK3049 sent_label={SENT}");
    println!("TASK3049 bin_label={BIN}");

    let sent_before = gmail_label_messages_matching(&mailbox, SENT, MARK);
    let bin_before = gmail_label_message_ids(&mailbox, BIN);
    println!("TASK3049 before_sent_matching_count={}", sent_before.len());
    println!(
        "TASK3049 before_sent_matching_ids=[{}]",
        sent_before.join(",")
    );
    println!("TASK3049 before_bin_count={}", bin_before.len());
    println!("TASK3049 before_bin_ids=[{}]", bin_before.join(","));
    if sent_before.len() != 3 {
        failures.push(format!(
            "Sent holds {} messages matching {MARK} before the run, not 3",
            sent_before.len()
        ));
    }
    if bin_before != vec![BIN_UNRELATED.to_owned()] {
        failures.push(format!(
            "Bin holds [{}] before the run, not the 1 unrelated message",
            bin_before.join(",")
        ));
    }

    let request =
        SharedMailDeleteRequest::marked(GMAIL_MAIL_SERVICE_ID, SIGNED_IN, SENT, MARKED_MESSAGE);
    println!("TASK3049 deleting_message_id={MARKED_MESSAGE}");
    let receipt = match delete_marked_gmail_message(&mut mailbox, &request) {
        Ok(receipt) => receipt,
        Err(error) => {
            println!(
                "TASK3049 direct_run_refused={} {}",
                error.code(),
                error.reason()
            );
            std::process::exit(1);
        }
    };

    let sent_after = gmail_label_messages_matching(&mailbox, SENT, MARK);
    let bin_after = gmail_label_message_ids(&mailbox, BIN);
    let copies_anywhere = mailbox
        .messages
        .iter()
        .filter(|message| message.message_id == MARKED_MESSAGE)
        .count();

    println!("TASK3049 bin_label_id_used={}", receipt.bin_label_id);
    println!(
        "TASK3049 run_steps={}",
        receipt.receipt.step_names().join(",")
    );
    println!(
        "TASK3049 relabel_to_bin_calls=[{}]",
        receipt.relabel_to_bin_calls.join(",")
    );
    println!(
        "TASK3049 remove_from_bin_calls=[{}]",
        receipt.remove_from_bin_calls.join(",")
    );
    println!("TASK3049 after_sent_matching_count={}", sent_after.len());
    println!(
        "TASK3049 after_sent_matching_ids=[{}]",
        sent_after.join(",")
    );
    println!("TASK3049 after_bin_count={}", bin_after.len());
    println!("TASK3049 after_bin_ids=[{}]", bin_after.join(","));
    println!("TASK3049 after_copies_of_deleted_message_anywhere={copies_anywhere}");
    println!(
        "TASK3049 whole_trash_emptied={}",
        receipt.receipt.whole_trash_emptied
    );
    println!(
        "TASK3049 other_bin_messages_before={} other_bin_messages_after={}",
        receipt.receipt.other_trash_messages_before, receipt.receipt.other_trash_messages_after
    );

    if sent_after.len() != 2 {
        failures.push(format!(
            "Sent holds {} messages matching {MARK} after the run, not 2",
            sent_after.len()
        ));
    }
    if bin_after != vec![BIN_UNRELATED.to_owned()] {
        failures.push(format!(
            "Bin holds [{}] after the run, not the 1 unrelated message",
            bin_after.join(",")
        ));
    }
    if copies_anywhere != 0 {
        failures.push(format!(
            "{copies_anywhere} copies of the deleted message are still in the mailbox"
        ));
    }
    if receipt.receipt.whole_trash_emptied {
        failures.push("the whole bin was emptied".to_owned());
    }
    if receipt.remove_from_bin_calls != vec![MARKED_MESSAGE.to_owned()] {
        failures.push(format!(
            "removal from Bin was asked for as [{}]",
            receipt.remove_from_bin_calls.join(",")
        ));
    }

    // A message the signed-in account did not send, asked for on a mailbox that
    // is otherwise identical.
    let mut not_mine_mailbox = seeded_gmail_mailbox();
    let before_refusal = not_mine_mailbox.clone();
    let not_mine_request =
        SharedMailDeleteRequest::marked(GMAIL_MAIL_SERVICE_ID, SIGNED_IN, "Inbox", NOT_MINE);
    println!("TASK3049 not_yours_message_id={NOT_MINE}");
    println!("TASK3049 not_yours_message_sender=friend-one@gmail.test");
    match delete_marked_gmail_message(&mut not_mine_mailbox, &not_mine_request) {
        Ok(_) => {
            println!("TASK3049 not_yours_run=accepted");
            failures.push("a message the account did not send was accepted".to_owned());
        }
        Err(error) => {
            println!("TASK3049 not_yours_code={}", error.code());
            println!("TASK3049 not_yours_reason={}", error.reason());
            println!(
                "TASK3049 not_yours_mailbox_unchanged={}",
                not_mine_mailbox == before_refusal
            );
            if error.code() != "not_yours" {
                failures.push(format!(
                    "the message the account did not send was refused {}, not not_yours",
                    error.code()
                ));
            }
            if not_mine_mailbox != before_refusal {
                failures.push("the not-yours refusal still changed the mailbox".to_owned());
            }
        }
    }

    // The direct run reports a failure rather than printing a clean line it did
    // not earn.
    if failures.is_empty() {
        println!("TASK3049 direct_run=ok");
    } else {
        println!("TASK3049 direct_run_failed={}", failures.join("; "));
        std::process::exit(1);
    }
}
