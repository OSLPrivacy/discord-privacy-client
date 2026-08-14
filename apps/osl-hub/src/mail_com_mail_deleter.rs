//! Mail.com's fill-in of the shared mail deleter (TASK 3070).
//!
//! TASK 3045 wrote the deletion order once, for every mail service: move the
//! marked message from its folder into trash, then remove **that one message**
//! from trash, by id. This module does not write a second copy of that order.
//! It fills in [`SharedMailTrashSurface`] for Mail.com — the folder names
//! Mail.com actually uses, the reads, the move, and the single removal — and
//! hands the run to [`delete_marked_mail_message`], so the "never empty the
//! whole trash" rule is enforced by the same code every other mail service is
//! judged by.
//!
//! What is Mail.com-specific and lives here:
//!
//! * the service id `mail.com` and the four folders the TASK 3068 reader found
//!   (`Inbox`, `Sent`, `Archive`, `Trash`), so a folder Mail.com does not have
//!   is a read failure rather than a silently empty folder;
//! * the trash folder is named `Trash` by Mail.com, not guessed by the deleter;
//! * the removal from trash is by message id and touches exactly one row.
//!
//! There is no `empty_trash` here, because [`SharedMailTrashSurface`] has no
//! such call to fill in.
//!
//! Pure: an in-process mailbox model, no network and no Mail.com credentials.
//! The seeded fixture is the same account, address and folder set the TASK 3068
//! Mail.com reader uses, so the deleter and the reader are talking about one
//! mailbox.

use crate::mail_owner_check::VisibleMailMessage;
use crate::shared_mail_deleter::{SharedMailDeleteRequest, SharedMailTrashSurface};

/// Mail.com's service id, as the deletion request names it.
pub const MAIL_COM_SERVICE_ID: &str = "mail.com";

/// The account the seeded Mail.com mailbox belongs to (TASK 3068's account).
pub const MAIL_COM_ACCOUNT_ID: &str = "acct-scrub-mail-com";

/// The signed-in address the owner check is run against.
pub const MAIL_COM_SIGNED_IN_ADDRESS: &str = "signed-in@mail.com";

/// The folder marked messages are deleted from in this run.
pub const MAIL_COM_SENT_FOLDER_ID: &str = "Sent";

/// What Mail.com calls its trash. Named by the service; the shared deleter
/// never guesses it.
pub const MAIL_COM_TRASH_FOLDER_ID: &str = "Trash";

/// Every folder Mail.com offers. A read of anything else is refused rather than
/// answered with an empty list, so a typo'd folder cannot read as "nothing to
/// delete".
pub const MAIL_COM_FOLDER_IDS: [&str; 4] = ["Inbox", "Sent", "Archive", "Trash"];

/// The subject marker the three seeded Sent messages carry.
pub const MAIL_COM_DELETION_MARKER: &str = "SCRUB-MC-DEL";

/// One message as Mail.com's mailbox holds it.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MailComMessage {
    pub folder_id: String,
    pub message_id: String,
    pub subject: String,
    /// `None` when Mail.com will not give up a readable sender address. The
    /// owner check refuses such a row rather than guessing at it.
    pub sender_address: Option<String>,
}

impl MailComMessage {
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

    pub fn with_unreadable_sender(mut self) -> Self {
        self.sender_address = None;
        self
    }
}

/// Mail.com's mailbox, and its fill-in of the shared deleter's surface.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MailComMailbox {
    messages: Vec<MailComMessage>,
    move_calls: Vec<String>,
    remove_calls: Vec<String>,
    /// A deliberately wrong fill-in used by the check that the whole-trash
    /// guard is not decoration: asked to remove one message, it takes the whole
    /// trash instead.
    removal_takes_whole_trash: bool,
}

impl MailComMailbox {
    pub fn empty() -> Self {
        Self {
            messages: Vec::new(),
            move_calls: Vec::new(),
            remove_calls: Vec::new(),
            removal_takes_whole_trash: false,
        }
    }

    pub fn with_message(mut self, message: MailComMessage) -> Self {
        self.messages.push(message);
        self
    }

