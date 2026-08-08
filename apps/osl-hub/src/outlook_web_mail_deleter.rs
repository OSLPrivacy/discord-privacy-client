//! Outlook on the web's fill-in of the shared mail deleter (TASK 3052).
//!
//! Gate 3045 owns the order of a mail deletion — move the marked message to
//! trash, then take **that one message** back out of trash — and refuses the
//! whole run before the first action unless the review marked the row and the
//! gate 3044 owner check says the signed-in account sent it. None of that is
//! repeated here. This module is only the per-service half: the
//! [`SharedMailTrashSurface`] fill-in for Outlook on the web.
//!
//! What is Outlook's, and therefore lives here:
//!
//! * Outlook calls its trash **Deleted Items**, not "Trash" or "Bin", and it
//!   calls the folder that scrubbed sent mail sits in **Sent Items**. Those are
//!   the same folder names gate 3050's Outlook web mailbox reader
//!   (`services::read_outlook_web_scrub_mailbox`) reads, and
//!   `task_3052_outlook_web_folder_names_match_the_gate_3050_reader` fails if
//!   the two ever drift apart.
//! * The trash folder id is looked up in the folder list the surface actually
//!   shows, rather than being asserted. A mailbox whose folder pane has no
//!   Deleted Items yields no trash folder id at all, and gate 3045 then refuses
//!   the run rather than deleting into a folder nobody named.
//! * Removing the message from Deleted Items is one row, by message id. Outlook
//!   on the web also offers "Empty folder" on Deleted Items; this fill-in never
//!   reaches for it, and [`SharedMailTrashSurface`] gives it no way to.

use crate::mail_owner_check::VisibleMailMessage;
use crate::shared_mail_deleter::{
    delete_marked_mail_message, SharedMailDeleteError, SharedMailDeleteReceipt,
    SharedMailDeleteRequest, SharedMailTrashSurface,
};

/// The service id Outlook on the web is known by. The same string
/// `service_connections::OUTLOOK_WEB_SERVICE_ID` carries — it is written out
/// again rather than imported because `service_connections.rs` cannot be
/// compiled in this worktree (see the module note in
/// `task_3052_outlook_web_mail_deleter/Cargo.toml`), and
/// `task_3052_outlook_web_service_id_matches_service_connections` fails if the
/// two stop agreeing.
///
/// Outlook desktop and Outlook on the web are different surfaces with different
/// fill-ins, so the plain `"outlook"` that `services.rs` passes to both readers
/// is deliberately not what a deletion is addressed to.
pub const OUTLOOK_WEB_SERVICE_ID: &str = "outlook-web";

pub const OUTLOOK_WEB_INBOX_FOLDER: &str = "Inbox";
pub const OUTLOOK_WEB_SENT_ITEMS_FOLDER: &str = "Sent Items";
pub const OUTLOOK_WEB_ARCHIVE_FOLDER: &str = "Archive";
/// Outlook's own name for its trash.
pub const OUTLOOK_WEB_DELETED_ITEMS_FOLDER: &str = "Deleted Items";

/// The four folders gate 3050's seeded Outlook web mailbox shows, in the order
/// its reader returns them.
pub const OUTLOOK_WEB_SCRUB_FOLDERS: [&str; 4] = [
    OUTLOOK_WEB_INBOX_FOLDER,
    OUTLOOK_WEB_SENT_ITEMS_FOLDER,
    OUTLOOK_WEB_ARCHIVE_FOLDER,
    OUTLOOK_WEB_DELETED_ITEMS_FOLDER,
];

/// One row of the Outlook web message list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutlookWebMailRow {
    pub folder: String,
    pub message_id: String,
    pub subject: String,
    /// The From address Outlook shows for the row. `None` is a row whose sender
    /// the surface could not read, which the owner check refuses rather than
    /// guesses.
    pub from_address: Option<String>,
}

impl OutlookWebMailRow {
    pub fn new(
        folder: impl Into<String>,
        message_id: impl Into<String>,
        subject: impl Into<String>,
        from_address: impl Into<String>,
    ) -> Self {
        Self {
            folder: folder.into(),
            message_id: message_id.into(),
            subject: subject.into(),
            from_address: Some(from_address.into()),
        }
    }

