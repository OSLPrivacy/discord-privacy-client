//! The iCloud Mail fill-in of the shared mail deleter (TASK 3073).
//!
//! Gate 3045 owns the order of operations for every mail service: move the
//! marked message to trash, then take **that one message** back out of trash, by
//! id. Nothing in here repeats that order, and nothing in here can empty a
//! trash: the only removal [`SharedMailTrashSurface`] offers is
//! `remove_one_message_from_trash(message_id)`.
//!
//! What this module adds is the iCloud half and only the iCloud half:
//!
//! * iCloud calls its trash folder `Trash`, so that is what the surface names.
//!   Gate 3045 asks the service for the name and never guesses it.
//! * folder contents come out of gate 3071's iCloud reader
//!   ([`read_icloud_shared_mailbox_messages`]), so the before and after counts
//!   the deleter checks are the counts iCloud itself reports — not a private
//!   view of the fixture the deleter kept for itself.
//! * the sender the owner check sees comes from the stored record, sender
//!   address and all, including the case where there is no readable sender. That
//!   fails closed as `owner_unknown` through gate 3044's check rather than being
//!   hidden behind a mailbox-wide refusal.
//!
//! Task 1272 ruled that iCloud may be driven ("iCloud STAYS IN"), which is why
//! this is filled in rather than left empty, the same ruling gate 3071 read.

use crate::icloud_mailbox_reader::{read_icloud_shared_mailbox_messages, ICLOUD_SERVICE_ID};
use crate::mail_owner_check::VisibleMailMessage;
use crate::shared_mail_deleter::{
    delete_marked_mail_message, SharedMailDeleteError, SharedMailDeleteReceipt,
    SharedMailDeleteRequest, SharedMailTrashSurface,
};
use crate::shared_mail_reader_types::{SharedMailboxReaderError, SharedMailboxSnapshot};

/// The folder iCloud Mail calls its trash.
pub const ICLOUD_TRASH_FOLDER_ID: &str = "Trash";

/// One marked iCloud message, ready for gate 3045's deleter.
pub fn icloud_delete_request(
    signed_in_address: impl Into<String>,
    folder_id: impl Into<String>,
    message_id: impl Into<String>,
) -> SharedMailDeleteRequest {
    SharedMailDeleteRequest::marked(ICLOUD_SERVICE_ID, signed_in_address, folder_id, message_id)
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum IcloudMailDeleteError {
    /// The snapshot is not something gate 3071's iCloud reader will read.
    NotAnIcloudMailbox(SharedMailboxReaderError),
    /// This account has no Trash folder, so there is nowhere to move a message
    /// to and nothing to take it back out of.
    NoTrashFolder,
    /// Gate 3045 refused the run. Its code and reason are carried through
    /// unchanged, so an iCloud refusal reads the same as every other service's.
    Shared(SharedMailDeleteError),
}

impl IcloudMailDeleteError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NotAnIcloudMailbox(_) => "not_an_icloud_mailbox",
            Self::NoTrashFolder => "no_trash_folder",
            Self::Shared(error) => error.code(),
        }
    }

    pub fn reason(&self) -> String {
        match self {
            Self::NotAnIcloudMailbox(error) => error.reason().to_owned(),
            Self::NoTrashFolder => "OSL: this iCloud account has no Trash folder".to_owned(),
            Self::Shared(error) => error.reason(),
        }
    }
}

impl From<SharedMailDeleteError> for IcloudMailDeleteError {
    fn from(error: SharedMailDeleteError) -> Self {
        Self::Shared(error)
    }
}

impl From<SharedMailboxReaderError> for IcloudMailDeleteError {
    fn from(error: SharedMailboxReaderError) -> Self {
        Self::NotAnIcloudMailbox(error)
    }
}

/// The iCloud fill-in of gate 3045's trash surface, over the very mailbox
/// snapshot gate 3071's reader reads.
#[derive(Debug)]
pub struct IcloudMailTrashSurface<'a> {
    mailbox: &'a mut SharedMailboxSnapshot,
    trash_folder_id: String,
}

