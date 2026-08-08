//! AOL Mail fill-in for the shared mail deleter (TASK 3064).
//!
//! The removal order is not written here. It is gate 3045's shared deleter,
//! [`delete_marked_mail_message`], which moves the marked message to trash and
//! then takes **that one message** back out of trash, by id. What this module
//! supplies is the part that is AOL's and nobody else's:
//!
//! * AOL's service id, `aol`;
//! * the folder AOL calls its trash — `Trash` (gate 3062 read exactly four
//!   folders off a seeded AOL mailbox: Inbox, Sent, Archive, Trash);
//! * how an AOL folder is listed, how an AOL message is moved to Trash, and how
//!   one message — one, by id — is permanently removed from Trash.
//!
//! AOL Mail is reached over IMAP, the way the rest of this tree already reaches
//! it (`scrub_imap::seeded_aol_mailbox_for_scrub_paging`), and that makes AOL's
//! two halves different from a label service's:
//!
//! * a message lives **in** a folder, so "move to Trash" is a real move — the
//!   copy in Sent is gone and one copy is in Trash, not one message wearing a
//!   second label;
//! * permanent removal is IMAP's two beats — flag that one message `\Deleted`,
//!   then expunge — and an expunge is folder-wide by nature. That is the hazard
//!   this fill-in has to answer for, so [`AolMailbox::remove_one_message_from_trash`]
//!   answers for it twice over: it **refuses to flag a message that is not in
//!   Trash**, so the second half cannot reach past the trash into a live folder,
//!   and it **refuses to expunge at all while any other message in Trash is
//!   already flagged `\Deleted`**, so the expunge cannot carry a bystander out
//!   with it. Only the folder it was asked for is ever expunged.
//!
//! There is no empty-trash call here, because [`SharedMailTrashSurface`] has no
//! empty-trash call to fill in. An AOL account's Trash holds mail the person put
//! there themselves; it is not this command's to destroy.

use crate::mail_owner_check::VisibleMailMessage;
use crate::shared_mail_deleter::{
    delete_marked_mail_message, SharedMailDeleteError, SharedMailDeleteReceipt,
    SharedMailDeleteRequest, SharedMailReviewDecision, SharedMailTrashSurface,
};

/// AOL Mail's service id — the same `aol` that
/// `service_connections::AOL_SERVICE_ID` names for AOL's mail-website controls.
/// It is stated here rather than imported from there so this fill-in does not
/// drag that file's website plumbing in behind it; the check
/// `task_3064_aol_names_its_own_service_trash_and_four_folders` pins the string.
pub const AOL_MAIL_SERVICE_ID: &str = "aol";

/// What AOL calls its trash. Named, not guessed: gate 3062's seeded AOL mailbox
/// lists this folder alongside Inbox, Sent and Archive.
pub const AOL_MAIL_TRASH_FOLDER_ID: &str = "Trash";

/// The four folders an AOL mailbox reads back, in the order gate 3062 read them.
pub const AOL_MAIL_FOLDER_IDS: [&str; 4] = ["Inbox", "Sent", "Archive", "Trash"];

/// One message sitting in one AOL folder, with its IMAP `\Deleted` flag.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AolMailboxMessage {
    pub folder_id: String,
    pub message_id: String,
    pub subject: String,
    pub sender_address: Option<String>,
    pub flagged_deleted: bool,
}

impl AolMailboxMessage {
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
            flagged_deleted: false,
        }
    }

    /// A message whose sender address cannot be read. Gate 3044's rule is that
    /// this fails closed rather than being guessed, so the fixture can say so.
    pub fn with_unreadable_sender(mut self) -> Self {
        self.sender_address = None;
        self
    }

    /// A message some other client already flagged `\Deleted`. A folder-wide
    /// expunge would carry this one out too, which is exactly what this fill-in
    /// refuses to let happen.
    pub fn already_flagged_deleted(mut self) -> Self {
        self.flagged_deleted = true;
        self
    }
}

