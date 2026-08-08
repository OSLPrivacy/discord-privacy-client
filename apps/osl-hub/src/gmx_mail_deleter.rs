//! GMX Mail's fill-in of the shared mail deleter (TASK 3067).
//!
//! TASK 3045 owns the order of operations for every mail service: move the
//! marked message to trash, then remove **that one message** from trash, by id.
//! Nothing in this file repeats that order, and nothing here decides whether a
//! message may be deleted — [`crate::shared_mail_deleter`] does both. What GMX
//! fills in is only the four service-shaped answers the shared deleter asks
//! for:
//!
//! * the service id GMX answers to (`gmx`),
//! * which of GMX's own folders is its trash — read out of GMX's folder list
//!   from TASK 3065 (`Inbox`, `Sent`, `Drafts`, `Trash`), not guessed here,
//! * what is in a GMX folder, and who a GMX message came from, and
//! * the two GMX actions: move one message to trash, and remove one message
//!   from trash by id.
//!
//! There is deliberately no "empty trash" here, because
//! [`SharedMailTrashSurface`] has no such call to fill in. A GMX message the
//! user put in trash themselves is not this command's to destroy, and the
//! shared deleter re-reads trash afterwards and refuses the run if any of it
//! went missing.

use crate::mail_owner_check::VisibleMailMessage;
use crate::shared_mail_deleter::{
    delete_marked_mail_message, SharedMailDeleteError, SharedMailDeleteReceipt,
    SharedMailDeleteRequest, SharedMailTrashSurface,
};

/// The service id GMX answers to. A delete request naming any other service is
/// refused by the shared deleter with `wrong_service`.
pub const GMX_SERVICE_ID: &str = "gmx";

/// GMX's own folder list, as TASK 3065's mailbox reader returns it.
pub const GMX_FOLDERS: &[&str] = &["Inbox", "Sent", "Drafts", "Trash"];

/// The folder GMX itself calls its trash. It is looked up in [`GMX_FOLDERS`]
/// rather than written out again, so a GMX that renamed the folder would take
/// the name with it instead of leaving this constant lying.
pub const GMX_TRASH_FOLDER: &str = "Trash";

/// The folder the Scrub review reads GMX's own sent mail from.
pub const GMX_SENT_FOLDER: &str = "Sent";

/// One GMX message as this fill-in holds it. `sender_address` is an `Option`
/// because GMX can show a row whose sender cannot be read, and TASK 3044's
/// owner check refuses that rather than guessing.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GmxMailMessage {
    pub folder: String,
    pub message_id: String,
    pub subject: String,
    pub sender_address: Option<String>,
}

impl GmxMailMessage {
    pub fn new(
        folder: impl Into<String>,
        message_id: impl Into<String>,
        subject: impl Into<String>,
        sender_address: impl Into<String>,
    ) -> Self {
        Self {
            folder: folder.into(),
            message_id: message_id.into(),
            subject: subject.into(),
            sender_address: Some(sender_address.into()),
        }
    }
}

/// GMX's folders as the deleter sees them. Counts are always answered by
/// walking this list, so "what is left afterwards" is read back rather than
/// remembered.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct GmxMailbox {
    messages: Vec<GmxMailMessage>,
    move_calls: Vec<String>,
    remove_calls: Vec<String>,
}

