//! TASK 3061 direct command.
//!
//! Prints Sent and Trash before the run, runs the shared mail deleter's Yahoo
//! fill-in on the one marked message, and prints Sent and Trash after. Exit
//! status is 0 when the run is accepted and 1 when it is refused, so a caller
//! does not have to read the text to tell them apart.

use task_3061_yahoo_mail_delete::yahoo_mail_delete::{
    delete_marked_yahoo_mail_message, YAHOO_MAIL_SENT_FOLDER_ID, YAHOO_MAIL_SERVICE_ID,
    YAHOO_MAIL_TRASH_FOLDER_ID,
};
use task_3061_yahoo_mail_delete::{
    seeded_yahoo_surface, MARKED_MESSAGE, MARKER, SIGNED_IN_ADDRESS, UNRELATED_TRASH_MESSAGE,
};

fn main() -> std::process::ExitCode {
    let mut surface = seeded_yahoo_surface();

    let sent_before = surface.subject_matches_in_folder(YAHOO_MAIL_SENT_FOLDER_ID, MARKER);
    let trash_before = surface.message_ids_in(YAHOO_MAIL_TRASH_FOLDER_ID);
    println!("TASK3061 service_id={YAHOO_MAIL_SERVICE_ID}");
    println!("TASK3061 signed_in_address={SIGNED_IN_ADDRESS}");
    println!("TASK3061 sent_folder_id={YAHOO_MAIL_SENT_FOLDER_ID}");
    println!("TASK3061 trash_folder_id={YAHOO_MAIL_TRASH_FOLDER_ID}");
    println!("TASK3061 marker={MARKER}");
    println!("TASK3061 marked_message_id={MARKED_MESSAGE}");
    println!("TASK3061 unrelated_trash_message_id={UNRELATED_TRASH_MESSAGE}");
    println!(
        "TASK3061 before_sent_matching_{MARKER}_count={}",
        sent_before.len()
    );
    println!(
        "TASK3061 before_sent_matching_ids=[{}]",
        sent_before.join(",")
    );
    println!(
        "TASK3061 before_sent_total_count={}",
        surface.message_ids_in(YAHOO_MAIL_SENT_FOLDER_ID).len()
    );
    println!("TASK3061 before_trash_count={}", trash_before.len());
    println!("TASK3061 before_trash_ids=[{}]", trash_before.join(","));

    let receipt = match delete_marked_yahoo_mail_message(
        &mut surface,
        SIGNED_IN_ADDRESS,
        YAHOO_MAIL_SENT_FOLDER_ID,
        MARKED_MESSAGE,
    ) {
        Ok(receipt) => receipt,
        Err(refusal) => {
            println!("TASK3061 direct_run=refused code={}", refusal.code());
            println!("TASK3061 direct_run_reason={}", refusal.reason());
            return std::process::ExitCode::from(1);
        }
    };

    let sent_after = surface.subject_matches_in_folder(YAHOO_MAIL_SENT_FOLDER_ID, MARKER);
    let trash_after = surface.message_ids_in(YAHOO_MAIL_TRASH_FOLDER_ID);
    println!(
        "TASK3061 direct_run_steps={}",
        receipt.step_names().join(",")
    );
    println!("TASK3061 move_calls=[{}]", surface.move_calls().join(","));
    println!(
        "TASK3061 remove_from_trash_calls=[{}]",
        surface.remove_calls().join(",")
    );
    println!(
        "TASK3061 after_sent_matching_{MARKER}_count={}",
        sent_after.len()
    );
    println!(
        "TASK3061 after_sent_matching_ids=[{}]",
        sent_after.join(",")
    );
    println!(
        "TASK3061 after_sent_total_count={}",
        surface.message_ids_in(YAHOO_MAIL_SENT_FOLDER_ID).len()
    );
    println!("TASK3061 after_trash_count={}", trash_after.len());
    println!("TASK3061 after_trash_ids=[{}]", trash_after.join(","));
    println!(
        "TASK3061 after_sent_copies_of_marked_message={}",
        surface.count_in_folder(YAHOO_MAIL_SENT_FOLDER_ID, MARKED_MESSAGE)
    );
    println!(
        "TASK3061 after_trash_copies_of_marked_message={}",
        surface.count_in_folder(YAHOO_MAIL_TRASH_FOLDER_ID, MARKED_MESSAGE)
    );
    println!(
        "TASK3061 other_trash_messages_before={} other_trash_messages_after={}",
        receipt.other_trash_messages_before, receipt.other_trash_messages_after
    );
    println!(
        "TASK3061 whole_trash_emptied={}",
        receipt.whole_trash_emptied
    );
    println!("TASK3061 direct_run=ok");
    std::process::ExitCode::from(0)
}
