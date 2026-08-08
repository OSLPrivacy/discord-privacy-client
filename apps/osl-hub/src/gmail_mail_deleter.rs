//! The Gmail fill-in of the shared mail deleter (TASK 3049).
//!
//! TASK 3045 wrote the shared deleter and left one fill-in per mail service:
//! [`SharedMailTrashSurface`]. This module is Gmail's, written against the same
//! [`SharedMailboxSnapshot`] gate 3047's Gmail reader reads, so a delete acts on
//! the mailbox the review actually looked at.
//!
//! Gmail calls its trash **Bin**, and the label is named by the account, not
//! guessed here: [`gmail_bin_label_id`] finds it among the account's own labels
//! (`[Gmail]/Bin`, `Bin`, and the US-English `[Gmail]/Trash`/`Trash` spelling of
//! the same label) and refuses the whole run if the account shows no bin label
//! or shows two different ones.
//!
//! One run is the shared deleter's two readable actions and nothing else:
//! relabel the marked message into Bin, then remove **that one message** from
//! Bin by id. There is no empty-bin call here, because
//! [`SharedMailTrashSurface`] has none to fill in;
//! [`GmailBinSurface::remove_one_message_from_trash`] only ever drops records
//! that are in Bin *and* carry the asked-for message id, and the shared deleter
//! re-reads Bin afterwards and refuses with `whole_trash_emptied` if anything
//! else that was sitting there went missing.
//!
//! Authority is unchanged: the review mark, and gate 3044's owner check. A
//! message the signed-in account did not send is refused `not_yours` before the
//! first action, so the mailbox is untouched.

use crate::mail_owner_check::VisibleMailMessage;
use crate::shared_mail_deleter::{
    delete_marked_mail_message, SharedMailDeleteError, SharedMailDeleteReceipt,
    SharedMailDeleteRequest, SharedMailTrashSurface,
};
use crate::shared_mail_snapshot::{SharedMailMessageRecord, SharedMailboxSnapshot};

/// The service id the shared deleter is asked for. Same id gate 3047's Gmail
/// reader is registered under.
pub const GMAIL_MAIL_SERVICE_ID: &str = "gmail";

/// Every name Gmail gives the one bin label. `[Gmail]/Bin` and `Bin` are what a
/// UK-English account shows; `[Gmail]/Trash` and `Trash` are the US-English
/// spelling of the same label, and gate 3047's own fixture uses it.
pub const GMAIL_BIN_LABEL_NAMES: [&str; 4] = ["[Gmail]/Bin", "Bin", "[Gmail]/Trash", "Trash"];

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum GmailMailDeleteError {
    /// The account shows no bin label at all, so there is nowhere to move the
    /// message to and nothing to remove it from.
    NoBinLabel,
    /// The account shows two different bin labels. Which one Gmail would use is
    /// a guess, and this command does not guess where a message goes.
    TwoBinLabels(String, String),
    InvalidMailbox(&'static str),
    /// A message sits under a label the account does not show.
    UnknownLabel(String),
    /// The shared deleter's own refusal, carried through unchanged so
    /// `not_yours`, `not_marked` and `whole_trash_emptied` read the same for
    /// Gmail as for every other mail service.
    Delete(SharedMailDeleteError),
}

impl GmailMailDeleteError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoBinLabel => "no_bin_label",
            Self::TwoBinLabels(_, _) => "two_bin_labels",
            Self::InvalidMailbox(_) => "invalid_mailbox",
            Self::UnknownLabel(_) => "unknown_label",
            Self::Delete(error) => error.code(),
        }
    }

    pub fn reason(&self) -> String {
        match self {
            Self::NoBinLabel => "OSL: this Gmail account shows no Bin label".to_owned(),
            Self::TwoBinLabels(first, second) => {
                format!("OSL: this Gmail account shows two Bin labels, {first} and {second}")
            }
            Self::InvalidMailbox(field) => format!("OSL: Gmail mailbox {field} is invalid"),
            Self::UnknownLabel(label_id) => {
                format!("OSL: Gmail message sits under an unknown label {label_id}")
            }
            Self::Delete(error) => error.reason(),
        }
    }

    /// The shared refusal underneath, when there is one. `not_yours` is read
    /// through this in the checks.
    pub const fn delete_error(&self) -> Option<&SharedMailDeleteError> {
        match self {
            Self::Delete(error) => Some(error),
            _ => None,
        }
    }
}