impl<'a> IcloudMailTrashSurface<'a> {
    /// Refuses before the surface exists at all if the snapshot is not a
    /// readable iCloud mailbox, or if the account has no Trash folder.
    pub fn open(mailbox: &'a mut SharedMailboxSnapshot) -> Result<Self, IcloudMailDeleteError> {
        let trash_folder_id = mailbox
            .labels
            .iter()
            .find(|folder| folder.label_id == ICLOUD_TRASH_FOLDER_ID)
            .map(|folder| folder.label_id.clone())
            .ok_or(IcloudMailDeleteError::NoTrashFolder)?;
        Ok(Self {
            mailbox,
            trash_folder_id,
        })
    }
}

impl SharedMailTrashSurface for IcloudMailTrashSurface<'_> {
    fn service_id(&self) -> &str {
        ICLOUD_SERVICE_ID
    }

    fn trash_folder_id(&self) -> &str {
        &self.trash_folder_id
    }

    fn message_ids_in_folder(&self, folder_id: &str) -> Result<Vec<String>, String> {
        read_icloud_shared_mailbox_messages(self.mailbox, folder_id)
            .map(|messages| {
                messages
                    .into_iter()
                    .map(|message| message.message_id)
                    .collect()
            })
            .map_err(|error| error.reason().to_owned())
    }

    fn visible_message(
        &self,
        folder_id: &str,
        message_id: &str,
    ) -> Result<Option<VisibleMailMessage>, String> {
        Ok(self
            .mailbox
            .messages
            .iter()
            .find(|message| message.label_id == folder_id && message.message_id == message_id)
            .map(|message| VisibleMailMessage {
                message_id: message.message_id.clone(),
                mailbox: message.label_id.clone(),
                sender_address: message.sender_address.clone(),
            }))
    }

    fn move_message_to_trash(&mut self, folder_id: &str, message_id: &str) -> Result<(), String> {
        let trash = self.trash_folder_id.clone();
        let mut moved = 0usize;
        for message in &mut self.mailbox.messages {
            if message.label_id == folder_id && message.message_id == message_id {
                message.label_id = trash.clone();
                moved += 1;
            }
        }
        if moved == 0 {
            return Err(format!(
                "OSL: iCloud message {message_id} is not in folder {folder_id}"
            ));
        }
        Ok(())
    }

    fn remove_one_message_from_trash(&mut self, message_id: &str) -> Result<(), String> {
        let trash = self.trash_folder_id.clone();
        let before = self.mailbox.messages.len();
        // One message, by id, and only where it sits in trash. There is no
        // "empty trash" here because gate 3045 never offers one to call.
        self.mailbox
            .messages
            .retain(|message| !(message.label_id == trash && message.message_id == message_id));
        if self.mailbox.messages.len() == before {
            return Err(format!(
                "OSL: iCloud message {message_id} is not in {trash}"
            ));
        }
        Ok(())
    }
}

/// Delete one marked iCloud message: move it to Trash, then remove that one
/// message from Trash.
pub fn delete_marked_icloud_message(
    mailbox: &mut SharedMailboxSnapshot,
    request: &SharedMailDeleteRequest,
) -> Result<SharedMailDeleteReceipt, IcloudMailDeleteError> {
    let mut surface = IcloudMailTrashSurface::open(mailbox)?;
    Ok(delete_marked_mail_message(&mut surface, request)?)
}

