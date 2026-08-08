//! The shared mail deleter (TASK 3045).
//!
//! Mail services do not remove a message where it sits: the readable action is
//! "move to trash", and the message is only actually gone once that one copy is
//! taken out of trash again. This module owns both halves for every mail
//! service, so no adapter writes its own removal order:
//!
//! 1. move the marked message from its folder into the trash folder, then
//! 2. remove **that one message** from trash, by id.
//!
//! Step 2 is deliberately not "empty trash". There is no empty-trash call in
//! [`SharedMailTrashSurface`] for a service fill-in to reach for, and
//! [`delete_marked_mail_message`] re-reads the trash afterwards and refuses with
//! [`SharedMailDeleteError::WholeTrashEmptied`] if anything that was sitting in
//! trash before the run is missing after it. A message the user put in trash
//! last week is not this command's to destroy.
//!
//! Authority comes from exactly two facts, and nothing else:
//!
//! * the review decision — [`SharedMailReviewDecision::MarkedForDeletion`] on a
//!   row the review actually reached (`reviewed`), and
//! * the shared mail owner check from TASK 3044,
//!   [`mail_message_is_owned_by_signed_in_address`].

use std::fmt;

use crate::mail_owner_check::{
    mail_message_is_owned_by_signed_in_address, MailOwnerCheckError, VisibleMailMessage,
};

/// What the review said about one mail row. Same two-word vocabulary the marked
/// deletion review uses; `reviewed` is carried separately, so a row that carries
/// a mark the review never reached is not treated as marked.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SharedMailReviewDecision {
    Keep,
    MarkedForDeletion,
}

impl SharedMailReviewDecision {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::MarkedForDeletion => "marked_for_deletion",
        }
    }
}

/// One direct run of the shared mail deleter.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SharedMailDeleteRequest {
    pub service_id: String,
    pub signed_in_address: String,
    pub folder_id: String,
    pub message_id: String,
    pub reviewed: bool,
    pub decision: SharedMailReviewDecision,
}

impl SharedMailDeleteRequest {
    pub fn marked(
        service_id: impl Into<String>,
        signed_in_address: impl Into<String>,
        folder_id: impl Into<String>,
        message_id: impl Into<String>,
    ) -> Self {
        Self {
            service_id: service_id.into(),
            signed_in_address: signed_in_address.into(),
            folder_id: folder_id.into(),
            message_id: message_id.into(),
            reviewed: true,
            decision: SharedMailReviewDecision::MarkedForDeletion,
        }
    }
}

/// The two readable actions the deleter is allowed to take. There is no third
/// one, and in particular there is no "empty trash".
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SharedMailDeleteStep {
    MovedToTrash,
    AlreadyInTrash,
    RemovedOneMessageFromTrash,
}

impl SharedMailDeleteStep {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::MovedToTrash => "move_to_trash",
            Self::AlreadyInTrash => "already_in_trash",
            Self::RemovedOneMessageFromTrash => "remove_one_message_from_trash",
        }
    }
}

/// What the run did, read back from the service rather than remembered.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SharedMailDeleteReceipt {
    pub service_id: String,
    pub folder_id: String,
    pub message_id: String,
    pub trash_folder_id: String,
    pub steps: Vec<SharedMailDeleteStep>,
    pub folder_copies_after: usize,
    pub trash_copies_after: usize,
    pub trash_ids_before: Vec<String>,
    pub trash_ids_after: Vec<String>,
    pub other_trash_messages_before: usize,
    pub other_trash_messages_after: usize,
    pub whole_trash_emptied: bool,
}

