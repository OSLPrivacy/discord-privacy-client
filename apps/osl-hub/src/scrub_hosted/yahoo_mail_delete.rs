//! Yahoo Mail's fill-in of the shared mail deleter (TASK 3061).
//!
//! Nothing here is a second deleter. The run is the shared one — TASK 3045's
//! [`delete_marked_mail_message`] — with the two facts only Yahoo Mail knows
//! filled in:
//!
//! * its service id, `yahoo`, and
//! * the name it gives its trash folder, `Trash`.
//!
//! Everything else is the shared deleter's: the marked check, gate 3044's owner
//! check, the move to trash, the removal of **that one message** from trash by
//! id, the read-back counts and the whole-trash guard. In particular Yahoo gets
//! no whole-trash shortcut, because [`SharedMailTrashSurface`] has no call that
//! would offer one — `remove_one_message_from_trash` names a single message id
//! and this fill-in removes exactly the rows carrying that id.
//!
//! This is a separate file from [`crate::scrub_hosted::yahoo_mail`] on purpose,
//! for the same reason gate 3044's owner check moved into
//! [`crate::mail_owner_check`]: the deleter needs the folder rows and nothing
//! that the mailbox reader's snapshot plumbing pulls in.
//! [`crate::scrub_hosted::yahoo_mail::yahoo_trash_surface_from_mailbox`] is the
//! bridge, so one seeded Yahoo mailbox feeds the reader and the deleter alike.

use crate::mail_owner_check::VisibleMailMessage;
use crate::shared_mail_deleter::{
    delete_marked_mail_message, SharedMailDeleteError, SharedMailDeleteReceipt,
    SharedMailDeleteRequest, SharedMailTrashSurface,
};

pub const YAHOO_MAIL_SERVICE_ID: &str = "yahoo";

/// What Yahoo Mail calls its trash folder. Named by the service rather than
/// guessed by the shared deleter — that is what `trash_folder_id()` is for.
pub const YAHOO_MAIL_TRASH_FOLDER_ID: &str = "Trash";

/// Yahoo's Sent folder, the folder a Scrub review of your own mail reads.
pub const YAHOO_MAIL_SENT_FOLDER_ID: &str = "Sent";

/// One row as Yahoo Mail shows it: which folder it is in, its id, its subject
/// and who sent it. The subject is carried because a Scrub run is named by what
/// it matched (`SCRUB-YH-DEL`), so a check can count the same rows the review
/// would have.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct YahooMailRow {
    pub folder_id: String,
    pub message_id: String,
    pub subject: String,
    pub sender_address: Option<String>,
}

impl YahooMailRow {
    pub fn new(
        folder_id: impl Into<String>,
        message_id: impl Into<String>,
        subject: impl Into<String>,
        sender_address: impl Into<String>,
    ) -> Self {
        Self {
            folder_id: folder_id.into(),
            message_id: message_id.into(),
            subject: subject.into(),
            sender_address: Some(sender_address.into()),
        }
    }

    /// A row whose sender Yahoo would not render. The shared deleter refuses
    /// these rather than guessing who sent them.
    pub fn with_unreadable_sender(
        folder_id: impl Into<String>,
        message_id: impl Into<String>,
        subject: impl Into<String>,
    ) -> Self {
        Self {
            folder_id: folder_id.into(),
            message_id: message_id.into(),
            subject: subject.into(),
            sender_address: None,
        }
    }
}

/// The Yahoo Mail folders the shared deleter drives.
///
/// Counts are always taken by reading the rows back out of this surface, never
/// from anything the deleter remembered.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct YahooMailTrashSurface {
    rows: Vec<YahooMailRow>,
    move_calls: Vec<String>,
    remove_calls: Vec<String>,
}

impl YahooMailTrashSurface {
    pub fn new(rows: impl IntoIterator<Item = YahooMailRow>) -> Self {
        Self {
            rows: rows.into_iter().collect(),
            move_calls: Vec::new(),
            remove_calls: Vec::new(),
        }
    }