impl From<SharedMailDeleteError> for GmailMailDeleteError {
    fn from(error: SharedMailDeleteError) -> Self {
        Self::Delete(error)
    }
}

impl core::fmt::Display for GmailMailDeleteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.reason())
    }
}

/// What one Gmail run did. The shared receipt is carried whole; the two extra
/// fields say which label Gmail's bin turned out to be and exactly what the
/// fill-in was asked to do.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GmailMailDeleteReceipt {
    pub bin_label_id: String,
    pub receipt: SharedMailDeleteReceipt,
    pub relabel_to_bin_calls: Vec<String>,
    pub remove_from_bin_calls: Vec<String>,
}

/// Gmail's fill-in of the shared trash surface. It borrows the account's
/// mailbox, so the deleter writes to the same snapshot the reader read.
#[derive(Debug)]
pub struct GmailBinSurface<'a> {
    mailbox: &'a mut SharedMailboxSnapshot,
    bin_label_id: String,
    relabel_calls: Vec<String>,
    remove_calls: Vec<String>,
}

impl<'a> GmailBinSurface<'a> {
    /// Build the fill-in, taking the bin label from the account's own labels.
    pub fn new(mailbox: &'a mut SharedMailboxSnapshot) -> Result<Self, GmailMailDeleteError> {
        let bin_label_id = gmail_bin_label_id(mailbox)?;
        Ok(Self {
            mailbox,
            bin_label_id,
            relabel_calls: Vec::new(),
            remove_calls: Vec::new(),
        })
    }

    pub fn bin_label_id(&self) -> &str {
        &self.bin_label_id
    }

    pub fn relabel_to_bin_calls(&self) -> &[String] {
        &self.relabel_calls
    }

    pub fn remove_from_bin_calls(&self) -> &[String] {
        &self.remove_calls
    }
}

impl SharedMailTrashSurface for GmailBinSurface<'_> {
    fn service_id(&self) -> &str {
        GMAIL_MAIL_SERVICE_ID
    }

    fn trash_folder_id(&self) -> &str {
        &self.bin_label_id
    }

    fn message_ids_in_folder(&self, folder_id: &str) -> Result<Vec<String>, String> {
        if !gmail_label_exists(self.mailbox, folder_id) {
            return Err(format!("Gmail label {folder_id} was not found"));
        }
        Ok(gmail_label_message_ids(self.mailbox, folder_id))
    }

    fn visible_message(
        &self,
        folder_id: &str,
        message_id: &str,
    ) -> Result<Option<VisibleMailMessage>, String> {
        if !gmail_label_exists(self.mailbox, folder_id) {
            return Err(format!("Gmail label {folder_id} was not found"));
        }
        let mut found =
            self.mailbox.messages.iter().filter(|message| {
                message.label_id == folder_id && message.message_id == message_id
            });
        let Some(message) = found.next() else {
            return Ok(None);
        };
        if found.next().is_some() {
            return Err(format!("Gmail message id {message_id} is duplicated"));
        }
        Ok(Some(VisibleMailMessage {
            message_id: message.message_id.clone(),
            mailbox: message.label_id.clone(),
            sender_address: message.sender_address.clone(),
        }))
    }

    fn move_message_to_trash(&mut self, folder_id: &str, message_id: &str) -> Result<(), String> {
        self.relabel_calls.push(format!("{folder_id}/{message_id}"));
        let bin = self.bin_label_id.clone();
        let mut relabelled = 0usize;
        for message in &mut self.mailbox.messages {
            if message.label_id == folder_id && message.message_id == message_id {
                message.label_id = bin.clone();
                relabelled += 1;
            }
        }
        if relabelled == 0 {
            return Err(format!(
                "Gmail message {message_id} is not under label {folder_id}"
            ));
        }
        Ok(())
    }

    fn remove_one_message_from_trash(&mut self, message_id: &str) -> Result<(), String> {
        self.remove_calls.push(message_id.to_owned());
        let bin = self.bin_label_id.clone();
        let before = self.mailbox.messages.len();
        // Only records that are in Bin *and* carry this id. Nothing else in Bin
        // is in reach, which is what makes "empty the bin" unreachable from
        // here rather than merely unasked-for.
        self.mailbox
            .messages
            .retain(|message| !(message.label_id == bin && message.message_id == message_id));
        if self.mailbox.messages.len() == before {
            return Err(format!("Gmail message {message_id} is not in {bin}"));
        }
        Ok(())
    }
}