impl GmxMailbox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_message(mut self, message: GmxMailMessage) -> Self {
        self.messages.push(message);
        self
    }

    /// GMX's folder list, in the order TASK 3065's reader returns it.
    pub fn folders(&self) -> Vec<String> {
        GMX_FOLDERS.iter().map(|name| (*name).to_owned()).collect()
    }

    pub fn message_ids_in(&self, folder: &str) -> Vec<String> {
        self.messages
            .iter()
            .filter(|message| message.folder == folder)
            .map(|message| message.message_id.clone())
            .collect()
    }

    pub fn subjects_in(&self, folder: &str) -> Vec<String> {
        self.messages
            .iter()
            .filter(|message| message.folder == folder)
            .map(|message| message.subject.clone())
            .collect()
    }

    pub fn count_in(&self, folder: &str) -> usize {
        self.messages
            .iter()
            .filter(|message| message.folder == folder)
            .count()
    }

    /// How many messages in `folder` carry `needle` in their subject — the
    /// count the finish line is stated in.
    pub fn count_matching_in(&self, folder: &str, needle: &str) -> usize {
        self.messages
            .iter()
            .filter(|message| message.folder == folder && message.subject.contains(needle))
            .count()
    }

    pub fn total_message_count(&self) -> usize {
        self.messages.len()
    }

    pub fn move_calls(&self) -> &[String] {
        &self.move_calls
    }

    pub fn remove_calls(&self) -> &[String] {
        &self.remove_calls
    }
}

impl SharedMailTrashSurface for GmxMailbox {
    fn service_id(&self) -> &str {
        GMX_SERVICE_ID
    }

    fn trash_folder_id(&self) -> &str {
        // Read out of GMX's own folder list rather than asserted here.
        GMX_FOLDERS
            .iter()
            .copied()
            .find(|folder| *folder == GMX_TRASH_FOLDER)
            .unwrap_or_default()
    }

    fn message_ids_in_folder(&self, folder_id: &str) -> Result<Vec<String>, String> {
        if !GMX_FOLDERS.iter().any(|folder| *folder == folder_id) {
            return Err(format!("GMX has no folder named {folder_id}"));
        }
        Ok(self.message_ids_in(folder_id))
    }

    fn visible_message(
        &self,
        folder_id: &str,
        message_id: &str,
    ) -> Result<Option<VisibleMailMessage>, String> {
        if !GMX_FOLDERS.iter().any(|folder| *folder == folder_id) {
            return Err(format!("GMX has no folder named {folder_id}"));
        }
        Ok(self
            .messages
            .iter()
            .find(|message| message.folder == folder_id && message.message_id == message_id)
            .map(|message| VisibleMailMessage {
                message_id: message.message_id.clone(),
                mailbox: message.folder.clone(),
                sender_address: message.sender_address.clone(),
            }))
    }

    fn move_message_to_trash(&mut self, folder_id: &str, message_id: &str) -> Result<(), String> {
        self.move_calls.push(format!("{folder_id}/{message_id}"));
        let mut moved = false;
        for message in &mut self.messages {
            if message.folder == folder_id && message.message_id == message_id {
                message.folder = GMX_TRASH_FOLDER.to_owned();
                moved = true;
            }
        }
        if moved {
            Ok(())
        } else {
            Err(format!("GMX has no message {message_id} in {folder_id}"))
        }
    }

    /// Removes exactly the one message named, and only from trash. This is the
    /// whole of GMX's removal: there is no GMX call here that takes the folder.
    fn remove_one_message_from_trash(&mut self, message_id: &str) -> Result<(), String> {
        self.remove_calls.push(message_id.to_owned());
        self.messages.retain(|message| {
            !(message.folder == GMX_TRASH_FOLDER && message.message_id == message_id)
        });
        Ok(())
    }
}

/// Delete one marked GMX message: the shared deleter's run, with GMX's service
/// id filled in. Every refusal, and both actions, come from TASK 3045.
pub fn delete_marked_gmx_message(
    mailbox: &mut GmxMailbox,
    signed_in_address: &str,
    folder: &str,
    message_id: &str,
) -> Result<SharedMailDeleteReceipt, SharedMailDeleteError> {
    let request =
        SharedMailDeleteRequest::marked(GMX_SERVICE_ID, signed_in_address, folder, message_id);
    delete_marked_mail_message(mailbox, &request)
}