    pub fn message_ids_in(&self, folder_id: &str) -> Vec<String> {
        self.rows
            .iter()
            .filter(|row| row.folder_id == folder_id)
            .map(|row| row.message_id.clone())
            .collect()
    }

    pub fn count_in_folder(&self, folder_id: &str, message_id: &str) -> usize {
        self.rows
            .iter()
            .filter(|row| row.folder_id == folder_id && row.message_id == message_id)
            .count()
    }

    /// The ids in `folder_id` whose subject carries `marker` — the rows a Scrub
    /// run named `marker` would have reached.
    pub fn subject_matches_in_folder(&self, folder_id: &str, marker: &str) -> Vec<String> {
        self.rows
            .iter()
            .filter(|row| row.folder_id == folder_id && row.subject.contains(marker))
            .map(|row| row.message_id.clone())
            .collect()
    }

    /// Every `move_message_to_trash` the shared deleter asked Yahoo for.
    pub fn move_calls(&self) -> &[String] {
        &self.move_calls
    }

    /// Every `remove_one_message_from_trash` the shared deleter asked Yahoo for,
    /// by message id. There is no whole-trash call to record.
    pub fn remove_calls(&self) -> &[String] {
        &self.remove_calls
    }
}

impl SharedMailTrashSurface for YahooMailTrashSurface {
    fn service_id(&self) -> &str {
        YAHOO_MAIL_SERVICE_ID
    }

    fn trash_folder_id(&self) -> &str {
        YAHOO_MAIL_TRASH_FOLDER_ID
    }

    fn message_ids_in_folder(&self, folder_id: &str) -> Result<Vec<String>, String> {
        Ok(self.message_ids_in(folder_id))
    }

    fn visible_message(
        &self,
        folder_id: &str,
        message_id: &str,
    ) -> Result<Option<VisibleMailMessage>, String> {
        Ok(self
            .rows
            .iter()
            .find(|row| row.folder_id == folder_id && row.message_id == message_id)
            .map(|row| VisibleMailMessage {
                message_id: row.message_id.clone(),
                mailbox: row.folder_id.clone(),
                sender_address: row.sender_address.clone(),
            }))
    }

    fn move_message_to_trash(&mut self, folder_id: &str, message_id: &str) -> Result<(), String> {
        self.move_calls.push(format!("{folder_id}/{message_id}"));
        let mut moved = false;
        for row in &mut self.rows {
            if row.folder_id == folder_id && row.message_id == message_id {
                row.folder_id = YAHOO_MAIL_TRASH_FOLDER_ID.to_owned();
                moved = true;
            }
        }
        if moved {
            Ok(())
        } else {
            Err("yahoo mail message is not in that folder".to_owned())
        }
    }

    /// One message, by id, out of Trash. Every other row in Trash stays exactly
    /// where it is: this is not "empty trash", and there is no call here that is.
    fn remove_one_message_from_trash(&mut self, message_id: &str) -> Result<(), String> {
        self.remove_calls.push(message_id.to_owned());
        self.rows.retain(|row| {
            !(row.folder_id == YAHOO_MAIL_TRASH_FOLDER_ID && row.message_id == message_id)
        });
        Ok(())
    }
}

