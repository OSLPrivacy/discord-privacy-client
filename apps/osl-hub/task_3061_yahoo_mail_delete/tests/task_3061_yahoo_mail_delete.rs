//! TASK 3061 - delete marked Yahoo messages.
//!
//! Finish line: Sent holds 3 messages matching SCRUB-YH-DEL and Trash holds 1
//! unrelated message before the run, Sent holds 2 and Trash holds 1 after.
//!
//! Every count below is read back out of the Yahoo folders after the run, not
//! remembered by the deleter.

use task_3061_yahoo_mail_delete::yahoo_mail_delete::{
    delete_marked_yahoo_mail_message, YAHOO_MAIL_SENT_FOLDER_ID, YAHOO_MAIL_TRASH_FOLDER_ID,
};
use task_3061_yahoo_mail_delete::{
    seeded_yahoo_surface, MARKED_MESSAGE, MARKER, SIGNED_IN_ADDRESS, UNRELATED_TRASH_MESSAGE,
};

#[test]
fn task_3061_sent_holds_three_marked_and_trash_one_before_and_sent_two_and_trash_one_after() {
    let mut surface = seeded_yahoo_surface();

    let sent_before = surface.subject_matches_in_folder(YAHOO_MAIL_SENT_FOLDER_ID, MARKER);
    let trash_before = surface.message_ids_in(YAHOO_MAIL_TRASH_FOLDER_ID);
    println!(
        "TASK3061 before_sent_matching_{MARKER}_count={}",
        sent_before.len()
    );
    println!(
        "TASK3061 before_sent_matching_ids=[{}]",
        sent_before.join(",")
    );
    println!("TASK3061 before_trash_count={}", trash_before.len());
    println!("TASK3061 before_trash_ids=[{}]", trash_before.join(","));

    // Before the run.
    assert_eq!(
        sent_before.len(),
        3,
        "Sent holds 3 messages matching {MARKER}"
    );
    assert_eq!(
        surface.message_ids_in(YAHOO_MAIL_SENT_FOLDER_ID).len(),
        3,
        "every message in Sent is one of the three matching {MARKER}"
    );
    assert_eq!(
        trash_before,
        vec![UNRELATED_TRASH_MESSAGE.to_owned()],
        "Trash holds 1 unrelated message"
    );

    let receipt = delete_marked_yahoo_mail_message(
        &mut surface,
        SIGNED_IN_ADDRESS,
        YAHOO_MAIL_SENT_FOLDER_ID,
        MARKED_MESSAGE,
    )
    .expect("the marked Yahoo message is deleted");

    let sent_after = surface.subject_matches_in_folder(YAHOO_MAIL_SENT_FOLDER_ID, MARKER);
    let trash_after = surface.message_ids_in(YAHOO_MAIL_TRASH_FOLDER_ID);
    println!("TASK3061 run_steps={}", receipt.step_names().join(","));
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
    println!("TASK3061 after_trash_count={}", trash_after.len());
    println!("TASK3061 after_trash_ids=[{}]", trash_after.join(","));

    // After the run.
    assert_eq!(sent_after.len(), 2, "Sent holds 2 after the run");
    assert_eq!(surface.message_ids_in(YAHOO_MAIL_SENT_FOLDER_ID).len(), 2);
    assert!(
        !sent_after.contains(&MARKED_MESSAGE.to_owned()),
        "the missing Sent message is the marked one"
    );
    assert_eq!(trash_after.len(), 1, "Trash holds 1 after the run");
    assert_eq!(
        trash_after,
        vec![UNRELATED_TRASH_MESSAGE.to_owned()],
        "the unrelated Trash message is untouched"
    );

    // The single message left Trash too, and it left because it was named, not
    // because the whole trash was emptied.
    assert_eq!(
        surface.count_in_folder(YAHOO_MAIL_TRASH_FOLDER_ID, MARKED_MESSAGE),
        0
    );
    assert_eq!(
        surface.count_in_folder(YAHOO_MAIL_SENT_FOLDER_ID, MARKED_MESSAGE),
        0
    );
    assert_eq!(
        receipt.step_names(),
        vec!["move_to_trash", "remove_one_message_from_trash"]
    );
    assert_eq!(surface.remove_calls(), [MARKED_MESSAGE.to_owned()]);
    assert!(!receipt.whole_trash_emptied);
    assert_eq!(receipt.other_trash_messages_before, 1);
    assert_eq!(receipt.other_trash_messages_after, 1);
}