/// The seeded GMX mailbox TASK 3067 is stated against: Sent holds three
/// messages matching `SCRUB-GX-DEL`, and Trash holds one unrelated message that
/// was already sitting there before the run.
pub const GMX_SIGNED_IN_ADDRESS: &str = "owner@gmx.test";
pub const GMX_DELETE_SUBJECT_MARK: &str = "SCRUB-GX-DEL";
pub const GMX_MARKED_MESSAGE_ID: &str = "gmx-sent-scrub-gx-del-2";
pub const GMX_UNRELATED_TRASH_MESSAGE_ID: &str = "gmx-trash-note-to-self";

pub fn seeded_gmx_delete_mailbox() -> GmxMailbox {
    GmxMailbox::new()
        .with_message(GmxMailMessage::new(
            GMX_SENT_FOLDER,
            "gmx-sent-scrub-gx-del-1",
            "SCRUB-GX-DEL-1",
            GMX_SIGNED_IN_ADDRESS,
        ))
        .with_message(GmxMailMessage::new(
            GMX_SENT_FOLDER,
            GMX_MARKED_MESSAGE_ID,
            "SCRUB-GX-DEL-2",
            GMX_SIGNED_IN_ADDRESS,
        ))
        .with_message(GmxMailMessage::new(
            GMX_SENT_FOLDER,
            "gmx-sent-scrub-gx-del-3",
            "SCRUB-GX-DEL-3",
            GMX_SIGNED_IN_ADDRESS,
        ))
        .with_message(GmxMailMessage::new(
            GMX_TRASH_FOLDER,
            GMX_UNRELATED_TRASH_MESSAGE_ID,
            "Grocery list",
            GMX_SIGNED_IN_ADDRESS,
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mail_owner_check::MailOwnerCheckError;

    #[test]
    fn task_3067_deleting_one_marked_gmx_message_leaves_two_in_sent_and_the_trash_message_alone() {
        let mut mailbox = seeded_gmx_delete_mailbox();

        let sent_before = mailbox.count_in(GMX_SENT_FOLDER);
        let sent_marked_before =
            mailbox.count_matching_in(GMX_SENT_FOLDER, GMX_DELETE_SUBJECT_MARK);
        let trash_before = mailbox.count_in(GMX_TRASH_FOLDER);
        let trash_unrelated_before =
            trash_before - mailbox.count_matching_in(GMX_TRASH_FOLDER, GMX_DELETE_SUBJECT_MARK);

        println!("TASK3067 service_id={}", mailbox.service_id());
        println!("TASK3067 folders={}", mailbox.folders().join(","));
        println!("TASK3067 trash_folder_id={}", mailbox.trash_folder_id());
        println!("TASK3067 before_sent_count={sent_before}");
        println!("TASK3067 before_sent_matching_{GMX_DELETE_SUBJECT_MARK}={sent_marked_before}");
        println!(
            "TASK3067 before_sent_subjects={}",
            mailbox.subjects_in(GMX_SENT_FOLDER).join(",")
        );
        println!("TASK3067 before_trash_count={trash_before}");
        println!("TASK3067 before_trash_unrelated_count={trash_unrelated_before}");
        println!(
            "TASK3067 before_trash_subjects={}",
            mailbox.subjects_in(GMX_TRASH_FOLDER).join(",")
        );

        assert_eq!(sent_before, 3);
        assert_eq!(
            sent_marked_before, 3,
            "Sent holds 3 messages matching {GMX_DELETE_SUBJECT_MARK} before the run"
        );
        assert_eq!(trash_before, 1);
        assert_eq!(
            trash_unrelated_before, 1,
            "the one message in Trash before the run is unrelated to {GMX_DELETE_SUBJECT_MARK}"
        );

        let receipt = delete_marked_gmx_message(
            &mut mailbox,
            GMX_SIGNED_IN_ADDRESS,
            GMX_SENT_FOLDER,
            GMX_MARKED_MESSAGE_ID,
        )
        .expect("the marked GMX message is deleted");

        let sent_after = mailbox.count_in(GMX_SENT_FOLDER);
        let sent_marked_after = mailbox.count_matching_in(GMX_SENT_FOLDER, GMX_DELETE_SUBJECT_MARK);
        let trash_after = mailbox.count_in(GMX_TRASH_FOLDER);
        let trash_unrelated_after =
            trash_after - mailbox.count_matching_in(GMX_TRASH_FOLDER, GMX_DELETE_SUBJECT_MARK);

        println!("TASK3067 steps={}", receipt.step_names().join(","));
        println!("TASK3067 after_sent_count={sent_after}");
        println!("TASK3067 after_sent_matching_{GMX_DELETE_SUBJECT_MARK}={sent_marked_after}");
        println!(
            "TASK3067 after_sent_subjects={}",
            mailbox.subjects_in(GMX_SENT_FOLDER).join(",")
        );
        println!("TASK3067 after_trash_count={trash_after}");
        println!("TASK3067 after_trash_unrelated_count={trash_unrelated_after}");
        println!(
            "TASK3067 after_trash_subjects={}",
            mailbox.subjects_in(GMX_TRASH_FOLDER).join(",")
        );
        println!(
            "TASK3067 after_trash_message_ids={}",
            mailbox.message_ids_in(GMX_TRASH_FOLDER).join(",")
        );
        println!("TASK3067 move_calls={}", mailbox.move_calls().join(","));
        println!("TASK3067 remove_calls={}", mailbox.remove_calls().join(","));
        println!(
            "TASK3067 whole_trash_emptied={}",
            receipt.whole_trash_emptied
        );

        assert_eq!(
            receipt.step_names(),
            vec!["move_to_trash", "remove_one_message_from_trash"],
            "a GMX delete is a move to trash and then removing that one message"
        );
        assert_eq!(sent_after, 2, "Sent holds 2 after the run");
        assert_eq!(sent_marked_after, 2);
        assert_eq!(trash_after, 1, "Trash holds 1 after the run");
        assert_eq!(
            trash_unrelated_after, 1,
            "the unrelated message is the one still in Trash"
        );
        assert_eq!(
            mailbox.message_ids_in(GMX_TRASH_FOLDER),
            vec![GMX_UNRELATED_TRASH_MESSAGE_ID.to_owned()],
            "the deleted message did not stay behind in Trash"
        );
        assert_eq!(receipt.folder_copies_after, 0);
        assert_eq!(receipt.trash_copies_after, 0);
        assert!(!receipt.whole_trash_emptied);
        assert_eq!(
            mailbox.remove_calls(),
            [GMX_MARKED_MESSAGE_ID.to_owned()],
            "removal is asked for by message id, exactly once"
        );
    }

    #[test]
    fn task_3067_gmx_answers_the_service_id_and_its_own_trash_folder() {
        let mailbox = seeded_gmx_delete_mailbox();

        assert_eq!(mailbox.service_id(), "gmx");
        assert_eq!(mailbox.trash_folder_id(), "Trash");
        assert_eq!(mailbox.folders(), vec!["Inbox", "Sent", "Drafts", "Trash"]);
        assert!(
            GMX_FOLDERS.contains(&mailbox.trash_folder_id()),
            "the trash folder is one GMX itself lists"
        );
    }

    #[test]
    fn task_3067_a_gmx_delete_asked_for_under_another_service_is_refused() {
        let mut mailbox = seeded_gmx_delete_mailbox();
        let request = SharedMailDeleteRequest::marked(
            "aol",
            GMX_SIGNED_IN_ADDRESS,
            GMX_SENT_FOLDER,
            GMX_MARKED_MESSAGE_ID,
        );

        let error = delete_marked_mail_message(&mut mailbox, &request).expect_err("refused");
        println!("TASK3067 refusal_wrong_service={}", error.code());

        assert_eq!(error.code(), "wrong_service");
        assert_eq!(mailbox.count_in(GMX_SENT_FOLDER), 3);
        assert_eq!(mailbox.count_in(GMX_TRASH_FOLDER), 1);
    }

    #[test]
    fn task_3067_a_gmx_message_the_account_did_not_send_is_refused_before_anything_moves() {
        let mut mailbox = GmxMailbox::new()
            .with_message(GmxMailMessage::new(
                GMX_SENT_FOLDER,
                GMX_MARKED_MESSAGE_ID,
                "SCRUB-GX-DEL-2",
                "someone-else@example.test",
            ))
            .with_message(GmxMailMessage::new(
                GMX_TRASH_FOLDER,
                GMX_UNRELATED_TRASH_MESSAGE_ID,
                "Grocery list",
                GMX_SIGNED_IN_ADDRESS,
            ));

        let error = delete_marked_gmx_message(
            &mut mailbox,
            GMX_SIGNED_IN_ADDRESS,
            GMX_SENT_FOLDER,
            GMX_MARKED_MESSAGE_ID,
        )
        .expect_err("refused");
        println!("TASK3067 refusal_not_yours={}", error.code());

        assert_eq!(error.code(), "not_yours");
        assert!(mailbox.move_calls().is_empty());
        assert!(mailbox.remove_calls().is_empty());
        assert_eq!(mailbox.count_in(GMX_SENT_FOLDER), 1);
        assert_eq!(mailbox.count_in(GMX_TRASH_FOLDER), 1);
    }

    #[test]
    fn task_3067_a_gmx_row_whose_sender_cannot_be_read_is_refused_rather_than_guessed() {
        let mut mailbox = GmxMailbox::new().with_message(GmxMailMessage {
            folder: GMX_SENT_FOLDER.to_owned(),
            message_id: GMX_MARKED_MESSAGE_ID.to_owned(),
            subject: "SCRUB-GX-DEL-2".to_owned(),
            sender_address: None,
        });

        let error = delete_marked_gmx_message(
            &mut mailbox,
            GMX_SIGNED_IN_ADDRESS,
            GMX_SENT_FOLDER,
            GMX_MARKED_MESSAGE_ID,
        )
        .expect_err("refused");
        println!("TASK3067 refusal_owner_unknown={}", error.code());

        assert_eq!(
            error,
            SharedMailDeleteError::OwnerUnknown(MailOwnerCheckError::SenderAddressUnreadable)
        );
        assert!(mailbox.remove_calls().is_empty());
        assert_eq!(mailbox.count_in(GMX_SENT_FOLDER), 1);
    }

    #[test]
    fn task_3067_an_unmarked_gmx_message_never_reaches_a_gmx_action() {
        let mut mailbox = seeded_gmx_delete_mailbox();
        let mut request = SharedMailDeleteRequest::marked(
            GMX_SERVICE_ID,
            GMX_SIGNED_IN_ADDRESS,
            GMX_SENT_FOLDER,
            GMX_MARKED_MESSAGE_ID,
        );
        request.decision = crate::shared_mail_deleter::SharedMailReviewDecision::Keep;

        let error = delete_marked_mail_message(&mut mailbox, &request).expect_err("refused");
        println!("TASK3067 refusal_not_marked={}", error.code());

        assert_eq!(error.code(), "not_marked");
        assert!(mailbox.move_calls().is_empty());
        assert!(mailbox.remove_calls().is_empty());
        assert_eq!(mailbox.count_in(GMX_SENT_FOLDER), 3);
        assert_eq!(mailbox.count_in(GMX_TRASH_FOLDER), 1);
    }

    #[test]
    fn task_3067_removing_one_gmx_message_never_takes_the_rest_of_trash() {
        let mut mailbox = seeded_gmx_delete_mailbox();

        // The only removal GMX offers is by message id. Asking for an id that is
        // not there removes nothing, rather than clearing the folder.
        mailbox
            .remove_one_message_from_trash("gmx-no-such-message")
            .expect("removing an unknown id is not an error");

        println!(
            "TASK3067 unrelated_removal_trash_after={}",
            mailbox.message_ids_in(GMX_TRASH_FOLDER).join(",")
        );
        assert_eq!(mailbox.count_in(GMX_TRASH_FOLDER), 1);
        assert_eq!(mailbox.count_in(GMX_SENT_FOLDER), 3);
    }
}