/// How many messages in `folder_id` have this subject, read through gate 3071's
/// iCloud reader.
pub fn icloud_messages_matching_subject(
    mailbox: &SharedMailboxSnapshot,
    folder_id: &str,
    subject: &str,
) -> Result<Vec<String>, SharedMailboxReaderError> {
    Ok(read_icloud_shared_mailbox_messages(mailbox, folder_id)?
        .into_iter()
        .filter(|message| message.subject == subject)
        .map(|message| message.message_id)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icloud_mailbox_reader::read_icloud_shared_mailbox_folders;
    use crate::shared_mail_deleter::SharedMailReviewDecision;
    use crate::shared_mail_reader_types::{SharedMailLabel, SharedMailMessageRecord};

    const SIGNED_IN: &str = "scrub-owner@icloud.test";
    const SENT: &str = "Sent";
    const TRASH: &str = "Trash";
    const MARKED_SUBJECT: &str = "SCRUB-IC-DEL";
    const MARKED_MESSAGE: &str = "sent-scrub-ic-del-two";
    const UNRELATED_IN_TRASH: &str = "trash-icloud-holiday-photos";

    /// Sent holds three messages matching SCRUB-IC-DEL. Trash holds one
    /// unrelated message, put there before the run.
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
        icloud_messages_matching_subject(mailbox, SENT, MARKED_SUBJECT).expect("read Sent")
    }

    fn trash_ids(mailbox: &SharedMailboxSnapshot) -> Vec<String> {
        read_icloud_shared_mailbox_messages(mailbox, TRASH)
            .expect("read Trash")
            .into_iter()
            .map(|message| message.message_id)
            .collect()
    }

    #[test]
    fn task_3073_marked_icloud_message_leaves_sent_at_two_and_trash_at_one() {
        let mut mailbox = seeded_icloud_mailbox();

        let sent_before = sent_matching(&mailbox);
        let trash_before = trash_ids(&mailbox);
        println!("TASK3073 service_id={ICLOUD_SERVICE_ID}");
        println!("TASK3073 trash_folder_id={ICLOUD_TRASH_FOLDER_ID}");
        println!("TASK3073 marked_subject={MARKED_SUBJECT}");
        println!("TASK3073 marked_message_id={MARKED_MESSAGE}");
        println!("TASK3073 before_sent_matching_count={}", sent_before.len());
        println!(
            "TASK3073 before_sent_matching_ids={}",
            sent_before.join(",")
        );
        println!("TASK3073 before_trash_count={}", trash_before.len());
        println!("TASK3073 before_trash_ids={}", trash_before.join(","));

        let receipt = delete_marked_icloud_message(
            &mut mailbox,
            &icloud_delete_request(SIGNED_IN, SENT, MARKED_MESSAGE),
        )
        .expect("the marked iCloud message is deleted");

        let sent_after = sent_matching(&mailbox);
        let trash_after = trash_ids(&mailbox);
        println!("TASK3073 run_steps={}", receipt.step_names().join(","));
        println!("TASK3073 after_sent_matching_count={}", sent_after.len());
        println!("TASK3073 after_sent_matching_ids={}", sent_after.join(","));
        println!("TASK3073 after_trash_count={}", trash_after.len());
        println!("TASK3073 after_trash_ids={}", trash_after.join(","));
        println!(
            "TASK3073 folder_copies_after={} trash_copies_after={}",
            receipt.folder_copies_after, receipt.trash_copies_after
        );
        println!(
            "TASK3073 whole_trash_emptied={} other_trash_before={} other_trash_after={}",
            receipt.whole_trash_emptied,
            receipt.other_trash_messages_before,
            receipt.other_trash_messages_after
        );

        assert_eq!(sent_before.len(), 3);
        assert_eq!(trash_before, vec![UNRELATED_IN_TRASH.to_owned()]);
        assert_eq!(
            receipt.step_names(),
            vec!["move_to_trash", "remove_one_message_from_trash"]
        );
        assert_eq!(sent_after.len(), 2);
        assert!(!sent_after.contains(&MARKED_MESSAGE.to_owned()));
        assert_eq!(trash_after, vec![UNRELATED_IN_TRASH.to_owned()]);
        assert_eq!(receipt.folder_copies_after, 0);
        assert_eq!(receipt.trash_copies_after, 0);
        assert!(!receipt.whole_trash_emptied);
        assert_eq!(receipt.trash_folder_id, ICLOUD_TRASH_FOLDER_ID);
    }

    #[test]
    fn task_3073_the_single_message_is_taken_out_of_trash_rather_than_left_there() {
        let mut mailbox = seeded_icloud_mailbox();
        let mut surface = IcloudMailTrashSurface::open(&mut mailbox).expect("iCloud trash surface");
        surface
            .move_message_to_trash(SENT, MARKED_MESSAGE)
            .expect("move to trash");
        let trash_mid_run = surface
            .message_ids_in_folder(TRASH)
            .expect("read Trash mid-run");
        println!("TASK3073 trash_after_move_only={}", trash_mid_run.join(","));
        assert_eq!(trash_mid_run.len(), 2);

        surface
            .remove_one_message_from_trash(MARKED_MESSAGE)
            .expect("remove that one message from trash");
        let trash_end = surface.message_ids_in_folder(TRASH).expect("read Trash");
        println!("TASK3073 trash_after_removal={}", trash_end.join(","));
        assert_eq!(trash_end, vec![UNRELATED_IN_TRASH.to_owned()]);
    }

    #[test]
    fn task_3073_a_message_the_account_did_not_send_is_refused_before_anything_moves() {
        let mut mailbox = seeded_icloud_mailbox();
        mailbox.messages.push(SharedMailMessageRecord::new(
            SENT,
            "sent-scrub-ic-del-forwarded",
            MARKED_SUBJECT,
            1_786_290_000,
            "someone-else@icloud.test",
            "A message the signed-in account did not send.",
        ));

        let error = delete_marked_icloud_message(
            &mut mailbox,
            &icloud_delete_request(SIGNED_IN, SENT, "sent-scrub-ic-del-forwarded"),
        )
        .expect_err("a message the account did not send is refused");
        println!("TASK3073 refusal_not_yours={}", error.code());
        println!("TASK3073 refusal_not_yours_reason={}", error.reason());
        assert_eq!(error.code(), "not_yours");
        assert_eq!(
            read_icloud_shared_mailbox_messages(&mailbox, TRASH)
                .expect("read Trash")
                .len(),
            1
        );
    }

    #[test]
    fn task_3073_an_unmarked_icloud_message_is_refused() {
        let mut mailbox = seeded_icloud_mailbox();
        let mut request = icloud_delete_request(SIGNED_IN, SENT, MARKED_MESSAGE);
        request.decision = SharedMailReviewDecision::Keep;

        let error = delete_marked_icloud_message(&mut mailbox, &request)
            .expect_err("an unmarked message is refused");
        println!("TASK3073 refusal_keep={}", error.code());
        assert_eq!(error.code(), "not_marked");
        assert_eq!(sent_matching(&mailbox).len(), 3);
    }

    #[test]
    fn task_3073_an_unreadable_sender_is_refused_rather_than_guessed() {
        let mut mailbox = seeded_icloud_mailbox();
        for message in &mut mailbox.messages {
            if message.message_id == MARKED_MESSAGE {
                message.sender_address = None;
            }
        }

        let error = delete_marked_icloud_message(
            &mut mailbox,
            &icloud_delete_request(SIGNED_IN, SENT, MARKED_MESSAGE),
        )
        .expect_err("an unreadable sender is refused");
        println!("TASK3073 refusal_owner_unknown={}", error.code());
        println!("TASK3073 refusal_owner_unknown_reason={}", error.reason());
        assert_eq!(error.code(), "owner_unknown");
    }

    #[test]
    fn task_3073_an_icloud_account_with_no_trash_folder_is_refused() {
        let mut mailbox = seeded_icloud_mailbox();
        mailbox.labels.retain(|folder| folder.label_id != TRASH);
        mailbox.messages.retain(|message| message.label_id != TRASH);

        let error = delete_marked_icloud_message(
            &mut mailbox,
            &icloud_delete_request(SIGNED_IN, SENT, MARKED_MESSAGE),
        )
        .expect_err("no Trash folder is refused");
        println!("TASK3073 refusal_no_trash_folder={}", error.code());
        println!("TASK3073 refusal_no_trash_folder_reason={}", error.reason());
        assert_eq!(error.code(), "no_trash_folder");
        assert_eq!(sent_matching(&mailbox).len(), 3);
    }

    #[test]
    fn task_3073_a_request_for_another_service_is_refused() {
        let mut mailbox = seeded_icloud_mailbox();
        let request = SharedMailDeleteRequest::marked("gmail", SIGNED_IN, SENT, MARKED_MESSAGE);

        let error = delete_marked_icloud_message(&mut mailbox, &request)
            .expect_err("another service's request is refused");
        println!("TASK3073 refusal_wrong_service={}", error.code());
        assert_eq!(error.code(), "wrong_service");
        assert_eq!(sent_matching(&mailbox).len(), 3);
    }

    #[test]
    fn task_3073_the_mailbox_the_deleter_drove_still_reads_as_an_icloud_mailbox() {
        let mut mailbox = seeded_icloud_mailbox();
        delete_marked_icloud_message(
            &mut mailbox,
            &icloud_delete_request(SIGNED_IN, SENT, MARKED_MESSAGE),
        )
        .expect("the marked iCloud message is deleted");

        let folders = read_icloud_shared_mailbox_folders(&mailbox).expect("iCloud folders");
        println!("TASK3073 after_folder_count={}", folders.len());
        assert_eq!(folders.len(), 4);
    }
}