/// The label this Gmail account calls its bin, taken from the account's own
/// label list rather than guessed.
pub fn gmail_bin_label_id(mailbox: &SharedMailboxSnapshot) -> Result<String, GmailMailDeleteError> {
    let mut found: Option<String> = None;
    for label in &mailbox.labels {
        if !is_gmail_bin_name(&label.label_id) && !is_gmail_bin_name(&label.name) {
            continue;
        }
        match &found {
            None => found = Some(label.label_id.clone()),
            Some(first) if first == &label.label_id => {}
            Some(first) => {
                return Err(GmailMailDeleteError::TwoBinLabels(
                    first.clone(),
                    label.label_id.clone(),
                ))
            }
        }
    }
    found.ok_or(GmailMailDeleteError::NoBinLabel)
}

fn is_gmail_bin_name(value: &str) -> bool {
    GMAIL_BIN_LABEL_NAMES
        .iter()
        .any(|name| name.eq_ignore_ascii_case(value.trim()))
}

fn gmail_label_exists(mailbox: &SharedMailboxSnapshot, label_id: &str) -> bool {
    mailbox
        .labels
        .iter()
        .any(|label| label.label_id == label_id)
}

/// Every message id under one label, in the order the account shows them.
pub fn gmail_label_message_ids(mailbox: &SharedMailboxSnapshot, label_id: &str) -> Vec<String> {
    mailbox
        .messages
        .iter()
        .filter(|message| message.label_id == label_id)
        .map(|message| message.message_id.clone())
        .collect()
}

/// The messages under one label whose subject carries `mark`. This is how the
/// run is counted: by reading the mailbox back, not by remembering what was
/// seeded.
pub fn gmail_label_messages_matching(
    mailbox: &SharedMailboxSnapshot,
    label_id: &str,
    mark: &str,
) -> Vec<String> {
    mailbox
        .messages
        .iter()
        .filter(|message| message.label_id == label_id && message.subject.contains(mark))
        .map(|message| message.message_id.clone())
        .collect()
}

/// Move one marked Gmail message to Bin and then remove that one message from
/// Bin. Every check runs before the first action.
pub fn delete_marked_gmail_message(
    mailbox: &mut SharedMailboxSnapshot,
    request: &SharedMailDeleteRequest,
) -> Result<GmailMailDeleteReceipt, GmailMailDeleteError> {
    validate_gmail_delete_mailbox(mailbox)?;

    let mut surface = GmailBinSurface::new(mailbox)?;
    let outcome = delete_marked_mail_message(&mut surface, request);
    let bin_label_id = surface.bin_label_id.clone();
    let relabel_to_bin_calls = surface.relabel_calls.clone();
    let remove_from_bin_calls = surface.remove_calls.clone();

    Ok(GmailMailDeleteReceipt {
        bin_label_id,
        receipt: outcome?,
        relabel_to_bin_calls,
        remove_from_bin_calls,
    })
}

fn validate_gmail_delete_mailbox(
    mailbox: &SharedMailboxSnapshot,
) -> Result<(), GmailMailDeleteError> {
    validate_mailbox_text(&mailbox.signed_in_address, "signed-in address", 254)?;
    for label in &mailbox.labels {
        validate_mailbox_text(&label.label_id, "label id", 128)?;
        validate_mailbox_text(&label.name, "label name", 128)?;
    }
    for message in &mailbox.messages {
        validate_gmail_delete_message(message)?;
        if !gmail_label_exists(mailbox, &message.label_id) {
            return Err(GmailMailDeleteError::UnknownLabel(message.label_id.clone()));
        }
    }
    Ok(())
}

fn validate_gmail_delete_message(
    message: &SharedMailMessageRecord,
) -> Result<(), GmailMailDeleteError> {
    validate_mailbox_text(&message.label_id, "label id", 128)?;
    validate_mailbox_text(&message.message_id, "message id", 180)?;
    if message.time <= 0 {
        return Err(GmailMailDeleteError::InvalidMailbox("message time"));
    }
    Ok(())
}