    pub fn with_unreadable_sender(
        folder: impl Into<String>,
        message_id: impl Into<String>,
        subject: impl Into<String>,
    ) -> Self {
        Self {
            folder: folder.into(),
            message_id: message_id.into(),
            subject: subject.into(),
            from_address: None,
        }
    }
}

/// The Outlook web mail surface the shared deleter drives: a folder pane and
/// the rows sitting in those folders, plus a record of the two actions the
/// deleter is allowed to ask for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutlookWebMailSurface {
    folders: Vec<String>,
    rows: Vec<OutlookWebMailRow>,
    move_to_deleted_items_calls: Vec<String>,
    delete_from_deleted_items_calls: Vec<String>,
    delete_presses_empty_folder: bool,
}

impl OutlookWebMailSurface {
    pub fn new(folders: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            folders: folders.into_iter().map(Into::into).collect(),
            rows: Vec::new(),
            move_to_deleted_items_calls: Vec::new(),
            delete_from_deleted_items_calls: Vec::new(),
            delete_presses_empty_folder: false,
        }
    }

    /// The folder pane gate 3050's Outlook web reader sees.
    pub fn with_scrub_folders() -> Self {
        Self::new(OUTLOOK_WEB_SCRUB_FOLDERS)
    }

    pub fn with_row(mut self, row: OutlookWebMailRow) -> Self {
        self.rows.push(row);
        self
    }

    /// A deliberately wrong fill-in: asked to delete one row out of Deleted
    /// Items, it presses Outlook's "Empty folder" instead. Nothing in
    /// [`SharedMailTrashSurface`] offers this; it exists so the gate 3045 guard
    /// can be shown going red.
    pub fn pressing_empty_folder_instead(mut self) -> Self {
        self.delete_presses_empty_folder = true;
        self
    }

    pub fn folders(&self) -> &[String] {
        &self.folders
    }

    pub fn shows_folder(&self, folder: &str) -> bool {
        self.folders.iter().any(|shown| shown == folder)
    }

    pub fn rows_in_folder(&self, folder: &str) -> Vec<&OutlookWebMailRow> {
        self.rows
            .iter()
            .filter(|row| row.folder == folder)
            .collect()
    }

    pub fn message_ids_in(&self, folder: &str) -> Vec<String> {
        self.rows_in_folder(folder)
            .into_iter()
            .map(|row| row.message_id.clone())
            .collect()
    }

    pub fn subjects_in(&self, folder: &str) -> Vec<String> {
        self.rows_in_folder(folder)
            .into_iter()
            .map(|row| row.subject.clone())
            .collect()
    }

    pub fn count_in_folder(&self, folder: &str, message_id: &str) -> usize {
        self.rows
            .iter()
            .filter(|row| row.folder == folder && row.message_id == message_id)
            .count()
    }

    /// How many rows in `folder` carry `marker` in their subject — the count the
    /// TASK 3052 finish line is written in.
    pub fn count_matching_subject(&self, folder: &str, marker: &str) -> usize {
        self.rows
            .iter()
            .filter(|row| row.folder == folder && row.subject.contains(marker))
            .count()
    }

    pub fn total_row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn move_to_deleted_items_calls(&self) -> &[String] {
        &self.move_to_deleted_items_calls
    }

    pub fn delete_from_deleted_items_calls(&self) -> &[String] {
        &self.delete_from_deleted_items_calls
    }
}

impl SharedMailTrashSurface for OutlookWebMailSurface {
    fn service_id(&self) -> &str {
        OUTLOOK_WEB_SERVICE_ID
    }

    fn trash_folder_id(&self) -> &str {
        // Read out of the folder pane rather than asserted: a mailbox that does
        // not show Deleted Items has no trash folder id, and gate 3045 refuses
        // the run with invalid_field rather than picking a folder itself.
        self.folders
            .iter()
            .find(|folder| *folder == OUTLOOK_WEB_DELETED_ITEMS_FOLDER)
            .map(String::as_str)
            .unwrap_or("")
    }

    fn message_ids_in_folder(&self, folder_id: &str) -> Result<Vec<String>, String> {
        if !self.shows_folder(folder_id) {
            return Err(format!("Outlook web shows no {folder_id} folder"));
        }
        Ok(self.message_ids_in(folder_id))
    }

