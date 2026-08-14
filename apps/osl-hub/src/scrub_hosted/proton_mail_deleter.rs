//! Proton Mail fill-in for the shared mail deleter (TASK 3058).
//!
//! The removal order itself is not written here. It is gate 3045's shared
//! deleter, [`delete_marked_mail_message`], which moves the marked message to
//! trash and then takes **that one message** back out of trash, by id. What
//! this module supplies is the part that is Proton's and nobody else's:
//!
//! * Proton's service id, `proton`;
//! * the folder Proton calls its trash — `Trash` (gate 3056 read exactly four
//!   folders off a seeded Proton mailbox: Inbox, Sent, Archive, Trash);
//! * how a Proton folder is listed, how a Proton message is moved to Trash, and
//!   how one message — one, by id — is permanently removed from Trash.
//!
//! Proton files a message under a label rather than copying it into a folder,
//! so "move to Trash" is a relabel of the one message, and the second half is a
//! permanent delete of that same id. [`ProtonMailbox`] models exactly that, and
//! its permanent delete **refuses any message that is not in Trash**, so the
//! second half cannot quietly reach past the trash into a live folder.
//!
//! There is no empty-trash call here, because [`SharedMailTrashSurface`] has no
//! empty-trash call to fill in. A Proton account's Trash holds mail the person
//! put there themselves; it is not this command's to destroy.

use crate::mail_owner_check::VisibleMailMessage;
use crate::shared_mail_deleter::{
    delete_marked_mail_message, SharedMailDeleteError, SharedMailDeleteReceipt,
    SharedMailDeleteRequest, SharedMailReviewDecision, SharedMailTrashSurface,
};

/// Proton Mail's service id.
///
/// It lives in this module rather than in `proton_mail.rs` so the deleter
/// fill-in does not drag the mailbox reader's plumbing in behind it, the same
/// move gate 3045 made with the owner check. `proton_mail.rs` re-exports it, so
/// every existing `proton_mail::PROTON_MAIL_SERVICE_ID` path still resolves.
pub const PROTON_MAIL_SERVICE_ID: &str = "proton";

/// What Proton calls its trash. Named, not guessed: gate 3056's seeded Proton
/// mailbox lists this folder alongside Inbox, Sent and Archive.
pub const PROTON_MAIL_TRASH_FOLDER_ID: &str = "Trash";

/// The four folders a Proton mailbox reads back, in the order gate 3056 read
/// them.
pub const PROTON_MAIL_FOLDER_IDS: [&str; 4] = ["Inbox", "Sent", "Archive", "Trash"];

/// One message sitting under one Proton label.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProtonMailboxMessage {
    pub folder_id: String,
    pub message_id: String,
    pub subject: String,
    pub sender_address: Option<String>,
}

impl ProtonMailboxMessage {
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

    /// A message whose sender address cannot be read. Gate 3044's rule is that
    /// this fails closed rather than being guessed, so the fixture can say so.
    pub fn with_unreadable_sender(mut self) -> Self {
        self.sender_address = None;
        self
    }
}

/// A Proton mailbox: its folders and the messages filed under them.
///
/// Counts are answered by walking the messages, so anything read back out of
/// this is what the mailbox holds now, not what a caller remembered.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProtonMailbox {
    folder_ids: Vec<String>,
    messages: Vec<ProtonMailboxMessage>,
    move_calls: Vec<String>,
    permanent_delete_calls: Vec<String>,
}

impl Default for ProtonMailbox {
    fn default() -> Self {
        Self::new()
    }
}

impl ProtonMailbox {
    /// An empty Proton mailbox with Proton's four folders.
    pub fn new() -> Self {
        Self {
            folder_ids: PROTON_MAIL_FOLDER_IDS
                .iter()
                .map(|folder| (*folder).to_owned())
                .collect(),
            messages: Vec::new(),
            move_calls: Vec::new(),
            permanent_delete_calls: Vec::new(),
        }
    }

    pub fn with_message(mut self, message: ProtonMailboxMessage) -> Self {
        self.messages.push(message);
        self
    }

    pub fn folder_ids(&self) -> &[String] {
        &self.folder_ids
    }