fn validate_mailbox_text(
    value: &str,
    name: &'static str,
    max_len: usize,
) -> Result<(), GmailMailDeleteError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) || value.len() > max_len {
        return Err(GmailMailDeleteError::InvalidMailbox(name));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_mail_snapshot::SharedMailLabel;

    const SIGNED_IN: &str = "scrub-owner@gmail.test";
    const SENT: &str = "Sent";
    const BIN: &str = "[Gmail]/Bin";
    const MARK: &str = "SCRUB-GM-DEL";
    const MARKED_MESSAGE: &str = "sent-scrub-gm-del-one";
    const BIN_UNRELATED: &str = "bin-kept-from-last-week";
    const NOT_MINE: &str = "inbox-scrub-gm-del-not-mine";

    /// Sent holds three SCRUB-GM-DEL messages the signed-in account sent, Bin
    /// holds one unrelated message that was already there, and Inbox holds one
    /// SCRUB-GM-DEL message somebody else sent.
    fn seeded_gmail_mailbox() -> SharedMailboxSnapshot {
        SharedMailboxSnapshot::new(
            SIGNED_IN,
            [
                SharedMailLabel::new("Inbox", "Inbox"),
                SharedMailLabel::new(SENT, "Sent"),
                SharedMailLabel::new("[Gmail]/All Mail", "All Mail"),
                SharedMailLabel::new(BIN, "Bin"),
            ],
            [
                SharedMailMessageRecord::new(
                    SENT,
                    MARKED_MESSAGE,
                    "SCRUB-GM-DEL one",
                    1_786_190_400,
                    SIGNED_IN,
                    "First sent message marked SCRUB-GM-DEL.",
                ),
                SharedMailMessageRecord::new(
                    SENT,
                    "sent-scrub-gm-del-two",
                    "SCRUB-GM-DEL two",
                    1_786_194_000,
                    SIGNED_IN,
                    "Second sent message marked SCRUB-GM-DEL.",
                ),
                SharedMailMessageRecord::new(
                    SENT,
                    "sent-scrub-gm-del-three",
                    "SCRUB-GM-DEL three",
                    1_786_197_600,
                    SIGNED_IN,
                    "Third sent message marked SCRUB-GM-DEL.",
                ),
                SharedMailMessageRecord::new(
                    BIN,
                    BIN_UNRELATED,
                    "Receipt from the shop",
                    1_785_585_600,
                    "shop@gmail.test",
                    "The unrelated message already sitting in Bin before the run.",
                ),
                SharedMailMessageRecord::new(
                    "Inbox",
                    NOT_MINE,
                    "SCRUB-GM-DEL sent by somebody else",
                    1_786_201_200,
                    "friend-one@gmail.test",
                    "A message the signed-in account did not send.",
                ),
            ],
        )
    }

    fn marked_request(label_id: &str, message_id: &str) -> SharedMailDeleteRequest {
        SharedMailDeleteRequest::marked(GMAIL_MAIL_SERVICE_ID, SIGNED_IN, label_id, message_id)
    }

    #[test]
    fn task_3049_gmail_run_takes_one_marked_message_out_of_sent_and_out_of_bin() {
        let mut mailbox = seeded_gmail_mailbox();

        let sent_before = gmail_label_messages_matching(&mailbox, SENT, MARK);
        let bin_before = gmail_label_message_ids(&mailbox, BIN);
        println!("TASK3049 mark={MARK}");
        println!("TASK3049 before_sent_matching_count={}", sent_before.len());
        println!(
            "TASK3049 before_sent_matching_ids=[{}]",
            sent_before.join(",")
        );
        println!("TASK3049 before_bin_count={}", bin_before.len());
        println!("TASK3049 before_bin_ids=[{}]", bin_before.join(","));
        assert_eq!(
            sent_before.len(),
            3,
            "Sent holds 3 SCRUB-GM-DEL messages before the run"
        );
        assert_eq!(
            bin_before,
            vec![BIN_UNRELATED.to_owned()],
            "Bin holds 1 unrelated message before the run"
        );

        let receipt =
            delete_marked_gmail_message(&mut mailbox, &marked_request(SENT, MARKED_MESSAGE))
                .expect("the Gmail run succeeds");

        let sent_after = gmail_label_messages_matching(&mailbox, SENT, MARK);
        let bin_after = gmail_label_message_ids(&mailbox, BIN);
        println!("TASK3049 bin_label_id={}", receipt.bin_label_id);
        println!(
            "TASK3049 run_steps={}",
            receipt.receipt.step_names().join(",")
        );
        println!(
            "TASK3049 relabel_to_bin_calls=[{}]",
            receipt.relabel_to_bin_calls.join(",")
        );
        println!(
            "TASK3049 remove_from_bin_calls=[{}]",
            receipt.remove_from_bin_calls.join(",")
        );
        println!("TASK3049 after_sent_matching_count={}", sent_after.len());
        println!(
            "TASK3049 after_sent_matching_ids=[{}]",
            sent_after.join(",")
        );
        println!("TASK3049 after_bin_count={}", bin_after.len());
        println!("TASK3049 after_bin_ids=[{}]", bin_after.join(","));
        println!(
            "TASK3049 after_copies_of_deleted_message_anywhere={}",
            mailbox
                .messages
                .iter()
                .filter(|message| message.message_id == MARKED_MESSAGE)
                .count()
        );
        println!(
            "TASK3049 whole_trash_emptied={}",
            receipt.receipt.whole_trash_emptied
        );

        assert_eq!(
            receipt.bin_label_id, BIN,
            "the bin label came from the account"
        );
        assert_eq!(
            receipt.receipt.step_names(),
            vec!["move_to_trash", "remove_one_message_from_trash"]
        );
        assert_eq!(
            receipt.relabel_to_bin_calls,
            vec![format!("{SENT}/{MARKED_MESSAGE}")]
        );
        assert_eq!(
            receipt.remove_from_bin_calls,
            vec![MARKED_MESSAGE.to_owned()],
            "removal is asked for by message id, exactly once"
        );
        assert_eq!(
            sent_after.len(),
            2,
            "Sent holds 2 SCRUB-GM-DEL messages after the run"
        );
        assert_eq!(
            sent_after,
            vec![
                "sent-scrub-gm-del-two".to_owned(),
                "sent-scrub-gm-del-three".to_owned()
            ]
        );
        assert_eq!(
            bin_after,
            vec![BIN_UNRELATED.to_owned()],
            "Bin still holds exactly the 1 unrelated message"
        );
        assert_eq!(
            mailbox
                .messages
                .iter()
                .filter(|message| message.message_id == MARKED_MESSAGE)
                .count(),
            0,
            "no copy of the deleted message is left under any label"
        );
        assert!(!receipt.receipt.whole_trash_emptied);
        assert_eq!(receipt.receipt.other_trash_messages_before, 1);
        assert_eq!(receipt.receipt.other_trash_messages_after, 1);
    }

    #[test]
    fn task_3049_a_gmail_message_the_account_did_not_send_is_refused_not_yours() {
        let mut mailbox = seeded_gmail_mailbox();
        let before = mailbox.clone();

        let error = delete_marked_gmail_message(&mut mailbox, &marked_request("Inbox", NOT_MINE))
            .expect_err("a message the account did not send is refused");

        println!("TASK3049 not_yours_code={}", error.code());
        println!("TASK3049 not_yours_reason={}", error.reason());
        println!(
            "TASK3049 not_yours_bin_after=[{}]",
            gmail_label_message_ids(&mailbox, BIN).join(",")
        );
        println!(
            "TASK3049 not_yours_inbox_after=[{}]",
            gmail_label_message_ids(&mailbox, "Inbox").join(",")
        );
        println!("TASK3049 not_yours_mailbox_unchanged={}", mailbox == before);

        assert_eq!(error.code(), "not_yours");
        assert_eq!(error.delete_error(), Some(&SharedMailDeleteError::NotYours));
        assert_eq!(
            error.reason(),
            "OSL: mail message was not sent by the signed-in account"
        );
        assert_eq!(
            mailbox, before,
            "the refusal reached the mailbox before any action, so nothing moved"
        );
    }

    #[test]
    fn task_3049_gmail_finds_the_bin_label_under_either_of_its_names() {
        for (label_id, name) in [
            ("[Gmail]/Bin", "Bin"),
            ("[Gmail]/Trash", "Trash"),
            ("Label_17", "Bin"),
        ] {
            let mailbox = SharedMailboxSnapshot::new(
                SIGNED_IN,
                [
                    SharedMailLabel::new(SENT, "Sent"),
                    SharedMailLabel::new(label_id, name),
                ],
                [],
            );
            let found = gmail_bin_label_id(&mailbox).expect("the bin label is found");
            println!("TASK3049 bin_label_for id={label_id} name={name} -> {found}");
            assert_eq!(found, label_id);
        }
    }

    #[test]
    fn task_3049_a_gmail_account_with_no_bin_label_is_refused_before_anything_moves() {
        let mut mailbox = SharedMailboxSnapshot::new(
            SIGNED_IN,
            [SharedMailLabel::new(SENT, "Sent")],
            [SharedMailMessageRecord::new(
                SENT,
                MARKED_MESSAGE,
                "SCRUB-GM-DEL one",
                1_786_190_400,
                SIGNED_IN,
                "First sent message marked SCRUB-GM-DEL.",
            )],
        );
        let before = mailbox.clone();

        let error =
            delete_marked_gmail_message(&mut mailbox, &marked_request(SENT, MARKED_MESSAGE))
                .expect_err("with no bin label there is nowhere to move the message");

        println!("TASK3049 no_bin_label_code={}", error.code());
        println!("TASK3049 no_bin_label_reason={}", error.reason());
        assert_eq!(error, GmailMailDeleteError::NoBinLabel);
        assert_eq!(mailbox, before);
    }

    #[test]
    fn task_3049_a_gmail_account_showing_two_bin_labels_is_refused_rather_than_guessed() {
        let mut mailbox = SharedMailboxSnapshot::new(
            SIGNED_IN,
            [
                SharedMailLabel::new(SENT, "Sent"),
                SharedMailLabel::new("[Gmail]/Bin", "Bin"),
                SharedMailLabel::new("[Gmail]/Trash", "Trash"),
            ],
            [SharedMailMessageRecord::new(
                SENT,
                MARKED_MESSAGE,
                "SCRUB-GM-DEL one",
                1_786_190_400,
                SIGNED_IN,
                "First sent message marked SCRUB-GM-DEL.",
            )],
        );
        let before = mailbox.clone();

        let error =
            delete_marked_gmail_message(&mut mailbox, &marked_request(SENT, MARKED_MESSAGE))
                .expect_err("two bin labels is a guess this command does not make");

        println!("TASK3049 two_bin_labels_code={}", error.code());
        println!("TASK3049 two_bin_labels_reason={}", error.reason());
        assert_eq!(error.code(), "two_bin_labels");
        assert_eq!(mailbox, before);
    }

    #[test]
    fn task_3049_an_unmarked_gmail_message_is_refused_before_anything_moves() {
        let mut mailbox = seeded_gmail_mailbox();
        let before = mailbox.clone();
        let mut request = marked_request(SENT, MARKED_MESSAGE);
        request.decision = crate::shared_mail_deleter::SharedMailReviewDecision::Keep;

        let error = delete_marked_gmail_message(&mut mailbox, &request).expect_err("refused");

        println!("TASK3049 not_marked_code={}", error.code());
        assert_eq!(error.code(), "not_marked");
        assert_eq!(mailbox, before);
    }

    #[test]
    fn task_3049_removing_the_message_from_bin_never_takes_the_rest_of_bin() {
        // The fill-in is handed an id that is in Bin and one that is not. In
        // both cases every other message in Bin is still there afterwards:
        // there is no reachable call that takes the whole bin.
        let mut mailbox = seeded_gmail_mailbox();
        mailbox.messages.push(SharedMailMessageRecord::new(
            BIN,
            "bin-second-old-message",
            "Another old message",
            1_785_589_200,
            SIGNED_IN,
            "A second message already in Bin.",
        ));
        let mut surface = GmailBinSurface::new(&mut mailbox).expect("bin label found");

        let missing = surface.remove_one_message_from_trash("no-such-message");
        surface
            .move_message_to_trash(SENT, MARKED_MESSAGE)
            .expect("the marked message relabels into Bin");
        surface
            .remove_one_message_from_trash(MARKED_MESSAGE)
            .expect("the marked message is removed from Bin");

        println!("TASK3049 unrelated_removal_refused={}", missing.is_err());
        println!(
            "TASK3049 bin_after_targeted_removal=[{}]",
            gmail_label_message_ids(&mailbox, BIN).join(",")
        );

        assert!(
            missing.is_err(),
            "an id that is not in Bin is an error, not a sweep"
        );
        assert_eq!(
            gmail_label_message_ids(&mailbox, BIN),
            vec![
                BIN_UNRELATED.to_owned(),
                "bin-second-old-message".to_owned()
            ],
            "both messages that were already in Bin are still there"
        );
    }

    #[test]
    fn task_3049_a_request_for_another_service_is_refused() {
        let mut mailbox = seeded_gmail_mailbox();
        let before = mailbox.clone();
        let request = SharedMailDeleteRequest::marked("icloud", SIGNED_IN, SENT, MARKED_MESSAGE);

        let error = delete_marked_gmail_message(&mut mailbox, &request).expect_err("refused");

        println!("TASK3049 wrong_service_code={}", error.code());
        assert_eq!(error.code(), "wrong_service");
        assert_eq!(mailbox, before);
    }
}