    /// The bad fill-in: one removal, whole trash gone. Used to show the shared
    /// deleter's guard refuses it.
    pub fn removing_one_takes_whole_trash(mut self) -> Self {
        self.removal_takes_whole_trash = true;
        self
    }

    pub fn is_known_folder(folder_id: &str) -> bool {
        MAIL_COM_FOLDER_IDS.contains(&folder_id)
    }

    pub fn folder_message_ids(&self, folder_id: &str) -> Vec<String> {
        self.messages
            .iter()
            .filter(|message| message.folder_id == folder_id)
            .map(|message| message.message_id.clone())
            .collect()
    }

    /// How many messages a folder holds right now, read out of the mailbox.
    pub fn folder_count(&self, folder_id: &str) -> usize {
        self.messages
            .iter()
            .filter(|message| message.folder_id == folder_id)
            .count()
    }

    /// How many messages in a folder match a marker, by subject or by id. This
    /// is what the finish line's "3 messages matching SCRUB-MC-DEL" counts.
    pub fn folder_count_matching(&self, folder_id: &str, marker: &str) -> usize {
        self.messages
            .iter()
            .filter(|message| {
                message.folder_id == folder_id
                    && (message.subject.contains(marker) || message.message_id.contains(marker))
            })
            .count()
    }