/// An AOL mailbox: its folders and the messages filed in them.
///
/// Counts are answered by walking the messages, so anything read back out of
/// this is what the mailbox holds now, not what a caller remembered.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AolMailbox {
    folder_ids: Vec<String>,
    messages: Vec<AolMailboxMessage>,
    move_calls: Vec<String>,
    flag_deleted_calls: Vec<String>,
    expunge_calls: Vec<String>,
}

impl Default for AolMailbox {
    fn default() -> Self {
        Self::new()
    }
}

impl AolMailbox {
    /// An empty AOL mailbox with AOL's four folders.
    pub fn new() -> Self {
        Self {
            folder_ids: AOL_MAIL_FOLDER_IDS
                .iter()
                .map(|folder| (*folder).to_owned())
                .collect(),
            messages: Vec::new(),
            move_calls: Vec::new(),
            flag_deleted_calls: Vec::new(),
            expunge_calls: Vec::new(),
        }
    }

    pub fn with_message(mut self, message: AolMailboxMessage) -> Self {
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

    /// The messages in one folder currently flagged `\Deleted` — what an expunge
    /// of that folder would take.
    pub fn flagged_deleted_in(&self, folder_id: &str) -> Vec<String> {
        self.messages
            .iter()
            .filter(|message| message.folder_id == folder_id && message.flagged_deleted)
            .map(|message| message.message_id.clone())
            .collect()
    }

    /// Every move-to-Trash AOL was asked for, as `folder/message-id`.
    pub fn move_calls(&self) -> &[String] {
        &self.move_calls
    }

    /// Every `\Deleted` flag AOL was asked for, by message id. One run leaves
    /// exactly one entry here.
    pub fn flag_deleted_calls(&self) -> &[String] {
        &self.flag_deleted_calls
    }

    /// Every expunge AOL was asked for, by folder id. One run leaves exactly one
    /// entry here, and it is Trash.
    pub fn expunge_calls(&self) -> &[String] {
        &self.expunge_calls
    }

    pub fn total_message_count(&self) -> usize {
        self.messages.len()
    }
}

impl SharedMailTrashSurface for AolMailbox {
    fn service_id(&self) -> &str {
        AOL_MAIL_SERVICE_ID
    }

    fn trash_folder_id(&self) -> &str {
        AOL_MAIL_TRASH_FOLDER_ID
    }

    fn message_ids_in_folder(&self, folder_id: &str) -> Result<Vec<String>, String> {
        if !self.has_folder(folder_id) {
            return Err(format!("AOL Mail has no folder named {folder_id}"));
        }
        Ok(self.message_ids_in(folder_id))
    }

    fn visible_message(
        &self,
        folder_id: &str,
        message_id: &str,
    ) -> Result<Option<VisibleMailMessage>, String> {
        if !self.has_folder(folder_id) {
            return Err(format!("AOL Mail has no folder named {folder_id}"));
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
            return Err(format!("AOL Mail has no folder named {folder_id}"));
        }
        let mut moved = false;
        for message in &mut self.messages {
            if message.folder_id == folder_id && message.message_id == message_id {
                // AOL files a message in a folder, so the move is a real move:
                // the copy that was in the source folder is gone and one copy is
                // now in Trash. A moved message arrives unflagged.
                message.folder_id = AOL_MAIL_TRASH_FOLDER_ID.to_owned();
                message.flagged_deleted = false;
                moved = true;
            }
        }
        if moved {
            Ok(())
        } else {
            Err(format!(
                "AOL Mail has no message {message_id} in {folder_id}"
            ))
        }
    }

