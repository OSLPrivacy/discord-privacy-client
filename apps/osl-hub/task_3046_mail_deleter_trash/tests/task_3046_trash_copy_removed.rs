//! TASK 3046 - test the mail deleter removes the trash copy too.
//!
//! Gates TASK 3045 (`shared_mail_deleter.rs`, moved into this lane's
//! `apps/osl-hub/src/` for this task). Seeds Sent with three messages matching
//! SCRUB-MAIL-DEL, marks one of them for deletion, and puts one unrelated
//! message in trash first.
//!
//! done when: Sent holds 3 and trash holds 1 before the run, Sent holds 2 and
//! trash holds 1 after, the missing Sent message is the marked one, and the
//! unrelated trash message is untouched.

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

fn seeded_store() -> SharedMailFolderStore {
    SharedMailFolderStore::new(SERVICE_ID, TRASH)
        .with_message(StoredMailMessage::new(SENT, KEPT_A, SIGNED_IN))
        .with_message(StoredMailMessage::new(SENT, MARKED_MESSAGE, SIGNED_IN))
        .with_message(StoredMailMessage::new(SENT, KEPT_B, SIGNED_IN))
        .with_message(StoredMailMessage::new(
            TRASH,
            UNRELATED_TRASH_MESSAGE,
            SIGNED_IN,
        ))
}

#[test]
fn task_3046_deleting_one_marked_sent_message_removes_the_trash_copy_too_and_leaves_the_rest() {
    let mut store = seeded_store();

    let sent_before = store.folder_message_ids(SENT);
    let trash_before = store.folder_message_ids(TRASH);
    println!(
        "TASK3046 sent_before_count={} ids=[{}]",
        sent_before.len(),
        sent_before.join(",")
    );
    println!(
        "TASK3046 trash_before_count={} ids=[{}]",
        trash_before.len(),
        trash_before.join(",")
    );
    assert_eq!(sent_before.len(), 3, "Sent holds 3 messages before the run");
    assert_eq!(trash_before.len(), 1, "trash holds 1 message before the run");

    let request = SharedMailDeleteRequest::marked(SERVICE_ID, SIGNED_IN, SENT, MARKED_MESSAGE);
    let receipt =
        delete_marked_mail_message(&mut store, &request).expect("the marked run succeeds");
    println!(
        "TASK3046 direct_run_steps={}",
        receipt.step_names().join(",")
    );

    let sent_after = store.folder_message_ids(SENT);
    let trash_after = store.folder_message_ids(TRASH);
    println!(
        "TASK3046 sent_after_count={} ids=[{}]",
        sent_after.len(),
        sent_after.join(",")
    );
    println!(
        "TASK3046 trash_after_count={} ids=[{}]",
        trash_after.len(),
        trash_after.join(",")
    );

    assert_eq!(sent_after.len(), 2, "Sent holds 2 messages after the run");
    assert_eq!(trash_after.len(), 1, "trash holds 1 message after the run");

    assert!(
        !sent_after.contains(&MARKED_MESSAGE.to_owned()),
        "the missing Sent message is the marked one"
    );
    assert!(
        sent_after.contains(&KEPT_A.to_owned()) && sent_after.contains(&KEPT_B.to_owned()),
        "the two unmarked Sent messages are untouched"
    );

    assert_eq!(
        trash_after,
        vec![UNRELATED_TRASH_MESSAGE.to_owned()],
        "the unrelated trash message is untouched, and no copy of the marked message is left in trash"
    );

    // The marked message is gone entirely: not in Sent, not left behind in trash.
    assert_eq!(store.count_in_folder(SENT, MARKED_MESSAGE), 0);
    assert_eq!(store.count_in_folder(TRASH, MARKED_MESSAGE), 0);
    assert_eq!(store.count_in_folder(TRASH, UNRELATED_TRASH_MESSAGE), 1);
    assert!(!receipt.whole_trash_emptied);
}

/// TASK 3046b - prove the 3046 check can fail.
///
/// A throwaway copy of the deleter that stops right after moving the marked
/// message to trash (never calls `remove_one_message_from_trash`). Run against
/// the exact 3046 scenario, trash ends up holding 2 messages instead of 1, so
/// the 3046 assertion `trash_after.len() == 1` goes red. `shared_mail_deleter.rs`
/// itself is never touched by this test — the throwaway logic lives entirely
/// here, so the working copy is unchanged afterwards.
fn throwaway_broken_delete_stops_after_move_to_trash(
    surface: &mut SharedMailFolderStore,
    request: &SharedMailDeleteRequest,
) {
    use shared_mail_deleter::SharedMailTrashSurface;
    surface
        .move_message_to_trash(&request.folder_id, &request.message_id)
        .expect("move to trash succeeds");
    // Deliberately missing: surface.remove_one_message_from_trash(...)
}

#[test]
fn task_3046b_a_deleter_that_stops_after_move_to_trash_makes_the_3046_check_go_red() {
    // Red: the broken throwaway copy leaves the marked message sitting in
    // trash, so trash holds 2 instead of the 1 the 3046 check requires.
    let mut broken_store = seeded_store();
    let request = SharedMailDeleteRequest::marked(SERVICE_ID, SIGNED_IN, SENT, MARKED_MESSAGE);
    throwaway_broken_delete_stops_after_move_to_trash(&mut broken_store, &request);

    let trash_after_broken = broken_store.folder_message_ids(TRASH);
    println!(
        "TASK3046b broken_trash_after_count={} ids=[{}]",
        trash_after_broken.len(),
        trash_after_broken.join(",")
    );
    assert_eq!(
        trash_after_broken.len(),
        2,
        "the 3046 check (trash holds 1 after the run) goes red: trash holds 2"
    );
    assert_ne!(
        trash_after_broken.len(),
        1,
        "confirms the real 3046 assertion would fail against this broken copy"
    );

    // Green: the real, unmodified `delete_marked_mail_message` from
    // `shared_mail_deleter.rs` still passes the same check on a fresh store —
    // the working copy was never touched by the throwaway break above.
    let mut real_store = seeded_store();
    delete_marked_mail_message(&mut real_store, &request).expect("the real run succeeds");
    let trash_after_real = real_store.folder_message_ids(TRASH);
    println!(
        "TASK3046b real_trash_after_count={} ids=[{}]",
        trash_after_real.len(),
        trash_after_real.join(",")
    );
    assert_eq!(
        trash_after_real.len(),
        1,
        "the working copy is unchanged: the real deleter still leaves trash at 1"
    );
    assert_eq!(trash_after_real, vec![UNRELATED_TRASH_MESSAGE.to_owned()]);
}