    pub fn count_in_folder(&self, folder_id: &str, message_id: &str) -> usize {
        self.messages
            .iter()
            .filter(|message| message.folder_id == folder_id && message.message_id == message_id)
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

impl SharedMailTrashSurface for MailComMailbox {
    fn service_id(&self) -> &str {
        MAIL_COM_SERVICE_ID
    }

    fn trash_folder_id(&self) -> &str {
        MAIL_COM_TRASH_FOLDER_ID
    }

    fn message_ids_in_folder(&self, folder_id: &str) -> Result<Vec<String>, String> {
        if !Self::is_known_folder(folder_id) {
            return Err(format!("Mail.com has no folder named {folder_id}"));
        }
        Ok(self.folder_message_ids(folder_id))
    }

    fn visible_message(
        &self,
        folder_id: &str,
        message_id: &str,
    ) -> Result<Option<VisibleMailMessage>, String> {
        if !Self::is_known_folder(folder_id) {
            return Err(format!("Mail.com has no folder named {folder_id}"));
        }
        Ok(self
            .messages
            .iter()
            .find(|message| message.folder_id == folder_id && message.message_id == message_id)
            .map(|message| VisibleMailMessage {
                message_id: message.message_id.clone(),
                mailbox: message.folder_id.clone(),
                sender_address: message.sender_address.clone(),
            }))
    }

    fn move_message_to_trash(&mut self, folder_id: &str, message_id: &str) -> Result<(), String> {
        if !Self::is_known_folder(folder_id) {
            return Err(format!("Mail.com has no folder named {folder_id}"));
        }
        self.move_calls.push(format!("{folder_id}/{message_id}"));
        let mut moved = 0usize;
        for message in &mut self.messages {
            if message.folder_id == folder_id && message.message_id == message_id {
                message.folder_id = MAIL_COM_TRASH_FOLDER_ID.to_owned();
                moved += 1;
            }
        }
        if moved == 0 {
            return Err(format!("Mail.com folder {folder_id} has no {message_id}"));
        }
        Ok(())
    }

    fn remove_one_message_from_trash(&mut self, message_id: &str) -> Result<(), String> {
        self.remove_calls.push(message_id.to_owned());
        if self.removal_takes_whole_trash {
            self.messages
                .retain(|message| message.folder_id != MAIL_COM_TRASH_FOLDER_ID);
            return Ok(());
        }
        let before = self.messages.len();
        // Exactly one row leaves: the named message, and only its copy in trash.
        self.messages.retain(|message| {
            !(message.folder_id == MAIL_COM_TRASH_FOLDER_ID && message.message_id == message_id)
        });
        if self.messages.len() == before {
            return Err(format!("Mail.com trash has no {message_id}"));
        }
        Ok(())
    }
}

/// The message TASK 3070's run deletes: one of the three `SCRUB-MC-DEL`
/// messages in Sent, the one the review marked.
pub const MAIL_COM_MARKED_MESSAGE_ID: &str = "SCRUB-MC-DEL-002";

/// The message that is already sitting in Mail.com's trash before the run, and
/// has nothing to do with it.
pub const MAIL_COM_UNRELATED_TRASH_MESSAGE_ID: &str = "mail-com-3070-old-newsletter";

/// The seeded Mail.com mailbox this task's run acts on: three `SCRUB-MC-DEL`
/// messages in Sent and one unrelated message already in Trash. The account,
/// address and folder names are TASK 3068's, so this is the same mailbox the
/// Mail.com reader reads.
pub fn seeded_mail_com_mailbox_for_deletion() -> MailComMailbox {
    MailComMailbox::empty()
        .with_message(MailComMessage::new(
            MAIL_COM_SENT_FOLDER_ID,
            "SCRUB-MC-DEL-001",
            "SCRUB-MC-DEL Mail.com order confirmation",
            MAIL_COM_SIGNED_IN_ADDRESS,
        ))
        .with_message(MailComMessage::new(
            MAIL_COM_SENT_FOLDER_ID,
            MAIL_COM_MARKED_MESSAGE_ID,
            "SCRUB-MC-DEL Mail.com delivery note",
            MAIL_COM_SIGNED_IN_ADDRESS,
        ))
        .with_message(MailComMessage::new(
            MAIL_COM_SENT_FOLDER_ID,
            "SCRUB-MC-DEL-003",
            "SCRUB-MC-DEL Mail.com account notice",
            MAIL_COM_SIGNED_IN_ADDRESS,
        ))
        .with_message(MailComMessage::new(
            MAIL_COM_TRASH_FOLDER_ID,
            MAIL_COM_UNRELATED_TRASH_MESSAGE_ID,
            "Mail.com weekly newsletter",
            MAIL_COM_SIGNED_IN_ADDRESS,
        ))
}

/// The request the run makes: delete the one marked `SCRUB-MC-DEL` message out
/// of Mail.com's Sent folder.
pub fn mail_com_marked_delete_request() -> SharedMailDeleteRequest {
    SharedMailDeleteRequest::marked(
        MAIL_COM_SERVICE_ID,
        MAIL_COM_SIGNED_IN_ADDRESS,
        MAIL_COM_SENT_FOLDER_ID,
        MAIL_COM_MARKED_MESSAGE_ID,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_mail_deleter::{
        delete_marked_mail_message, SharedMailDeleteError, SharedMailReviewDecision,
    };

    #[test]
    fn task_3070_marked_mail_com_message_leaves_sent_at_two_and_trash_at_one() {
        let mut mailbox = seeded_mail_com_mailbox_for_deletion();

        let sent_matching_before =
            mailbox.folder_count_matching(MAIL_COM_SENT_FOLDER_ID, MAIL_COM_DELETION_MARKER);
        let trash_before = mailbox.folder_count(MAIL_COM_TRASH_FOLDER_ID);
        let trash_matching_before =
            mailbox.folder_count_matching(MAIL_COM_TRASH_FOLDER_ID, MAIL_COM_DELETION_MARKER);
        println!("TASK3070 service_id={MAIL_COM_SERVICE_ID}");
        println!("TASK3070 before_sent_matching_SCRUB-MC-DEL={sent_matching_before}");
        println!("TASK3070 before_trash_count={trash_before}");
        println!("TASK3070 before_trash_matching_SCRUB-MC-DEL={trash_matching_before}");
        assert_eq!(sent_matching_before, 3);
        assert_eq!(trash_before, 1);
        assert_eq!(trash_matching_before, 0, "the trash message is unrelated");

        let receipt = delete_marked_mail_message(&mut mailbox, &mail_com_marked_delete_request())
            .expect("the marked Mail.com message deletes");

        let sent_matching_after =
            mailbox.folder_count_matching(MAIL_COM_SENT_FOLDER_ID, MAIL_COM_DELETION_MARKER);
        let trash_after = mailbox.folder_count(MAIL_COM_TRASH_FOLDER_ID);
        println!("TASK3070 steps={}", receipt.step_names().join(","));
        println!("TASK3070 after_sent_matching_SCRUB-MC-DEL={sent_matching_after}");
        println!("TASK3070 after_trash_count={trash_after}");
        println!(
            "TASK3070 after_sent_ids={}",
            mailbox.folder_message_ids(MAIL_COM_SENT_FOLDER_ID).join(",")
        );
        println!(
            "TASK3070 after_trash_ids={}",
            mailbox
                .folder_message_ids(MAIL_COM_TRASH_FOLDER_ID)
                .join(",")
        );

        assert_eq!(sent_matching_after, 2);
        assert_eq!(trash_after, 1);
        // The message that went is the marked one, and only that one.
        assert_eq!(
            mailbox.count_in_folder(MAIL_COM_SENT_FOLDER_ID, MAIL_COM_MARKED_MESSAGE_ID),
            0
        );
        assert_eq!(
            mailbox.count_in_folder(MAIL_COM_TRASH_FOLDER_ID, MAIL_COM_MARKED_MESSAGE_ID),
            0
        );
        assert_eq!(
            mailbox.folder_message_ids(MAIL_COM_SENT_FOLDER_ID),
            vec!["SCRUB-MC-DEL-001".to_owned(), "SCRUB-MC-DEL-003".to_owned()]
        );
        // The unrelated message that was already in trash is untouched.
        assert_eq!(
            mailbox.folder_message_ids(MAIL_COM_TRASH_FOLDER_ID),
            vec![MAIL_COM_UNRELATED_TRASH_MESSAGE_ID.to_owned()]
        );
        assert_eq!(receipt.folder_copies_after, 0);
        assert_eq!(receipt.trash_copies_after, 0);
        assert!(!receipt.whole_trash_emptied);
        assert_eq!(receipt.trash_folder_id, MAIL_COM_TRASH_FOLDER_ID);
        assert_eq!(
            receipt.step_names(),
            vec!["move_to_trash", "remove_one_message_from_trash"]
        );
        // One move, one removal, and the removal names one message id.
        assert_eq!(mailbox.move_calls(), ["Sent/SCRUB-MC-DEL-002"]);
        assert_eq!(mailbox.remove_calls(), ["SCRUB-MC-DEL-002"]);
    }

    #[test]
    fn task_3070_an_unmarked_mail_com_message_is_refused_and_nothing_moves() {
        let mut mailbox = seeded_mail_com_mailbox_for_deletion();
        let before = mailbox.clone();
        let mut request = mail_com_marked_delete_request();
        request.decision = SharedMailReviewDecision::Keep;

        let error = delete_marked_mail_message(&mut mailbox, &request)
            .expect_err("an unmarked Mail.com message is refused");
        println!("TASK3070 unmarked_refusal={}", error.code());
        assert_eq!(error.code(), "not_marked");
        assert_eq!(mailbox, before, "a refusal moves nothing");
        assert!(mailbox.move_calls().is_empty());
        assert!(mailbox.remove_calls().is_empty());
    }

    #[test]
    fn task_3070_a_mail_com_message_the_account_did_not_send_is_refused() {
        let mut mailbox = seeded_mail_com_mailbox_for_deletion().with_message(
            MailComMessage::new(
                "Inbox",
                "inbox-mail-com-3070-001",
                "SCRUB-MC-DEL looking message that arrived",
                "sender-one@example.test",
            ),
        );
        let before = mailbox.clone();
        let request = SharedMailDeleteRequest::marked(
            MAIL_COM_SERVICE_ID,
            MAIL_COM_SIGNED_IN_ADDRESS,
            "Inbox",
            "inbox-mail-com-3070-001",
        );

        let error = delete_marked_mail_message(&mut mailbox, &request)
            .expect_err("a message Mail.com did not send from this account is refused");
        println!("TASK3070 not_yours_refusal={}", error.code());
        println!("TASK3070 not_yours_reason={}", error.reason());
        assert_eq!(error.code(), "not_yours");
        assert_eq!(mailbox, before);
    }

    #[test]
    fn task_3070_a_mail_com_message_with_an_unreadable_sender_is_refused_not_guessed() {
        let mut mailbox = MailComMailbox::empty()
            .with_message(
                MailComMessage::new(
                    MAIL_COM_SENT_FOLDER_ID,
                    MAIL_COM_MARKED_MESSAGE_ID,
                    "SCRUB-MC-DEL Mail.com delivery note",
                    MAIL_COM_SIGNED_IN_ADDRESS,
                )
                .with_unreadable_sender(),
            )
            .with_message(MailComMessage::new(
                MAIL_COM_TRASH_FOLDER_ID,
                MAIL_COM_UNRELATED_TRASH_MESSAGE_ID,
                "Mail.com weekly newsletter",
                MAIL_COM_SIGNED_IN_ADDRESS,
            ));
        let before = mailbox.clone();

        let error = delete_marked_mail_message(&mut mailbox, &mail_com_marked_delete_request())
            .expect_err("an unreadable Mail.com sender is refused");
        println!("TASK3070 owner_unknown_refusal={}", error.code());
        assert_eq!(error.code(), "owner_unknown");
        assert_eq!(mailbox, before);
    }

    #[test]
    fn task_3070_a_folder_mail_com_does_not_have_is_refused() {
        let mut mailbox = seeded_mail_com_mailbox_for_deletion();
        let request = SharedMailDeleteRequest::marked(
            MAIL_COM_SERVICE_ID,
            MAIL_COM_SIGNED_IN_ADDRESS,
            "Deleted Items",
            MAIL_COM_MARKED_MESSAGE_ID,
        );

        let error = delete_marked_mail_message(&mut mailbox, &request)
            .expect_err("Mail.com has no folder called Deleted Items");
        println!("TASK3070 unknown_folder_refusal={}", error.code());
        assert_eq!(error.code(), "folder_read_failed");
        assert_eq!(
            mailbox.folder_count_matching(MAIL_COM_SENT_FOLDER_ID, MAIL_COM_DELETION_MARKER),
            3
        );
    }

    #[test]
    fn task_3070_a_request_naming_another_service_is_refused() {
        let mut mailbox = seeded_mail_com_mailbox_for_deletion();
        let request = SharedMailDeleteRequest::marked(
            "gmx",
            MAIL_COM_SIGNED_IN_ADDRESS,
            MAIL_COM_SENT_FOLDER_ID,
            MAIL_COM_MARKED_MESSAGE_ID,
        );

        let error = delete_marked_mail_message(&mut mailbox, &request)
            .expect_err("a GMX request is not Mail.com's to run");
        println!("TASK3070 wrong_service_refusal={}", error.code());
        assert_eq!(error.code(), "wrong_service");
        assert_eq!(mailbox.total_message_count(), 4);
    }

    #[test]
    fn task_3070_a_mail_com_fill_in_that_empties_the_whole_trash_is_refused() {
        let mut mailbox = seeded_mail_com_mailbox_for_deletion().removing_one_takes_whole_trash();

        let error = delete_marked_mail_message(&mut mailbox, &mail_com_marked_delete_request())
            .expect_err("emptying Mail.com's whole trash is refused");
        println!("TASK3070 whole_trash_refusal={}", error.code());
        println!("TASK3070 whole_trash_reason={}", error.reason());
        assert_eq!(error, SharedMailDeleteError::WholeTrashEmptied);
        assert_eq!(error.code(), "whole_trash_emptied");
        // The refusal is what the run reports; the bad fill-in still did the
        // damage, which is exactly why the guard reads the trash back.
        assert_eq!(mailbox.folder_count(MAIL_COM_TRASH_FOLDER_ID), 0);
    }

    #[test]
    fn task_3070_mail_com_names_its_own_trash_and_folders() {
        let mailbox = seeded_mail_com_mailbox_for_deletion();
        assert_eq!(mailbox.service_id(), "mail.com");
        assert_eq!(mailbox.trash_folder_id(), "Trash");
        assert_eq!(
            MAIL_COM_FOLDER_IDS.to_vec(),
            vec!["Inbox", "Sent", "Archive", "Trash"]
        );
        assert!(mailbox.message_ids_in_folder("Bin").is_err());
        println!("TASK3070 folders={}", MAIL_COM_FOLDER_IDS.join(","));
    }
}