    fn remove_one_message_from_trash(&mut self, message_id: &str) -> Result<(), String> {
        self.flag_deleted_calls.push(message_id.to_owned());

        if self.count_of_message_in(AOL_MAIL_TRASH_FOLDER_ID, message_id) == 0 {
            // AOL flags and expunges out of Trash and nowhere else, so this half
            // of the run cannot reach past the trash into a live folder.
            return Err(format!(
                "AOL Mail message {message_id} is not in {AOL_MAIL_TRASH_FOLDER_ID}"
            ));
        }

        let bystanders: Vec<String> = self
            .messages
            .iter()
            .filter(|message| {
                message.folder_id == AOL_MAIL_TRASH_FOLDER_ID
                    && message.message_id != message_id
                    && message.flagged_deleted
            })
            .map(|message| message.message_id.clone())
            .collect();
        if !bystanders.is_empty() {
            // An IMAP expunge takes every flagged message in the folder. If
            // something else in Trash is already flagged, there is no expunge
            // that removes only ours, so nothing is removed at all.
            return Err(format!(
                "an expunge of {AOL_MAIL_TRASH_FOLDER_ID} would also remove [{}], which this command never does",
                bystanders.join(",")
            ));
        }

        for message in &mut self.messages {
            if message.folder_id == AOL_MAIL_TRASH_FOLDER_ID && message.message_id == message_id {
                message.flagged_deleted = true;
            }
        }
        self.expunge_calls.push(AOL_MAIL_TRASH_FOLDER_ID.to_owned());
        // The expunge is of Trash only, and takes only what is flagged there —
        // which the check above has just established is our one message.
        self.messages.retain(|message| {
            !(message.folder_id == AOL_MAIL_TRASH_FOLDER_ID && message.flagged_deleted)
        });
        Ok(())
    }
}

/// One delete request against AOL Mail, carrying the review's own two facts:
/// whether the review reached this row at all, and what it decided.
pub fn aol_mail_delete_request(
    signed_in_address: &str,
    folder_id: &str,
    message_id: &str,
    reviewed: bool,
    decision: SharedMailReviewDecision,
) -> SharedMailDeleteRequest {
    SharedMailDeleteRequest {
        service_id: AOL_MAIL_SERVICE_ID.to_owned(),
        signed_in_address: signed_in_address.to_owned(),
        folder_id: folder_id.to_owned(),
        message_id: message_id.to_owned(),
        reviewed,
        decision,
    }
}

/// A request for a row the review reached and marked for deletion.
pub fn aol_mail_marked_delete_request(
    signed_in_address: &str,
    folder_id: &str,
    message_id: &str,
) -> SharedMailDeleteRequest {
    aol_mail_delete_request(
        signed_in_address,
        folder_id,
        message_id,
        true,
        SharedMailReviewDecision::MarkedForDeletion,
    )
}

/// Run the shared mail deleter against an AOL mailbox.
///
/// Every refusal is the shared deleter's, taken before the first action, so a
/// refused request never reaches AOL at all.
pub fn delete_marked_aol_mail_message(
    mailbox: &mut AolMailbox,
    request: &SharedMailDeleteRequest,
) -> Result<SharedMailDeleteReceipt, SharedMailDeleteError> {
    delete_marked_mail_message(mailbox, request)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGNED_IN: &str = "scrub.owner@aol.example.test";
    const SENT: &str = "Sent";
    const INBOX: &str = "Inbox";
    const MARKER: &str = "SCRUB-AO-DEL";
    const MARKED_MESSAGE: &str = "aol-3064-sent-002";
    const UNRELATED_IN_TRASH: &str = "aol-3064-trash-was-already-here";

    /// The seeded AOL mailbox this task's finish line names: Sent holds three
    /// messages matching SCRUB-AO-DEL, Trash holds one unrelated message. The
    /// signed-in address and the folder names are gate 3062's seeded AOL
    /// mailbox.
    fn seeded_aol_mailbox() -> AolMailbox {
        AolMailbox::new()
            .with_message(AolMailboxMessage::new(
                SENT,
                "aol-3064-sent-001",
                "SCRUB-AO-DEL export request",
                SIGNED_IN,
            ))
            .with_message(AolMailboxMessage::new(
                SENT,
                MARKED_MESSAGE,
                "SCRUB-AO-DEL erasure demand",
                SIGNED_IN,
            ))
            .with_message(AolMailboxMessage::new(
                SENT,
                "aol-3064-sent-003",
                "SCRUB-AO-DEL confirmation note",
                SIGNED_IN,
            ))
            .with_message(AolMailboxMessage::new(
                AOL_MAIL_TRASH_FOLDER_ID,
                UNRELATED_IN_TRASH,
                "Holiday photos from last summer",
                SIGNED_IN,
            ))
    }

    #[test]
    fn task_3064_deleting_one_marked_sent_message_leaves_two_and_keeps_the_unrelated_trash_message()
    {
        let mut mailbox = seeded_aol_mailbox();

        let sent_matching_before = mailbox.count_matching_in(SENT, MARKER);
        let trash_before = mailbox.message_ids_in(AOL_MAIL_TRASH_FOLDER_ID);
        println!("TASK3064 before_sent_matching_{MARKER}={sent_matching_before}");
        println!("TASK3064 before_trash_count={}", trash_before.len());
        println!("TASK3064 before_trash_ids={}", trash_before.join(","));

        let request = aol_mail_marked_delete_request(SIGNED_IN, SENT, MARKED_MESSAGE);
        let receipt = delete_marked_aol_mail_message(&mut mailbox, &request)
            .expect("delete one marked AOL message");

        let sent_matching_after = mailbox.count_matching_in(SENT, MARKER);
        let trash_after = mailbox.message_ids_in(AOL_MAIL_TRASH_FOLDER_ID);
        println!("TASK3064 steps={}", receipt.step_names().join(","));
        println!("TASK3064 after_sent_matching_{MARKER}={sent_matching_after}");
        println!("TASK3064 after_trash_count={}", trash_after.len());
        println!("TASK3064 after_trash_ids={}", trash_after.join(","));

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
        // The expunge was of Trash, and no other folder lost anything.
        assert_eq!(
            mailbox.expunge_calls(),
            [AOL_MAIL_TRASH_FOLDER_ID.to_owned()]
        );
        assert_eq!(mailbox.count_in(INBOX), 0);
        assert_eq!(mailbox.count_in(SENT), 2);
    }

    #[test]
    fn task_3064_the_single_message_is_flagged_and_expunged_by_id() {
        let mut mailbox = seeded_aol_mailbox();
        let request = aol_mail_marked_delete_request(SIGNED_IN, SENT, MARKED_MESSAGE);
        delete_marked_aol_mail_message(&mut mailbox, &request).expect("delete one message");

        println!("TASK3064 move_calls={}", mailbox.move_calls().join(","));
        println!(
            "TASK3064 flag_deleted_calls={}",
            mailbox.flag_deleted_calls().join(",")
        );
        println!(
            "TASK3064 expunge_calls={}",
            mailbox.expunge_calls().join(",")
        );
        assert_eq!(mailbox.move_calls(), [format!("{SENT}/{MARKED_MESSAGE}")]);
        assert_eq!(mailbox.flag_deleted_calls(), [MARKED_MESSAGE.to_owned()]);
        assert_eq!(
            mailbox.expunge_calls(),
            [AOL_MAIL_TRASH_FOLDER_ID.to_owned()]
        );
        // One message left the mailbox, and only one.
        assert_eq!(mailbox.total_message_count(), 3);
        assert!(mailbox
            .flagged_deleted_in(AOL_MAIL_TRASH_FOLDER_ID)
            .is_empty());
    }

    #[test]
    fn task_3064_aol_refuses_to_flag_a_message_that_is_not_in_trash() {
        let mut mailbox = seeded_aol_mailbox();
        let refusal = mailbox
            .remove_one_message_from_trash(MARKED_MESSAGE)
            .expect_err("AOL must not flag and expunge outside Trash");
        println!("TASK3064 flag_outside_trash_refusal={refusal}");
        assert!(refusal.contains("is not in Trash"));
        assert_eq!(mailbox.count_matching_in(SENT, MARKER), 3);
        assert!(mailbox.expunge_calls().is_empty());
    }

    #[test]
    fn task_3064_an_expunge_that_would_carry_another_trash_message_out_is_refused() {
        let mut mailbox = seeded_aol_mailbox().with_message(
            AolMailboxMessage::new(
                AOL_MAIL_TRASH_FOLDER_ID,
                "aol-3064-someone-else-flagged-this",
                "Flagged by another client",
                SIGNED_IN,
            )
            .already_flagged_deleted(),
        );
        let request = aol_mail_marked_delete_request(SIGNED_IN, SENT, MARKED_MESSAGE);

        let refusal = delete_marked_aol_mail_message(&mut mailbox, &request)
            .expect_err("an expunge that takes a bystander is never this command's outcome");

        println!("TASK3064 bystander_refusal_code={}", refusal.code());
        println!("TASK3064 bystander_refusal_reason={}", refusal.reason());
        println!(
            "TASK3064 bystander_trash_after={}",
            mailbox.message_ids_in(AOL_MAIL_TRASH_FOLDER_ID).join(",")
        );
        assert_eq!(refusal.code(), "remove_from_trash_failed");
        assert!(refusal
            .reason()
            .contains("aol-3064-someone-else-flagged-this"));
        assert!(mailbox.expunge_calls().is_empty());
        // Nothing was expunged: both Trash messages are still there, and so is
        // the marked one the move put there.
        assert_eq!(
            mailbox.count_of_message_in(
                AOL_MAIL_TRASH_FOLDER_ID,
                "aol-3064-someone-else-flagged-this"
            ),
            1
        );
        assert_eq!(
            mailbox.count_of_message_in(AOL_MAIL_TRASH_FOLDER_ID, UNRELATED_IN_TRASH),
            1
        );
    }

    #[test]
    fn task_3064_an_unmarked_message_is_refused_before_anything_moves() {
        let mut mailbox = seeded_aol_mailbox();
        let keep = aol_mail_delete_request(
            SIGNED_IN,
            SENT,
            MARKED_MESSAGE,
            true,
            SharedMailReviewDecision::Keep,
        );
        let unreviewed = aol_mail_delete_request(
            SIGNED_IN,
            SENT,
            MARKED_MESSAGE,
            false,
            SharedMailReviewDecision::MarkedForDeletion,
        );

        let keep_refusal = delete_marked_aol_mail_message(&mut mailbox, &keep)
            .expect_err("a kept message is not deleted");
        let unreviewed_refusal = delete_marked_aol_mail_message(&mut mailbox, &unreviewed)
            .expect_err("a row the review never reached is not deleted");

        println!("TASK3064 keep_refusal={}", keep_refusal.code());
        println!("TASK3064 unreviewed_refusal={}", unreviewed_refusal.code());
        assert_eq!(keep_refusal.code(), "not_marked");
        assert_eq!(unreviewed_refusal.code(), "not_marked");
        assert!(mailbox.move_calls().is_empty());
        assert!(mailbox.flag_deleted_calls().is_empty());
        assert!(mailbox.expunge_calls().is_empty());
        assert_eq!(mailbox.count_matching_in(SENT, MARKER), 3);
    }

    #[test]
    fn task_3064_a_message_another_address_sent_is_refused() {
        let mut mailbox = AolMailbox::new().with_message(AolMailboxMessage::new(
            SENT,
            "aol-3064-not-ours",
            "SCRUB-AO-DEL forwarded by someone else",
            "someone.else@aol.example.test",
        ));
        let request = aol_mail_marked_delete_request(SIGNED_IN, SENT, "aol-3064-not-ours");
        let refusal = delete_marked_aol_mail_message(&mut mailbox, &request)
            .expect_err("another address's message is not deleted");
        println!("TASK3064 not_yours_refusal={}", refusal.code());
        assert_eq!(refusal.code(), "not_yours");
        assert!(mailbox.move_calls().is_empty());
        assert!(mailbox.expunge_calls().is_empty());
    }

    #[test]
    fn task_3064_an_unreadable_sender_is_refused_rather_than_guessed() {
        let mut mailbox = AolMailbox::new().with_message(
            AolMailboxMessage::new(
                SENT,
                "aol-3064-no-sender",
                "SCRUB-AO-DEL sender missing",
                SIGNED_IN,
            )
            .with_unreadable_sender(),
        );
        let request = aol_mail_marked_delete_request(SIGNED_IN, SENT, "aol-3064-no-sender");
        let refusal = delete_marked_aol_mail_message(&mut mailbox, &request)
            .expect_err("an unreadable sender is not guessed");
        println!("TASK3064 owner_unknown_refusal={}", refusal.code());
        assert_eq!(refusal.code(), "owner_unknown");
        assert!(mailbox.move_calls().is_empty());
    }

    #[test]
    fn task_3064_a_request_for_another_service_is_refused() {
        let mut mailbox = seeded_aol_mailbox();
        let mut request = aol_mail_marked_delete_request(SIGNED_IN, SENT, MARKED_MESSAGE);
        request.service_id = "yahoo".to_owned();
        let refusal = delete_marked_aol_mail_message(&mut mailbox, &request)
            .expect_err("another service's request is not run against AOL");
        println!("TASK3064 wrong_service_refusal={}", refusal.code());
        assert_eq!(refusal.code(), "wrong_service");
        assert!(mailbox.move_calls().is_empty());
    }

    #[test]
    fn task_3064_a_folder_aol_does_not_have_is_refused() {
        let mut mailbox = seeded_aol_mailbox();
        let request = aol_mail_marked_delete_request(SIGNED_IN, "All Mail", "aol-3064-sent-001");
        let refusal = delete_marked_aol_mail_message(&mut mailbox, &request)
            .expect_err("a folder AOL does not have is not read");
        println!("TASK3064 unknown_folder_refusal={}", refusal.code());
        println!("TASK3064 unknown_folder_reason={}", refusal.reason());
        assert_eq!(refusal.code(), "folder_read_failed");
        assert!(mailbox.move_calls().is_empty());
    }

    #[test]
    fn task_3064_aol_names_its_own_service_trash_and_four_folders() {
        let mailbox = AolMailbox::new();
        println!("TASK3064 service_id={}", mailbox.service_id());
        println!("TASK3064 trash_folder_id={}", mailbox.trash_folder_id());
        println!("TASK3064 folders={}", mailbox.folder_ids().join(","));
        assert_eq!(mailbox.service_id(), "aol");
        assert_eq!(AOL_MAIL_SERVICE_ID, "aol");
        assert_eq!(mailbox.trash_folder_id(), "Trash");
        assert_eq!(mailbox.folder_ids(), ["Inbox", "Sent", "Archive", "Trash"]);
    }

    #[test]
    fn task_3064_a_marked_message_already_in_trash_is_removed_without_a_move() {
        let mut mailbox = seeded_aol_mailbox().with_message(AolMailboxMessage::new(
            AOL_MAIL_TRASH_FOLDER_ID,
            "aol-3064-already-trashed",
            "SCRUB-AO-DEL already in trash",
            SIGNED_IN,
        ));
        let request = aol_mail_marked_delete_request(
            SIGNED_IN,
            AOL_MAIL_TRASH_FOLDER_ID,
            "aol-3064-already-trashed",
        );
        let receipt =
            delete_marked_aol_mail_message(&mut mailbox, &request).expect("remove from trash");
        println!(
            "TASK3064 already_in_trash_steps={}",
            receipt.step_names().join(",")
        );
        assert_eq!(
            receipt.step_names(),
            vec!["already_in_trash", "remove_one_message_from_trash"]
        );
        assert!(mailbox.move_calls().is_empty());
        assert_eq!(
            mailbox.message_ids_in(AOL_MAIL_TRASH_FOLDER_ID),
            vec![UNRELATED_IN_TRASH.to_owned()]
        );
    }
}