impl SharedMailDeleteReceipt {
    pub fn step_names(&self) -> Vec<&'static str> {
        self.steps
            .iter()
            .map(SharedMailDeleteStep::as_str)
            .collect()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SharedMailDeleteError {
    InvalidField(&'static str),
    WrongService,
    NotMarked,
    NotYours,
    OwnerUnknown(MailOwnerCheckError),
    MessageNotInFolder,
    FolderReadFailed(String),
    MoveToTrashFailed(String),
    NotInTrashAfterMove,
    RemoveFromTrashFailed(String),
    CopyLeftInFolder(usize),
    CopyLeftInTrash(usize),
    WholeTrashEmptied,
}

impl SharedMailDeleteError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidField(_) => "invalid_field",
            Self::WrongService => "wrong_service",
            Self::NotMarked => "not_marked",
            Self::NotYours => "not_yours",
            Self::OwnerUnknown(_) => "owner_unknown",
            Self::MessageNotInFolder => "message_not_in_folder",
            Self::FolderReadFailed(_) => "folder_read_failed",
            Self::MoveToTrashFailed(_) => "move_to_trash_failed",
            Self::NotInTrashAfterMove => "not_in_trash_after_move",
            Self::RemoveFromTrashFailed(_) => "remove_from_trash_failed",
            Self::CopyLeftInFolder(_) => "copy_left_in_folder",
            Self::CopyLeftInTrash(_) => "copy_left_in_trash",
            Self::WholeTrashEmptied => "whole_trash_emptied",
        }
    }

    pub fn reason(&self) -> String {
        match self {
            Self::InvalidField(field) => format!("OSL: mail delete {field} is invalid"),
            Self::WrongService => {
                "OSL: mail delete asks for a different service than this one".to_owned()
            }
            Self::NotMarked => "OSL: mail message was not marked for deletion".to_owned(),
            Self::NotYours => "OSL: mail message was not sent by the signed-in account".to_owned(),
            Self::OwnerUnknown(error) => error.reason().to_owned(),
            Self::MessageNotInFolder => "OSL: mail message is not in that folder".to_owned(),
            Self::FolderReadFailed(detail) => format!("OSL: mail folder cannot be read: {detail}"),
            Self::MoveToTrashFailed(detail) => {
                format!("OSL: mail message could not be moved to trash: {detail}")
            }
            Self::NotInTrashAfterMove => {
                "OSL: mail message is not in trash after the move to trash".to_owned()
            }
            Self::RemoveFromTrashFailed(detail) => {
                format!("OSL: mail message could not be removed from trash: {detail}")
            }
            Self::CopyLeftInFolder(count) => {
                format!("OSL: {count} copies of the mail message are still in the folder")
            }
            Self::CopyLeftInTrash(count) => {
                format!("OSL: {count} copies of the mail message are still in trash")
            }
            Self::WholeTrashEmptied => {
                "OSL: the whole trash was emptied, which this command never does".to_owned()
            }
        }
    }
}

impl fmt::Display for SharedMailDeleteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason())
    }
}

impl From<MailOwnerCheckError> for SharedMailDeleteError {
    fn from(error: MailOwnerCheckError) -> Self {
        Self::OwnerUnknown(error)
    }
}

/// The one fill-in a mail service writes. Note what is missing: there is no
/// `empty_trash`, so a service cannot offer the deleter that shortcut and the
/// deleter cannot take it.
pub trait SharedMailTrashSurface {
    fn service_id(&self) -> &str;

    /// The folder this service calls its trash — "Trash", "Deleted Items",
    /// "Bin" — named by the service, not guessed here.
    fn trash_folder_id(&self) -> &str;

    fn message_ids_in_folder(&self, folder_id: &str) -> Result<Vec<String>, String>;

    fn visible_message(
        &self,
        folder_id: &str,
        message_id: &str,
    ) -> Result<Option<VisibleMailMessage>, String>;

    fn move_message_to_trash(&mut self, folder_id: &str, message_id: &str) -> Result<(), String>;

    fn remove_one_message_from_trash(&mut self, message_id: &str) -> Result<(), String>;
}