    pub fn has_folder(&self, folder_id: &str) -> bool {
        self.folder_ids.iter().any(|folder| folder == folder_id)
    }

    pub fn message_ids_in(&self, folder_id: &str) -> Vec<String> {
        self.messages
            .iter()
            .filter(|message| message.folder_id == folder_id)
            .map(|message| message.message_id.clone())
            .collect()
    }

    pub fn count_in(&self, folder_id: &str) -> usize {
        self.message_ids_in(folder_id).len()
    }

    pub fn count_of_message_in(&self, folder_id: &str, message_id: &str) -> usize {
        self.messages
            .iter()
            .filter(|message| message.folder_id == folder_id && message.message_id == message_id)
            .count()
    }

    /// The messages in one folder whose subject carries a marker — how the
    /// seeded Scrub fixtures tell their own mail apart from everything else in
    /// the mailbox.
    pub fn subjects_matching_in(&self, folder_id: &str, marker: &str) -> Vec<String> {
        self.messages
            .iter()
            .filter(|message| message.folder_id == folder_id && message.subject.contains(marker))
            .map(|message| message.subject.clone())
            .collect()
    }

    pub fn count_matching_in(&self, folder_id: &str, marker: &str) -> usize {
        self.subjects_matching_in(folder_id, marker).len()
    }

    pub fn message_ids_matching_in(&self, folder_id: &str, marker: &str) -> Vec<String> {
        self.messages
            .iter()
            .filter(|message| message.folder_id == folder_id && message.subject.contains(marker))
            .map(|message| message.message_id.clone())
            .collect()
    }

    /// Every move-to-Trash Proton was asked for, as `folder/message-id`.
    pub fn move_calls(&self) -> &[String] {
        &self.move_calls
    }

    /// Every permanent delete Proton was asked for, by message id. One run
    /// leaves exactly one entry here.
    pub fn permanent_delete_calls(&self) -> &[String] {
        &self.permanent_delete_calls
    }

    pub fn total_message_count(&self) -> usize {
        self.messages.len()
    }
}

impl SharedMailTrashSurface for ProtonMailbox {
    fn service_id(&self) -> &str {
        PROTON_MAIL_SERVICE_ID
    }

    fn trash_folder_id(&self) -> &str {
        PROTON_MAIL_TRASH_FOLDER_ID
    }

    fn message_ids_in_folder(&self, folder_id: &str) -> Result<Vec<String>, String> {
        if !self.has_folder(folder_id) {
            return Err(format!("Proton Mail has no folder named {folder_id}"));
        }
        Ok(self.message_ids_in(folder_id))
    }