    fn visible_message(
        &self,
        folder_id: &str,
        message_id: &str,
    ) -> Result<Option<VisibleMailMessage>, String> {
        if !self.shows_folder(folder_id) {
            return Err(format!("Outlook web shows no {folder_id} folder"));
        }
        Ok(self
            .rows
            .iter()
            .find(|row| row.folder == folder_id && row.message_id == message_id)
            .map(|row| VisibleMailMessage {
                message_id: row.message_id.clone(),
                mailbox: row.folder.clone(),
                sender_address: row.from_address.clone(),
            }))
    }

    fn move_message_to_trash(&mut self, folder_id: &str, message_id: &str) -> Result<(), String> {
        if !self.shows_folder(OUTLOOK_WEB_DELETED_ITEMS_FOLDER) {
            return Err("Outlook web shows no Deleted Items folder".to_owned());
        }
        self.move_to_deleted_items_calls
            .push(format!("{folder_id}/{message_id}"));
        let mut moved = false;
        for row in &mut self.rows {
            if row.folder == folder_id && row.message_id == message_id {
                row.folder = OUTLOOK_WEB_DELETED_ITEMS_FOLDER.to_owned();
                moved = true;
            }
        }
        if moved {
            Ok(())
        } else {
            Err(format!("no {message_id} row is in {folder_id}"))
        }
    }

    fn remove_one_message_from_trash(&mut self, message_id: &str) -> Result<(), String> {
        self.delete_from_deleted_items_calls
            .push(message_id.to_owned());
        if self.delete_presses_empty_folder {
            self.rows
                .retain(|row| row.folder != OUTLOOK_WEB_DELETED_ITEMS_FOLDER);
            return Ok(());
        }
        let before = self.rows.len();
        self.rows.retain(|row| {
            !(row.folder == OUTLOOK_WEB_DELETED_ITEMS_FOLDER && row.message_id == message_id)
        });
        if self.rows.len() == before {
            return Err(format!(
                "no {message_id} row is in {OUTLOOK_WEB_DELETED_ITEMS_FOLDER}"
            ));
        }
        Ok(())
    }
}

/// Delete one reviewed, marked Outlook web message: Outlook's own entry point
/// into the shared mail deleter.
pub fn delete_marked_outlook_web_message(
    surface: &mut OutlookWebMailSurface,
    signed_in_address: &str,
    folder: &str,
    message_id: &str,
) -> Result<SharedMailDeleteReceipt, SharedMailDeleteError> {
    delete_marked_mail_message(
        surface,
        &SharedMailDeleteRequest::marked(
            OUTLOOK_WEB_SERVICE_ID,
            signed_in_address,
            folder,
            message_id,
        ),
    )
}

/// The subject marker the TASK 3052 finish line counts, in the `SCRUB-OW-`
/// family gate 3050's seeded mailbox already uses.
pub const OUTLOOK_WEB_DELETE_MARKER: &str = "SCRUB-OW-DEL";
pub const OUTLOOK_WEB_SCRUB_SIGNED_IN_ADDRESS: &str = "owner@outlook.example";
pub const OUTLOOK_WEB_MARKED_MESSAGE_ID: &str = "sent-task-3052-002";
pub const OUTLOOK_WEB_ALREADY_IN_DELETED_ITEMS_ID: &str = "deleted-items-task-3052-earlier";

