//! Outlook desktop's TASK 3055 fill-in of the shared mail deleter.

use crate::mail_owner_check::VisibleMailMessage;
use crate::shared_mail_deleter::{
    delete_marked_mail_message, SharedMailDeleteError, SharedMailDeleteReceipt,
    SharedMailDeleteRequest, SharedMailTrashSurface,
};

pub const OUTLOOK_DESKTOP_MAIL_SERVICE_ID: &str = "outlook";
pub const OUTLOOK_DESKTOP_DELETED_ITEMS_FOLDER_ID: &str = "Deleted Items";
pub const OUTLOOK_DESKTOP_SENT_ITEMS_FOLDER_ID: &str = "Sent Items";
pub const OUTLOOK_DESKTOP_DEL_MARKER: &str = "SCRUB-OD-DEL";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OutlookDesktopMailRow {
    pub folder_id: String,
    pub message_id: String,
    pub subject: String,
    pub sender_address: Option<String>,
}

impl OutlookDesktopMailRow {
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

/// Outlook desktop rows driven by the shared deleter. The only destructive
/// primitive removes one named message id from Deleted Items.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct OutlookDesktopMailTrashSurface {
    rows: Vec<OutlookDesktopMailRow>,
    move_calls: Vec<String>,
    remove_calls: Vec<String>,
}

impl OutlookDesktopMailTrashSurface {
    pub fn new(rows: impl IntoIterator<Item = OutlookDesktopMailRow>) -> Self {
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

    pub fn subject_matches_in_folder(&self, folder_id: &str, marker: &str) -> Vec<String> {
        self.rows
            .iter()
            .filter(|row| row.folder_id == folder_id && row.subject.contains(marker))
            .map(|row| row.message_id.clone())
            .collect()
    }

    pub fn move_calls(&self) -> &[String] {
        &self.move_calls
    }

    pub fn remove_calls(&self) -> &[String] {
        &self.remove_calls
    }
}

impl SharedMailTrashSurface for OutlookDesktopMailTrashSurface {
    fn service_id(&self) -> &str {
        OUTLOOK_DESKTOP_MAIL_SERVICE_ID
    }

    fn trash_folder_id(&self) -> &str {
        OUTLOOK_DESKTOP_DELETED_ITEMS_FOLDER_ID
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
                row.folder_id = OUTLOOK_DESKTOP_DELETED_ITEMS_FOLDER_ID.to_owned();
                moved = true;
            }
        }
        moved
            .then_some(())
            .ok_or_else(|| "outlook desktop message is not in that folder".to_owned())
    }

    fn remove_one_message_from_trash(&mut self, message_id: &str) -> Result<(), String> {
        self.remove_calls.push(message_id.to_owned());
        self.rows.retain(|row| {
            !(row.folder_id == OUTLOOK_DESKTOP_DELETED_ITEMS_FOLDER_ID
                && row.message_id == message_id)
        });
        Ok(())
    }
}

pub fn delete_marked_outlook_desktop_mail_message(
    surface: &mut OutlookDesktopMailTrashSurface,
    signed_in_address: &str,
    folder_id: &str,
    message_id: &str,
) -> Result<SharedMailDeleteReceipt, SharedMailDeleteError> {
    delete_marked_mail_message(
        surface,
        &SharedMailDeleteRequest::marked(
            OUTLOOK_DESKTOP_MAIL_SERVICE_ID,
            signed_in_address,
            folder_id,
            message_id,
        ),
    )
}