/// Move one marked message to trash and then take that one message back out of
/// trash. Every check runs before the first action, so a refusal never reaches
/// the service fill-in at all.
pub fn delete_marked_mail_message(
    surface: &mut impl SharedMailTrashSurface,
    request: &SharedMailDeleteRequest,
) -> Result<SharedMailDeleteReceipt, SharedMailDeleteError> {
    validate_field(&request.service_id, "service id")?;
    validate_field(&request.signed_in_address, "signed-in address")?;
    validate_field(&request.folder_id, "folder id")?;
    validate_field(&request.message_id, "message id")?;

    if request.service_id != surface.service_id() {
        return Err(SharedMailDeleteError::WrongService);
    }

    let trash_folder_id = surface.trash_folder_id().to_owned();
    validate_field(&trash_folder_id, "trash folder id")?;

    // The review is what puts a message in scope at all, so it is checked
    // before whose message it is.
    if !request.reviewed || request.decision != SharedMailReviewDecision::MarkedForDeletion {
        return Err(SharedMailDeleteError::NotMarked);
    }

    let visible = surface
        .visible_message(&request.folder_id, &request.message_id)
        .map_err(SharedMailDeleteError::FolderReadFailed)?
        .ok_or(SharedMailDeleteError::MessageNotInFolder)?;
    if !mail_message_is_owned_by_signed_in_address(&request.signed_in_address, &visible)? {
        return Err(SharedMailDeleteError::NotYours);
    }

    let trash_ids_before = read_folder(surface, &trash_folder_id)?;
    let other_trash_before = other_trash_ids(&trash_ids_before, &request.message_id);

    let mut steps = Vec::new();
    if request.folder_id == trash_folder_id {
        // Already sitting in trash: the move is the identity, so the run is
        // just the second half. Recorded so the receipt still says what happened.
        steps.push(SharedMailDeleteStep::AlreadyInTrash);
    } else {
        surface
            .move_message_to_trash(&request.folder_id, &request.message_id)
            .map_err(SharedMailDeleteError::MoveToTrashFailed)?;
        steps.push(SharedMailDeleteStep::MovedToTrash);

        let folder_after_move = count_copies(
            &read_folder(surface, &request.folder_id)?,
            &request.message_id,
        );
        if folder_after_move != 0 {
            return Err(SharedMailDeleteError::CopyLeftInFolder(folder_after_move));
        }
        if count_copies(
            &read_folder(surface, &trash_folder_id)?,
            &request.message_id,
        ) == 0
        {
            return Err(SharedMailDeleteError::NotInTrashAfterMove);
        }
    }

    surface
        .remove_one_message_from_trash(&request.message_id)
        .map_err(SharedMailDeleteError::RemoveFromTrashFailed)?;
    steps.push(SharedMailDeleteStep::RemovedOneMessageFromTrash);

    let folder_copies_after = count_copies(
        &read_folder(surface, &request.folder_id)?,
        &request.message_id,
    );
    if folder_copies_after != 0 {
        return Err(SharedMailDeleteError::CopyLeftInFolder(folder_copies_after));
    }

    let trash_ids_after = read_folder(surface, &trash_folder_id)?;
    let trash_copies_after = count_copies(&trash_ids_after, &request.message_id);
    if trash_copies_after != 0 {
        return Err(SharedMailDeleteError::CopyLeftInTrash(trash_copies_after));
    }

    let other_trash_after = other_trash_ids(&trash_ids_after, &request.message_id);
    let whole_trash_emptied = other_trash_before
        .iter()
        .any(|id| !other_trash_after.contains(id));
    if whole_trash_emptied {
        return Err(SharedMailDeleteError::WholeTrashEmptied);
    }

    Ok(SharedMailDeleteReceipt {
        service_id: request.service_id.clone(),
        folder_id: request.folder_id.clone(),
        message_id: request.message_id.clone(),
        trash_folder_id,
        steps,
        folder_copies_after,
        trash_copies_after,
        trash_ids_before,
        trash_ids_after,
        other_trash_messages_before: other_trash_before.len(),
        other_trash_messages_after: other_trash_after.len(),
        whole_trash_emptied,
    })
}

fn read_folder(
    surface: &impl SharedMailTrashSurface,
    folder_id: &str,
) -> Result<Vec<String>, SharedMailDeleteError> {
    surface
        .message_ids_in_folder(folder_id)
        .map_err(SharedMailDeleteError::FolderReadFailed)
}

fn count_copies(ids: &[String], message_id: &str) -> usize {
    ids.iter().filter(|id| *id == message_id).count()
}

fn other_trash_ids(ids: &[String], message_id: &str) -> Vec<String> {
    ids.iter().filter(|id| *id != message_id).cloned().collect()
}

fn validate_field(value: &str, name: &'static str) -> Result<(), SharedMailDeleteError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) || value.len() > 180 {
        return Err(SharedMailDeleteError::InvalidField(name));
    }
    Ok(())
}

/// A provider-neutral folder store: the fixture the direct command runs against
/// and the shape a real mail fill-in drives. Copies are counted by reading the
/// folders back, not from anything the deleter remembered.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SharedMailFolderStore {
    service_id: String,
    trash_folder_id: String,
    messages: Vec<StoredMailMessage>,
    move_calls: Vec<String>,
    remove_calls: Vec<String>,
    removal_takes_whole_trash: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StoredMailMessage {
    pub folder_id: String,
    pub message_id: String,
    pub sender_address: Option<String>,
}

impl StoredMailMessage {
    pub fn new(
        folder_id: impl Into<String>,
        message_id: impl Into<String>,
        sender_address: impl Into<String>,
    ) -> Self {
        Self {
            folder_id: folder_id.into(),
            message_id: message_id.into(),
            sender_address: Some(sender_address.into()),
        }
    }
}