    fn visible_message(
        &self,
        folder_id: &str,
        message_id: &str,
    ) -> Result<Option<VisibleMailMessage>, String> {
        if !self.has_folder(folder_id) {
            return Err(format!("Proton Mail has no folder named {folder_id}"));
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
        self.move_calls.push(format!("{folder_id}/{message_id}"));
        if !self.has_folder(folder_id) {
            return Err(format!("Proton Mail has no folder named {folder_id}"));
        }
        let mut moved = false;
        for message in &mut self.messages {
            if message.folder_id == folder_id && message.message_id == message_id {
                // Proton files a message under a label: the move is a relabel of
                // the one message, not a copy into a second place.
                message.folder_id = PROTON_MAIL_TRASH_FOLDER_ID.to_owned();
                moved = true;
            }
        }
        if moved {
            Ok(())
        } else {
            Err(format!(
                "Proton Mail has no message {message_id} in {folder_id}"
            ))
        }
    }

    fn remove_one_message_from_trash(&mut self, message_id: &str) -> Result<(), String> {
        self.permanent_delete_calls.push(message_id.to_owned());
        let in_trash = self.count_of_message_in(PROTON_MAIL_TRASH_FOLDER_ID, message_id);
        if in_trash == 0 {
            // Proton permanently deletes out of Trash and nowhere else, so this
            // half of the run cannot reach past the trash into a live folder.
            return Err(format!(
                "Proton Mail message {message_id} is not in {PROTON_MAIL_TRASH_FOLDER_ID}"
            ));
        }
        self.messages.retain(|message| {
            !(message.folder_id == PROTON_MAIL_TRASH_FOLDER_ID && message.message_id == message_id)
        });
        Ok(())
    }
}

/// One delete request against Proton Mail, carrying the review's own two facts:
/// whether the review reached this row at all, and what it decided.
pub fn proton_mail_delete_request(
    signed_in_address: &str,
    folder_id: &str,
    message_id: &str,
    reviewed: bool,
    decision: SharedMailReviewDecision,
) -> SharedMailDeleteRequest {
    SharedMailDeleteRequest {
        service_id: PROTON_MAIL_SERVICE_ID.to_owned(),
        signed_in_address: signed_in_address.to_owned(),
        folder_id: folder_id.to_owned(),
        message_id: message_id.to_owned(),
        reviewed,
        decision,
    }
}

/// A request for a row the review reached and marked for deletion.
pub fn proton_mail_marked_delete_request(
    signed_in_address: &str,
    folder_id: &str,
    message_id: &str,
) -> SharedMailDeleteRequest {
    proton_mail_delete_request(
        signed_in_address,
        folder_id,
        message_id,
        true,
        SharedMailReviewDecision::MarkedForDeletion,
    )
}

/// Run the shared mail deleter against a Proton mailbox.
///
/// Every refusal is the shared deleter's, taken before the first action, so a
/// refused request never reaches Proton at all.
pub fn delete_marked_proton_mail_message(
    mailbox: &mut ProtonMailbox,
    request: &SharedMailDeleteRequest,
) -> Result<SharedMailDeleteReceipt, SharedMailDeleteError> {
    delete_marked_mail_message(mailbox, request)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGNED_IN: &str = "scrub.owner@proton.test";
    const SENT: &str = "Sent";
    const MARKER: &str = "SCRUB-PR-DEL";
    const MARKED_MESSAGE: &str = "proton-3058-sent-002";
    const UNRELATED_IN_TRASH: &str = "proton-3058-trash-was-already-here";

    /// The seeded Proton mailbox this task's finish line names: Sent holds three
    /// messages matching SCRUB-PR-DEL, Trash holds one unrelated message.
    fn seeded_proton_mailbox() -> ProtonMailbox {
        ProtonMailbox::new()
            .with_message(ProtonMailboxMessage::new(
                SENT,
                "proton-3058-sent-001",
                "SCRUB-PR-DEL export request",
                SIGNED_IN,
            ))
            .with_message(ProtonMailboxMessage::new(
                SENT,
                MARKED_MESSAGE,
                "SCRUB-PR-DEL erasure demand",
                SIGNED_IN,
            ))
            .with_message(ProtonMailboxMessage::new(
                SENT,
                "proton-3058-sent-003",
                "SCRUB-PR-DEL confirmation note",
                SIGNED_IN,
            ))
            .with_message(ProtonMailboxMessage::new(
                PROTON_MAIL_TRASH_FOLDER_ID,
                UNRELATED_IN_TRASH,
                "Holiday photos from last summer",
                SIGNED_IN,
            ))
    }

    #[test]
    fn task_3058_deleting_one_marked_sent_message_leaves_two_and_keeps_the_unrelated_trash_message()
    {
        let mut mailbox = seeded_proton_mailbox();

        let sent_matching_before = mailbox.count_matching_in(SENT, MARKER);
        let trash_before = mailbox.message_ids_in(PROTON_MAIL_TRASH_FOLDER_ID);
        println!("TASK3058 before_sent_matching_{MARKER}={sent_matching_before}");
        println!("TASK3058 before_trash_count={}", trash_before.len());
        println!("TASK3058 before_trash_ids={}", trash_before.join(","));

        let request = proton_mail_marked_delete_request(SIGNED_IN, SENT, MARKED_MESSAGE);
        let receipt = delete_marked_proton_mail_message(&mut mailbox, &request)
            .expect("delete one marked Proton message");

        let sent_matching_after = mailbox.count_matching_in(SENT, MARKER);
        let trash_after = mailbox.message_ids_in(PROTON_MAIL_TRASH_FOLDER_ID);
        println!("TASK3058 steps={}", receipt.step_names().join(","));
        println!("TASK3058 after_sent_matching_{MARKER}={sent_matching_after}");
        println!("TASK3058 after_trash_count={}", trash_after.len());
        println!("TASK3058 after_trash_ids={}", trash_after.join(","));
        println!(
            "TASK3058 permanent_delete_calls={}",
            mailbox.permanent_delete_calls().join(",")
        );

        assert_eq!(sent_matching_before, 3);
        assert_eq!(trash_before, vec![UNRELATED_IN_TRASH.to_owned()]);
        assert_eq!(sent_matching_after, 2);
        assert_eq!(trash_after, vec![UNRELATED_IN_TRASH.to_owned()]);
        assert_eq!(receipt.folder_copies_after, 0);
        assert_eq!(receipt.trash_copies_after, 0);
        assert!(!receipt.whole_trash_emptied);
        assert_eq!(
            receipt.step_names(),
            vec!["move_to_trash", "remove_one_message_from_trash"]
        );
    }

    #[test]
    fn task_3058_the_single_message_is_removed_from_trash_by_id() {
        let mut mailbox = seeded_proton_mailbox();
        let request = proton_mail_marked_delete_request(SIGNED_IN, SENT, MARKED_MESSAGE);
        delete_marked_proton_mail_message(&mut mailbox, &request).expect("delete one message");

        println!("TASK3058 move_calls={}", mailbox.move_calls().join(","));
        println!(
            "TASK3058 permanent_delete_call_count={}",
            mailbox.permanent_delete_calls().len()
        );
        assert_eq!(mailbox.move_calls(), [format!("{SENT}/{MARKED_MESSAGE}")]);
        assert_eq!(
            mailbox.permanent_delete_calls(),
            [MARKED_MESSAGE.to_owned()]
        );
        // One message left the mailbox, and only one.
        assert_eq!(mailbox.total_message_count(), 3);
    }

    #[test]
    fn task_3058_proton_refuses_a_permanent_delete_of_a_message_that_is_not_in_trash() {
        let mut mailbox = seeded_proton_mailbox();
        let refusal = mailbox
            .remove_one_message_from_trash(MARKED_MESSAGE)
            .expect_err("Proton must not permanently delete outside Trash");
        println!("TASK3058 permanent_delete_outside_trash_refusal={refusal}");
        assert!(refusal.contains("is not in Trash"));
        assert_eq!(mailbox.count_matching_in(SENT, MARKER), 3);
    }

    #[test]
    fn task_3058_an_unmarked_message_is_refused_before_anything_moves() {
        let mut mailbox = seeded_proton_mailbox();
        let keep = proton_mail_delete_request(
            SIGNED_IN,
            SENT,
            MARKED_MESSAGE,
            true,
            SharedMailReviewDecision::Keep,
        );
        let unreviewed = proton_mail_delete_request(
            SIGNED_IN,
            SENT,
            MARKED_MESSAGE,
            false,
            SharedMailReviewDecision::MarkedForDeletion,
        );

        let keep_refusal = delete_marked_proton_mail_message(&mut mailbox, &keep)
            .expect_err("a kept message is not deleted");
        let unreviewed_refusal = delete_marked_proton_mail_message(&mut mailbox, &unreviewed)
            .expect_err("a row the review never reached is not deleted");

        println!("TASK3058 keep_refusal={}", keep_refusal.code());
        println!("TASK3058 unreviewed_refusal={}", unreviewed_refusal.code());
        assert_eq!(keep_refusal.code(), "not_marked");
        assert_eq!(unreviewed_refusal.code(), "not_marked");
        assert!(mailbox.move_calls().is_empty());
        assert!(mailbox.permanent_delete_calls().is_empty());
        assert_eq!(mailbox.count_matching_in(SENT, MARKER), 3);
    }

    #[test]
    fn task_3058_a_message_another_address_sent_is_refused() {
        let mut mailbox = ProtonMailbox::new().with_message(ProtonMailboxMessage::new(
            SENT,
            "proton-3058-not-ours",
            "SCRUB-PR-DEL forwarded by someone else",
            "someone.else@proton.test",
        ));
        let request = proton_mail_marked_delete_request(SIGNED_IN, SENT, "proton-3058-not-ours");
        let refusal = delete_marked_proton_mail_message(&mut mailbox, &request)
            .expect_err("another address's message is not deleted");
        println!("TASK3058 not_yours_refusal={}", refusal.code());
        assert_eq!(refusal.code(), "not_yours");
        assert!(mailbox.move_calls().is_empty());
    }

    #[test]
    fn task_3058_an_unreadable_sender_is_refused_rather_than_guessed() {
        let mut mailbox = ProtonMailbox::new().with_message(
            ProtonMailboxMessage::new(
                SENT,
                "proton-3058-no-sender",
                "SCRUB-PR-DEL sender missing",
                SIGNED_IN,
            )
            .with_unreadable_sender(),
        );
        let request = proton_mail_marked_delete_request(SIGNED_IN, SENT, "proton-3058-no-sender");
        let refusal = delete_marked_proton_mail_message(&mut mailbox, &request)
            .expect_err("an unreadable sender is not guessed");
        println!("TASK3058 owner_unknown_refusal={}", refusal.code());
        assert_eq!(refusal.code(), "owner_unknown");
        assert!(mailbox.move_calls().is_empty());
    }

    #[test]
    fn task_3058_a_request_for_another_service_is_refused() {
        let mut mailbox = seeded_proton_mailbox();
        let mut request = proton_mail_marked_delete_request(SIGNED_IN, SENT, MARKED_MESSAGE);
        request.service_id = "gmail".to_owned();
        let refusal = delete_marked_proton_mail_message(&mut mailbox, &request)
            .expect_err("another service's request is not run against Proton");
        println!("TASK3058 wrong_service_refusal={}", refusal.code());
        assert_eq!(refusal.code(), "wrong_service");
        assert!(mailbox.move_calls().is_empty());
    }

    #[test]
    fn task_3058_a_folder_proton_does_not_have_is_refused() {
        let mut mailbox = seeded_proton_mailbox();
        let request =
            proton_mail_marked_delete_request(SIGNED_IN, "All Mail", "proton-3058-sent-001");
        let refusal = delete_marked_proton_mail_message(&mut mailbox, &request)
            .expect_err("a folder Proton does not have is not read");
        println!("TASK3058 unknown_folder_refusal={}", refusal.code());
        println!("TASK3058 unknown_folder_reason={}", refusal.reason());
        assert_eq!(refusal.code(), "folder_read_failed");
        assert!(mailbox.move_calls().is_empty());
    }

    #[test]
    fn task_3058_proton_names_its_own_trash_and_four_folders() {
        let mailbox = ProtonMailbox::new();
        println!("TASK3058 service_id={}", mailbox.service_id());
        println!("TASK3058 trash_folder_id={}", mailbox.trash_folder_id());
        println!("TASK3058 folders={}", mailbox.folder_ids().join(","));
        assert_eq!(mailbox.service_id(), "proton");
        assert_eq!(mailbox.trash_folder_id(), "Trash");
        assert_eq!(mailbox.folder_ids(), ["Inbox", "Sent", "Archive", "Trash"]);
    }

    #[test]
    fn task_3058_a_marked_message_already_in_trash_is_removed_without_a_move() {
        let mut mailbox = seeded_proton_mailbox().with_message(ProtonMailboxMessage::new(
            PROTON_MAIL_TRASH_FOLDER_ID,
            "proton-3058-already-trashed",
            "SCRUB-PR-DEL already in trash",
            SIGNED_IN,
        ));
        let request = proton_mail_marked_delete_request(
            SIGNED_IN,
            PROTON_MAIL_TRASH_FOLDER_ID,
            "proton-3058-already-trashed",
        );
        let receipt =
            delete_marked_proton_mail_message(&mut mailbox, &request).expect("remove from trash");
        println!(
            "TASK3058 already_in_trash_steps={}",
            receipt.step_names().join(",")
        );
        assert_eq!(
            receipt.step_names(),
            vec!["already_in_trash", "remove_one_message_from_trash"]
        );
        assert!(mailbox.move_calls().is_empty());
        assert_eq!(
            mailbox.message_ids_in(PROTON_MAIL_TRASH_FOLDER_ID),
            vec![UNRELATED_IN_TRASH.to_owned()]
        );
    }
}