/// Delete one marked Yahoo Mail message: move it to Yahoo's Trash, then remove
/// that one message from Trash. The checks — marked, and sent by the signed-in
/// address — are the shared deleter's, and all of them run before the first
/// write, so a refusal never reaches Yahoo at all.
pub fn delete_marked_yahoo_mail_message(
    surface: &mut YahooMailTrashSurface,
    signed_in_address: &str,
    folder_id: &str,
    message_id: &str,
) -> Result<SharedMailDeleteReceipt, SharedMailDeleteError> {
    delete_marked_mail_message(
        surface,
        &SharedMailDeleteRequest::marked(
            YAHOO_MAIL_SERVICE_ID,
            signed_in_address,
            folder_id,
            message_id,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_mail_deleter::SharedMailReviewDecision;

    const SIGNED_IN_ADDRESS: &str = "owner@yahoo.example.test";
    const MARKER: &str = "SCRUB-YH-DEL";
    const MARKED_MESSAGE: &str = "sent-yh-3061-002";
    const UNRELATED_TRASH_MESSAGE: &str = "trash-yh-3061-was-already-here";

    /// Sent holds three messages matching SCRUB-YH-DEL; Trash holds one
    /// unrelated message the user binned earlier and which is not this run's to
    /// destroy.
    fn seeded_yahoo_surface() -> YahooMailTrashSurface {
        YahooMailTrashSurface::new([
            YahooMailRow::new(
                YAHOO_MAIL_SENT_FOLDER_ID,
                "sent-yh-3061-001",
                "SCRUB-YH-DEL renewal receipt",
                SIGNED_IN_ADDRESS,
            ),
            YahooMailRow::new(
                YAHOO_MAIL_SENT_FOLDER_ID,
                MARKED_MESSAGE,
                "SCRUB-YH-DEL address confirmation",
                SIGNED_IN_ADDRESS,
            ),
            YahooMailRow::new(
                YAHOO_MAIL_SENT_FOLDER_ID,
                "sent-yh-3061-003",
                "SCRUB-YH-DEL travel plan",
                SIGNED_IN_ADDRESS,
            ),
            YahooMailRow::new(
                YAHOO_MAIL_TRASH_FOLDER_ID,
                UNRELATED_TRASH_MESSAGE,
                "Yahoo newsletter binned last week",
                "news@example.test",
            ),
        ])
    }

    #[test]
    fn task_3061_marked_yahoo_message_leaves_sent_and_trash_and_the_other_trash_message_stays() {
        let mut surface = seeded_yahoo_surface();

        let sent_before = surface.subject_matches_in_folder(YAHOO_MAIL_SENT_FOLDER_ID, MARKER);
        let trash_before = surface.message_ids_in(YAHOO_MAIL_TRASH_FOLDER_ID);
        println!("TASK3061 service_id={YAHOO_MAIL_SERVICE_ID}");
        println!("TASK3061 trash_folder_id={YAHOO_MAIL_TRASH_FOLDER_ID}");
        println!("TASK3061 marker={MARKER}");
        println!("TASK3061 marked_message_id={MARKED_MESSAGE}");
        println!("TASK3061 before_sent_matching_count={}", sent_before.len());
        println!(
            "TASK3061 before_sent_matching_ids={}",
            sent_before.join(",")
        );
        println!("TASK3061 before_trash_count={}", trash_before.len());
        println!("TASK3061 before_trash_ids={}", trash_before.join(","));

        assert_eq!(sent_before.len(), 3);
        assert_eq!(
            surface.message_ids_in(YAHOO_MAIL_SENT_FOLDER_ID).len(),
            3,
            "every message in Sent is one of the three matching {MARKER}"
        );
        assert_eq!(trash_before, vec![UNRELATED_TRASH_MESSAGE.to_owned()]);

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
        println!("TASK3061 after_sent_matching_count={}", sent_after.len());
        println!("TASK3061 after_sent_matching_ids={}", sent_after.join(","));
        println!("TASK3061 after_trash_count={}", trash_after.len());
        println!("TASK3061 after_trash_ids={}", trash_after.join(","));
        println!("TASK3061 move_calls={}", surface.move_calls().join(","));
        println!("TASK3061 remove_calls={}", surface.remove_calls().join(","));

        assert_eq!(sent_after.len(), 2, "Sent holds 2 after the run");
        assert_eq!(surface.message_ids_in(YAHOO_MAIL_SENT_FOLDER_ID).len(), 2);
        assert!(!sent_after.contains(&MARKED_MESSAGE.to_owned()));
        assert_eq!(
            trash_after,
            vec![UNRELATED_TRASH_MESSAGE.to_owned()],
            "Trash holds 1 after the run, the unrelated message, untouched"
        );
        assert_eq!(
            surface.count_in_folder(YAHOO_MAIL_SENT_FOLDER_ID, MARKED_MESSAGE),
            0
        );
        assert_eq!(
            surface.count_in_folder(YAHOO_MAIL_TRASH_FOLDER_ID, MARKED_MESSAGE),
            0,
            "the single message was removed from Trash too"
        );
        assert_eq!(
            receipt.step_names(),
            vec!["move_to_trash", "remove_one_message_from_trash"]
        );
        assert_eq!(receipt.trash_folder_id, YAHOO_MAIL_TRASH_FOLDER_ID);
        assert!(!receipt.whole_trash_emptied);
        assert_eq!(receipt.other_trash_messages_before, 1);
        assert_eq!(receipt.other_trash_messages_after, 1);
        assert_eq!(surface.remove_calls(), [MARKED_MESSAGE.to_owned()]);
    }

    #[test]
    fn task_3061_a_yahoo_message_the_account_did_not_send_is_refused_before_anything_moves() {
        let mut surface = seeded_yahoo_surface();

        let refusal = delete_marked_yahoo_mail_message(
            &mut surface,
            "someone-else@yahoo.example.test",
            YAHOO_MAIL_SENT_FOLDER_ID,
            MARKED_MESSAGE,
        )
        .expect_err("a message the signed-in account did not send is refused");

        println!("TASK3061 refusal_not_yours_code={}", refusal.code());
        println!("TASK3061 refusal_not_yours_reason={}", refusal.reason());
        assert_eq!(refusal.code(), "not_yours");
        assert_eq!(surface.message_ids_in(YAHOO_MAIL_SENT_FOLDER_ID).len(), 3);
        assert_eq!(
            surface.message_ids_in(YAHOO_MAIL_TRASH_FOLDER_ID),
            vec![UNRELATED_TRASH_MESSAGE.to_owned()]
        );
        assert!(surface.move_calls().is_empty());
        assert!(surface.remove_calls().is_empty());
    }

    #[test]
    fn task_3061_an_unmarked_yahoo_message_is_refused_before_anything_moves() {
        let mut surface = seeded_yahoo_surface();
        let mut request = SharedMailDeleteRequest::marked(
            YAHOO_MAIL_SERVICE_ID,
            SIGNED_IN_ADDRESS,
            YAHOO_MAIL_SENT_FOLDER_ID,
            "sent-yh-3061-001",
        );
        request.decision = SharedMailReviewDecision::Keep;

        let refusal = delete_marked_mail_message(&mut surface, &request)
            .expect_err("a message the review did not mark is refused");

        println!("TASK3061 refusal_not_marked_code={}", refusal.code());
        assert_eq!(refusal.code(), "not_marked");
        assert_eq!(surface.message_ids_in(YAHOO_MAIL_SENT_FOLDER_ID).len(), 3);
        assert!(surface.move_calls().is_empty());
        assert!(surface.remove_calls().is_empty());
    }

    #[test]
    fn task_3061_a_yahoo_row_with_an_unreadable_sender_is_refused_rather_than_guessed() {
        let mut surface = YahooMailTrashSurface::new([
            YahooMailRow::with_unreadable_sender(
                YAHOO_MAIL_SENT_FOLDER_ID,
                "sent-yh-3061-004",
                "SCRUB-YH-DEL sender Yahoo would not render",
            ),
            YahooMailRow::new(
                YAHOO_MAIL_TRASH_FOLDER_ID,
                UNRELATED_TRASH_MESSAGE,
                "Yahoo newsletter binned last week",
                "news@example.test",
            ),
        ]);

        let refusal = delete_marked_yahoo_mail_message(
            &mut surface,
            SIGNED_IN_ADDRESS,
            YAHOO_MAIL_SENT_FOLDER_ID,
            "sent-yh-3061-004",
        )
        .expect_err("an unreadable sender is refused");

        println!("TASK3061 refusal_owner_unknown_code={}", refusal.code());
        assert_eq!(refusal.code(), "owner_unknown");
        assert!(surface.move_calls().is_empty());
        assert!(surface.remove_calls().is_empty());
    }
}