impl SharedMailFolderStore {
    pub fn new(service_id: impl Into<String>, trash_folder_id: impl Into<String>) -> Self {
        Self {
            service_id: service_id.into(),
            trash_folder_id: trash_folder_id.into(),
            messages: Vec::new(),
            move_calls: Vec::new(),
            remove_calls: Vec::new(),
            removal_takes_whole_trash: false,
        }
    }

    pub fn with_message(mut self, message: StoredMailMessage) -> Self {
        self.messages.push(message);
        self
    }

    /// A deliberately bad service fill-in that reaches for "empty trash" when it
    /// was asked to remove one message. Used to show the guard is not decoration.
    pub fn removing_one_takes_whole_trash(mut self) -> Self {
        self.removal_takes_whole_trash = true;
        self
    }

    pub fn count_in_folder(&self, folder_id: &str, message_id: &str) -> usize {
        self.messages
            .iter()
            .filter(|message| message.folder_id == folder_id && message.message_id == message_id)
            .count()
    }

    pub fn folder_message_ids(&self, folder_id: &str) -> Vec<String> {
        self.messages
            .iter()
            .filter(|message| message.folder_id == folder_id)
            .map(|message| message.message_id.clone())
            .collect()
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

impl SharedMailTrashSurface for SharedMailFolderStore {
    fn service_id(&self) -> &str {
        &self.service_id
    }

    fn trash_folder_id(&self) -> &str {
        &self.trash_folder_id
    }

    fn message_ids_in_folder(&self, folder_id: &str) -> Result<Vec<String>, String> {
        Ok(self.folder_message_ids(folder_id))
    }

    fn visible_message(
        &self,
        folder_id: &str,
        message_id: &str,
    ) -> Result<Option<VisibleMailMessage>, String> {
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
        let trash = self.trash_folder_id.clone();
        let mut moved = false;
        for message in &mut self.messages {
            if message.folder_id == folder_id && message.message_id == message_id {
                message.folder_id = trash.clone();
                moved = true;
            }
        }
        if moved {
            Ok(())
        } else {
            Err("message is not in that folder".to_owned())
        }
    }

    fn remove_one_message_from_trash(&mut self, message_id: &str) -> Result<(), String> {
        self.remove_calls.push(message_id.to_owned());
        let trash = self.trash_folder_id.clone();
        if self.removal_takes_whole_trash {
            self.messages.retain(|message| message.folder_id != trash);
            return Ok(());
        }
        self.messages
            .retain(|message| !(message.folder_id == trash && message.message_id == message_id));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVICE_ID: &str = "gmail";
    const SIGNED_IN: &str = "owner@example.test";
    const FOLDER: &str = "Sent";
    const TRASH: &str = "Trash";
    const MARKED_MESSAGE: &str = "sent-task-3045-marked";
    const ALREADY_IN_TRASH: &str = "trash-task-3045-was-already-here";

    fn seeded_store() -> SharedMailFolderStore {
        SharedMailFolderStore::new(SERVICE_ID, TRASH)
            .with_message(StoredMailMessage::new(FOLDER, MARKED_MESSAGE, SIGNED_IN))
            .with_message(StoredMailMessage::new(TRASH, ALREADY_IN_TRASH, SIGNED_IN))
    }

    fn marked_request() -> SharedMailDeleteRequest {
        SharedMailDeleteRequest::marked(SERVICE_ID, SIGNED_IN, FOLDER, MARKED_MESSAGE)
    }

    #[test]
    fn task_3045_direct_run_leaves_no_copy_in_the_folder_or_trash_and_keeps_the_other_trash_message(
    ) {
        let mut store = seeded_store();

        assert_eq!(store.count_in_folder(FOLDER, MARKED_MESSAGE), 1);
        assert_eq!(store.count_in_folder(TRASH, ALREADY_IN_TRASH), 1);
        println!(
            "TASK3045 before_folder_copies_of_marked_message={}",
            store.count_in_folder(FOLDER, MARKED_MESSAGE)
        );
        println!(
            "TASK3045 before_trash_copies_of_marked_message={}",
            store.count_in_folder(TRASH, MARKED_MESSAGE)
        );
        println!(
            "TASK3045 before_trash_copies_of_second_message={}",
            store.count_in_folder(TRASH, ALREADY_IN_TRASH)
        );

        let receipt = delete_marked_mail_message(&mut store, &marked_request())
            .expect("the direct run succeeds");

        let folder_copies_after = store.count_in_folder(FOLDER, MARKED_MESSAGE);
        let trash_copies_after = store.count_in_folder(TRASH, MARKED_MESSAGE);
        let second_message_after = store.count_in_folder(TRASH, ALREADY_IN_TRASH);

        println!(
            "TASK3045 direct_run_steps={}",
            receipt.step_names().join(",")
        );
        println!("TASK3045 after_folder_copies_of_marked_message={folder_copies_after}");
        println!("TASK3045 after_trash_copies_of_marked_message={trash_copies_after}");
        println!("TASK3045 after_trash_copies_of_second_message={second_message_after}");
        println!(
            "TASK3045 after_trash_message_ids={}",
            store.folder_message_ids(TRASH).join(",")
        );
        println!(
            "TASK3045 move_to_trash_calls={}",
            store.move_calls().join(",")
        );
        println!(
            "TASK3045 remove_from_trash_calls={}",
            store.remove_calls().join(",")
        );
        println!(
            "TASK3045 whole_trash_emptied={}",
            receipt.whole_trash_emptied
        );

        assert_eq!(
            receipt.step_names(),
            vec!["move_to_trash", "remove_one_message_from_trash"],
            "the run is a move to trash followed by removing that one message"
        );
        assert_eq!(folder_copies_after, 0, "zero copies left in the folder");
        assert_eq!(trash_copies_after, 0, "zero copies left in trash");
        assert_eq!(receipt.folder_copies_after, 0);
        assert_eq!(receipt.trash_copies_after, 0);
        assert_eq!(
            second_message_after, 1,
            "the message already sitting in trash before the run is still there"
        );
        assert_eq!(
            store.folder_message_ids(TRASH),
            vec![ALREADY_IN_TRASH.to_owned()],
            "trash holds exactly the message that was already there"
        );
        assert_eq!(store.total_message_count(), 1);
        assert!(!receipt.whole_trash_emptied);
        assert_eq!(receipt.other_trash_messages_before, 1);
        assert_eq!(receipt.other_trash_messages_after, 1);
        assert_eq!(store.move_calls(), [format!("{FOLDER}/{MARKED_MESSAGE}")]);
        assert_eq!(
            store.remove_calls(),
            [MARKED_MESSAGE.to_owned()],
            "removal is asked for by message id, exactly once"
        );
    }

    #[test]
    fn task_3045_a_service_that_empties_the_whole_trash_is_refused() {
        let mut store = seeded_store().removing_one_takes_whole_trash();

        let error = delete_marked_mail_message(&mut store, &marked_request())
            .expect_err("emptying the whole trash is never this command's outcome");

        println!("TASK3045 whole_trash_guard_code={}", error.code());
        println!("TASK3045 whole_trash_guard_reason={}", error.reason());
        println!(
            "TASK3045 whole_trash_guard_trash_after=[{}]",
            store.folder_message_ids(TRASH).join(",")
        );

        assert_eq!(error, SharedMailDeleteError::WholeTrashEmptied);
        assert_eq!(error.code(), "whole_trash_emptied");
        assert_eq!(
            error.reason(),
            "OSL: the whole trash was emptied, which this command never does"
        );
    }

    #[test]
    fn task_3045_removing_one_message_never_touches_the_rest_of_trash() {
        // The trait offers exactly two calls and neither one is an empty-trash
        // call, so the only removal a service can be asked for is by message id.
        let mut store = seeded_store();
        let _ = store.move_message_to_trash(FOLDER, "no-such-message");
        let _ = store.remove_one_message_from_trash("no-such-message");

        println!(
            "TASK3045 unrelated_removal_trash_after=[{}]",
            store.folder_message_ids(TRASH).join(",")
        );
        assert_eq!(
            store.count_in_folder(TRASH, ALREADY_IN_TRASH),
            1,
            "asking to remove an unrelated id never touches the rest of trash"
        );
        assert_eq!(store.count_in_folder(FOLDER, MARKED_MESSAGE), 1);
    }

    #[test]
    fn task_3045_a_marked_message_already_in_trash_is_removed_without_a_move() {
        let mut store = SharedMailFolderStore::new(SERVICE_ID, TRASH)
            .with_message(StoredMailMessage::new(TRASH, MARKED_MESSAGE, SIGNED_IN))
            .with_message(StoredMailMessage::new(TRASH, ALREADY_IN_TRASH, SIGNED_IN));
        let request = SharedMailDeleteRequest::marked(SERVICE_ID, SIGNED_IN, TRASH, MARKED_MESSAGE);

        let receipt = delete_marked_mail_message(&mut store, &request).expect("the run succeeds");

        println!(
            "TASK3045 already_in_trash_steps={}",
            receipt.step_names().join(",")
        );
        assert_eq!(
            receipt.step_names(),
            vec!["already_in_trash", "remove_one_message_from_trash"]
        );
        assert!(store.move_calls().is_empty());
        assert_eq!(store.count_in_folder(TRASH, MARKED_MESSAGE), 0);
        assert_eq!(store.count_in_folder(TRASH, ALREADY_IN_TRASH), 1);
    }

    #[test]
    fn task_3045_an_unmarked_or_unreviewed_message_is_refused_before_anything_moves() {
        for (label, reviewed, decision) in [
            ("keep", true, SharedMailReviewDecision::Keep),
            (
                "unreviewed_mark",
                false,
                SharedMailReviewDecision::MarkedForDeletion,
            ),
        ] {
            let mut store = seeded_store();
            let mut request = marked_request();
            request.reviewed = reviewed;
            request.decision = decision;

            let error = delete_marked_mail_message(&mut store, &request).expect_err("refused");
            println!("TASK3045 refusal_{label}={}", error.code());

            assert_eq!(error, SharedMailDeleteError::NotMarked);
            assert!(store.move_calls().is_empty());
            assert!(store.remove_calls().is_empty());
            assert_eq!(store.count_in_folder(FOLDER, MARKED_MESSAGE), 1);
        }
    }

    #[test]
    fn task_3045_a_message_the_account_did_not_send_is_refused_before_anything_moves() {
        let mut store = SharedMailFolderStore::new(SERVICE_ID, TRASH)
            .with_message(StoredMailMessage::new(
                FOLDER,
                MARKED_MESSAGE,
                "someone-else@example.test",
            ))
            .with_message(StoredMailMessage::new(TRASH, ALREADY_IN_TRASH, SIGNED_IN));

        let error = delete_marked_mail_message(&mut store, &marked_request()).expect_err("refused");
        println!("TASK3045 refusal_not_yours={}", error.code());
        println!("TASK3045 refusal_not_yours_reason={}", error.reason());

        assert_eq!(error, SharedMailDeleteError::NotYours);
        assert!(store.move_calls().is_empty());
        assert!(store.remove_calls().is_empty());
        assert_eq!(store.count_in_folder(FOLDER, MARKED_MESSAGE), 1);
        assert_eq!(store.count_in_folder(TRASH, ALREADY_IN_TRASH), 1);
    }

    #[test]
    fn task_3045_an_unreadable_sender_is_refused_rather_than_guessed() {
        let mut store =
            SharedMailFolderStore::new(SERVICE_ID, TRASH).with_message(StoredMailMessage {
                folder_id: FOLDER.to_owned(),
                message_id: MARKED_MESSAGE.to_owned(),
                sender_address: None,
            });

        let error = delete_marked_mail_message(&mut store, &marked_request()).expect_err("refused");
        println!("TASK3045 refusal_owner_unknown={}", error.code());
        println!("TASK3045 refusal_owner_unknown_reason={}", error.reason());

        assert_eq!(
            error,
            SharedMailDeleteError::OwnerUnknown(MailOwnerCheckError::SenderAddressUnreadable)
        );
        assert!(store.remove_calls().is_empty());
        assert_eq!(store.count_in_folder(FOLDER, MARKED_MESSAGE), 1);
    }

    #[test]
    fn task_3045_a_message_missing_from_the_named_folder_is_refused() {
        let mut store = seeded_store();
        let request =
            SharedMailDeleteRequest::marked(SERVICE_ID, SIGNED_IN, FOLDER, "no-such-message");

        let error = delete_marked_mail_message(&mut store, &request).expect_err("refused");
        println!("TASK3045 refusal_message_not_in_folder={}", error.code());

        assert_eq!(error, SharedMailDeleteError::MessageNotInFolder);
        assert!(store.remove_calls().is_empty());
        assert_eq!(store.total_message_count(), 2);
    }

    #[test]
    fn task_3045_a_request_for_a_different_service_is_refused() {
        let mut store = seeded_store();
        let request = SharedMailDeleteRequest::marked("icloud", SIGNED_IN, FOLDER, MARKED_MESSAGE);

        let error = delete_marked_mail_message(&mut store, &request).expect_err("refused");
        println!("TASK3045 refusal_wrong_service={}", error.code());

        assert_eq!(error, SharedMailDeleteError::WrongService);
        assert_eq!(store.total_message_count(), 2);
    }
}