/// The seeded Outlook web mailbox the TASK 3052 run starts from: three Sent
/// Items rows matching `SCRUB-OW-DEL`, and one unrelated message already sitting
/// in Deleted Items.
///
/// The second sent row is addressed in capitals, the way gate 3050's fixture
/// does, so the run also crosses the owner check's case-insensitive match.
pub fn seeded_outlook_web_scrub_mailbox() -> OutlookWebMailSurface {
    OutlookWebMailSurface::with_scrub_folders()
        .with_row(OutlookWebMailRow::new(
            OUTLOOK_WEB_SENT_ITEMS_FOLDER,
            "sent-task-3052-001",
            "SCRUB-OW-DEL-ONE",
            OUTLOOK_WEB_SCRUB_SIGNED_IN_ADDRESS,
        ))
        .with_row(OutlookWebMailRow::new(
            OUTLOOK_WEB_SENT_ITEMS_FOLDER,
            OUTLOOK_WEB_MARKED_MESSAGE_ID,
            "SCRUB-OW-DEL-TWO",
            OUTLOOK_WEB_SCRUB_SIGNED_IN_ADDRESS.to_ascii_uppercase(),
        ))
        .with_row(OutlookWebMailRow::new(
            OUTLOOK_WEB_SENT_ITEMS_FOLDER,
            "sent-task-3052-003",
            "SCRUB-OW-DEL-THREE",
            OUTLOOK_WEB_SCRUB_SIGNED_IN_ADDRESS,
        ))
        .with_row(OutlookWebMailRow::new(
            OUTLOOK_WEB_DELETED_ITEMS_FOLDER,
            OUTLOOK_WEB_ALREADY_IN_DELETED_ITEMS_ID,
            "Outlook web receipt the owner binned last week",
            "billing@contoso.example",
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mail_owner_check::MailOwnerCheckError;

    const SENT: &str = OUTLOOK_WEB_SENT_ITEMS_FOLDER;
    const DELETED: &str = OUTLOOK_WEB_DELETED_ITEMS_FOLDER;
    const SIGNED_IN: &str = OUTLOOK_WEB_SCRUB_SIGNED_IN_ADDRESS;
    const MARKED: &str = OUTLOOK_WEB_MARKED_MESSAGE_ID;
    const ALREADY_BINNED: &str = OUTLOOK_WEB_ALREADY_IN_DELETED_ITEMS_ID;

    #[test]
    fn task_3052_sent_items_drops_one_marked_message_and_deleted_items_keeps_its_own() {
        let mut surface = seeded_outlook_web_scrub_mailbox();

        let sent_matching_before = surface.count_matching_subject(SENT, OUTLOOK_WEB_DELETE_MARKER);
        let deleted_items_before = surface.rows_in_folder(DELETED).len();
        let deleted_items_matching_before =
            surface.count_matching_subject(DELETED, OUTLOOK_WEB_DELETE_MARKER);
        println!(
            "TASK3052 before sent_items_matching_{OUTLOOK_WEB_DELETE_MARKER}={sent_matching_before}"
        );
        println!("TASK3052 before deleted_items_messages={deleted_items_before}");
        println!(
            "TASK3052 before deleted_items_matching_{OUTLOOK_WEB_DELETE_MARKER}={deleted_items_matching_before}"
        );

        let receipt = delete_marked_outlook_web_message(&mut surface, SIGNED_IN, SENT, MARKED)
            .expect("the marked Outlook web message is deleted");

        let sent_matching_after = surface.count_matching_subject(SENT, OUTLOOK_WEB_DELETE_MARKER);
        let deleted_items_after = surface.rows_in_folder(DELETED).len();
        println!("TASK3052 trash_folder_id={}", receipt.trash_folder_id);
        println!("TASK3052 steps={}", receipt.step_names().join(","));
        println!(
            "TASK3052 after sent_items_matching_{OUTLOOK_WEB_DELETE_MARKER}={sent_matching_after}"
        );
        println!("TASK3052 after deleted_items_messages={deleted_items_after}");
        println!(
            "TASK3052 after deleted_items_ids=[{}]",
            surface.message_ids_in(DELETED).join(",")
        );
        println!(
            "TASK3052 move_to_deleted_items_calls=[{}]",
            surface.move_to_deleted_items_calls().join(",")
        );
        println!(
            "TASK3052 delete_from_deleted_items_calls=[{}]",
            surface.delete_from_deleted_items_calls().join(",")
        );

        assert_eq!(sent_matching_before, 3, "Sent Items holds 3 before the run");
        assert_eq!(
            deleted_items_before, 1,
            "Deleted Items holds 1 message before the run"
        );
        assert_eq!(
            deleted_items_matching_before, 0,
            "the message already in Deleted Items is unrelated to the marker"
        );

        assert_eq!(sent_matching_after, 2, "Sent Items holds 2 after the run");
        assert_eq!(
            deleted_items_after, 1,
            "Deleted Items still holds 1 message after the run"
        );
        assert_eq!(
            surface.message_ids_in(DELETED),
            vec![ALREADY_BINNED.to_owned()],
            "Deleted Items holds exactly the message that was already there"
        );

        assert_eq!(receipt.trash_folder_id, DELETED);
        assert_eq!(
            receipt.step_names(),
            vec!["move_to_trash", "remove_one_message_from_trash"]
        );
        assert_eq!(receipt.folder_copies_after, 0);
        assert_eq!(receipt.trash_copies_after, 0);
        assert!(!receipt.whole_trash_emptied);
        assert_eq!(receipt.other_trash_messages_before, 1);
        assert_eq!(receipt.other_trash_messages_after, 1);
        assert_eq!(surface.count_in_folder(SENT, MARKED), 0);
        assert_eq!(surface.count_in_folder(DELETED, MARKED), 0);
        assert_eq!(
            surface.move_to_deleted_items_calls(),
            [format!("{SENT}/{MARKED}")]
        );
        assert_eq!(
            surface.delete_from_deleted_items_calls(),
            [MARKED.to_owned()],
            "the single message is taken out of Deleted Items by id, exactly once"
        );
    }

    #[test]
    fn task_3052_outlook_web_trash_is_deleted_items_read_out_of_the_folder_pane() {
        let surface = seeded_outlook_web_scrub_mailbox();
        println!("TASK3052 folder_pane=[{}]", surface.folders().join(","));
        println!(
            "TASK3052 trash_folder_id_from_surface={}",
            surface.trash_folder_id()
        );

        assert_eq!(surface.service_id(), "outlook-web");
        assert_eq!(surface.trash_folder_id(), "Deleted Items");
        assert_eq!(
            surface.folders(),
            ["Inbox", "Sent Items", "Archive", "Deleted Items"]
        );
    }

    #[test]
    fn task_3052_a_mailbox_with_no_deleted_items_folder_is_refused_before_anything_moves() {
        let mut surface =
            OutlookWebMailSurface::new([OUTLOOK_WEB_INBOX_FOLDER, OUTLOOK_WEB_SENT_ITEMS_FOLDER])
                .with_row(OutlookWebMailRow::new(
                    SENT,
                    MARKED,
                    "SCRUB-OW-DEL-TWO",
                    SIGNED_IN,
                ));

        let error = delete_marked_outlook_web_message(&mut surface, SIGNED_IN, SENT, MARKED)
            .expect_err("a mailbox with no Deleted Items folder is refused");
        println!("TASK3052 no_deleted_items_folder_code={}", error.code());
        println!("TASK3052 no_deleted_items_folder_reason={}", error.reason());

        assert_eq!(error.code(), "invalid_field");
        assert!(surface.move_to_deleted_items_calls().is_empty());
        assert!(surface.delete_from_deleted_items_calls().is_empty());
        assert_eq!(surface.count_in_folder(SENT, MARKED), 1);
    }

    #[test]
    fn task_3052_pressing_empty_folder_instead_is_refused_by_the_gate_3045_guard() {
        let mut surface = seeded_outlook_web_scrub_mailbox().pressing_empty_folder_instead();

        let error = delete_marked_outlook_web_message(&mut surface, SIGNED_IN, SENT, MARKED)
            .expect_err("emptying Deleted Items is never this command's outcome");
        println!("TASK3052 empty_folder_code={}", error.code());
        println!("TASK3052 empty_folder_reason={}", error.reason());

        assert_eq!(error.code(), "whole_trash_emptied");
        assert_eq!(
            error.reason(),
            "OSL: the whole trash was emptied, which this command never does"
        );
    }

    #[test]
    fn task_3052_a_message_the_account_did_not_send_is_refused_with_not_yours() {
        let mut surface = OutlookWebMailSurface::with_scrub_folders()
            .with_row(OutlookWebMailRow::new(
                SENT,
                "sent-task-3052-delegate",
                "SCRUB-OW-DEL-DELEGATE",
                "assistant@contoso.example",
            ))
            .with_row(OutlookWebMailRow::new(
                DELETED,
                ALREADY_BINNED,
                "Outlook web receipt the owner binned last week",
                "billing@contoso.example",
            ));

        let error = delete_marked_outlook_web_message(
            &mut surface,
            SIGNED_IN,
            SENT,
            "sent-task-3052-delegate",
        )
        .expect_err("a message the signed-in account did not send is refused");
        println!("TASK3052 not_yours_code={}", error.code());
        println!("TASK3052 not_yours_reason={}", error.reason());

        assert_eq!(error.code(), "not_yours");
        assert!(surface.move_to_deleted_items_calls().is_empty());
        assert!(surface.delete_from_deleted_items_calls().is_empty());
        assert_eq!(surface.count_in_folder(SENT, "sent-task-3052-delegate"), 1);
        assert_eq!(surface.rows_in_folder(DELETED).len(), 1);
    }

    #[test]
    fn task_3052_an_unmarked_or_unreviewed_row_never_reaches_outlook_web() {
        for (label, reviewed, decision) in [
            (
                "keep",
                true,
                crate::shared_mail_deleter::SharedMailReviewDecision::Keep,
            ),
            (
                "unreviewed_mark",
                false,
                crate::shared_mail_deleter::SharedMailReviewDecision::MarkedForDeletion,
            ),
        ] {
            let mut surface = seeded_outlook_web_scrub_mailbox();
            let mut request =
                SharedMailDeleteRequest::marked(OUTLOOK_WEB_SERVICE_ID, SIGNED_IN, SENT, MARKED);
            request.reviewed = reviewed;
            request.decision = decision;

            let error = delete_marked_mail_message(&mut surface, &request).expect_err("refused");
            println!("TASK3052 refusal_{label}={}", error.code());

            assert_eq!(error.code(), "not_marked");
            assert!(surface.move_to_deleted_items_calls().is_empty());
            assert!(surface.delete_from_deleted_items_calls().is_empty());
            assert_eq!(
                surface.count_matching_subject(SENT, OUTLOOK_WEB_DELETE_MARKER),
                3
            );
        }
    }

    #[test]
    fn task_3052_a_row_whose_sender_outlook_web_cannot_read_is_refused_rather_than_guessed() {
        let mut surface = OutlookWebMailSurface::with_scrub_folders().with_row(
            OutlookWebMailRow::with_unreadable_sender(SENT, MARKED, "SCRUB-OW-DEL-TWO"),
        );

        let error = delete_marked_outlook_web_message(&mut surface, SIGNED_IN, SENT, MARKED)
            .expect_err("an unreadable From address is refused");
        println!("TASK3052 owner_unknown_code={}", error.code());
        println!("TASK3052 owner_unknown_reason={}", error.reason());

        assert_eq!(
            error,
            SharedMailDeleteError::OwnerUnknown(MailOwnerCheckError::SenderAddressUnreadable)
        );
        assert!(surface.delete_from_deleted_items_calls().is_empty());
        assert_eq!(surface.count_in_folder(SENT, MARKED), 1);
    }

    #[test]
    fn task_3052_an_outlook_desktop_request_is_refused_by_the_web_fill_in() {
        let mut surface = seeded_outlook_web_scrub_mailbox();
        let request = SharedMailDeleteRequest::marked("outlook-desktop", SIGNED_IN, SENT, MARKED);

        let error = delete_marked_mail_message(&mut surface, &request).expect_err("refused");
        println!("TASK3052 wrong_service_code={}", error.code());

        assert_eq!(error, SharedMailDeleteError::WrongService);
        assert_eq!(surface.total_row_count(), 4);
    }

    #[test]
    fn task_3052_outlook_web_service_id_matches_service_connections() {
        const SERVICE_CONNECTIONS: &str = include_str!("service_connections.rs");
        let declared =
            format!("pub const OUTLOOK_WEB_SERVICE_ID: &str = \"{OUTLOOK_WEB_SERVICE_ID}\";");
        println!("TASK3052 service_connections_declares={declared}");

        assert!(
            SERVICE_CONNECTIONS.contains(&declared),
            "service_connections.rs no longer declares OUTLOOK_WEB_SERVICE_ID as {OUTLOOK_WEB_SERVICE_ID:?}"
        );
    }

    #[test]
    fn task_3052_outlook_web_folder_names_match_the_gate_3050_reader() {
        const SERVICES: &str = include_str!("services.rs");

        assert!(
            SERVICES.contains("pub fn read_outlook_web_scrub_mailbox("),
            "gate 3050's Outlook web mailbox reader is no longer in services.rs"
        );
        for folder in OUTLOOK_WEB_SCRUB_FOLDERS {
            let seeded = format!("MailboxFolderCandidate::new(\"{folder}\", \"{folder}\")");
            println!("TASK3052 gate_3050_folder={folder} seeded_as={seeded}");
            assert!(
                SERVICES.contains(&seeded),
                "gate 3050's seeded Outlook web mailbox no longer shows a {folder} folder"
            );
        }
        assert!(
            SERVICES.contains(&format!("\"{OUTLOOK_WEB_SENT_ITEMS_FOLDER}\",")),
            "gate 3050's reader no longer reads {OUTLOOK_WEB_SENT_ITEMS_FOLDER}"
        );
    }
}
