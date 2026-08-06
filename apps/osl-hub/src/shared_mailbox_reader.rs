//! Shared mailbox fixtures and page places for mail-backed readers.
//!
//! Mail adapters own provider-specific IMAP or webmail access. This module owns
//! the provider-neutral folder/message shape and adapts one folder into the
//! shared one-page conversation reader.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::shared_conversation_scroll::{SharedConversationScrollablePlace, SharedPlaceMessage};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedMailboxFolder {
    pub id: String,
    pub label: String,
    pub service: String,
    pub account_id: String,
}

impl SharedMailboxFolder {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        service: impl Into<String>,
        account_id: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            service: service.into(),
            account_id: account_id.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedMailboxMessageSummary {
    pub id: String,
    pub subject: String,
    pub time_unix_seconds: i64,
    pub sender: String,
}

impl SharedMailboxMessageSummary {
    pub fn new(
        id: impl Into<String>,
        subject: impl Into<String>,
        time_unix_seconds: i64,
        sender: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            subject: subject.into(),
            time_unix_seconds,
            sender: sender.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedMailboxMessage {
    pub summary: SharedMailboxMessageSummary,
    pub body: String,
}

impl SharedMailboxMessage {
    pub fn new(summary: SharedMailboxMessageSummary, body: impl Into<String>) -> Self {
        Self {
            summary,
            body: body.into(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SharedMailboxReader {
    folders: Vec<SharedMailboxFolder>,
    messages_by_folder: BTreeMap<String, Vec<SharedMailboxMessage>>,
}

impl SharedMailboxReader {
    pub fn new(
        folders: Vec<SharedMailboxFolder>,
        messages_by_folder: BTreeMap<String, Vec<SharedMailboxMessage>>,
    ) -> Result<Self, String> {
        let mut seen = BTreeMap::new();
        for folder in &folders {
            validate_part(&folder.id, "mailbox folder id")?;
            validate_part(&folder.label, "mailbox folder label")?;
            validate_part(&folder.service, "mailbox service")?;
            validate_part(&folder.account_id, "mailbox account id")?;
            if seen.insert(folder.id.clone(), ()).is_some() {
                return Err("mailbox folder id is duplicated".to_owned());
            }
        }
        for folder_id in messages_by_folder.keys() {
            if !seen.contains_key(folder_id) {
                return Err("mailbox messages reference an unknown folder".to_owned());
            }
        }
        Ok(Self {
            folders,
            messages_by_folder,
        })
    }

    pub fn list_folders(&self) -> Vec<SharedMailboxFolder> {
        self.folders.clone()
    }

    pub fn list_messages(
        &self,
        folder_id: &str,
    ) -> Result<Vec<SharedMailboxMessageSummary>, String> {
        self.messages_for_folder(folder_id).map(|messages| {
            messages
                .iter()
                .map(|message| message.summary.clone())
                .collect()
        })
    }

    pub fn open_message(
        &self,
        folder_id: &str,
        message_id: &str,
    ) -> Result<SharedMailboxMessage, String> {
        self.messages_for_folder(folder_id)?
            .iter()
            .find(|message| message.summary.id == message_id)
            .cloned()
            .ok_or_else(|| "mailbox message not found".to_owned())
    }

    pub fn folder_page_place(
        &self,
        folder_id: &str,
        page_size: usize,
    ) -> Result<SharedMailboxFolderPagePlace, String> {
        if page_size == 0 {
            return Err("mailbox page size must be at least one".to_owned());
        }
        Ok(SharedMailboxFolderPagePlace {
            messages: self.messages_for_folder(folder_id)?.to_vec(),
            current_page: 0,
            page_size,
            one_screen_scrolls: 0,
            stop_when_reading_page: None,
            stop_request_callback: None,
        })
    }

    fn messages_for_folder(&self, folder_id: &str) -> Result<&[SharedMailboxMessage], String> {
        if !self.folders.iter().any(|folder| folder.id == folder_id) {
            return Err("mailbox folder not found".to_owned());
        }
        Ok(self
            .messages_by_folder
            .get(folder_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]))
    }
}

pub struct SharedMailboxFolderPagePlace {
    messages: Vec<SharedMailboxMessage>,
    current_page: usize,
    page_size: usize,
    one_screen_scrolls: usize,
    stop_when_reading_page: Option<usize>,
    stop_request_callback: Option<Box<dyn Fn() -> Result<(), String>>>,
}

impl std::fmt::Debug for SharedMailboxFolderPagePlace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedMailboxFolderPagePlace")
            .field("message_count", &self.messages.len())
            .field("current_page", &self.current_page)
            .field("page_size", &self.page_size)
            .field("one_screen_scrolls", &self.one_screen_scrolls)
            .field("stop_when_reading_page", &self.stop_when_reading_page)
            .finish()
    }
}

impl SharedMailboxFolderPagePlace {
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    pub fn current_page_number(&self) -> usize {
        self.current_page.saturating_add(1)
    }

    pub fn one_screen_scroll_count(&self) -> usize {
        self.one_screen_scrolls
    }

    pub fn request_stop_when_reading_page(
        &mut self,
        page_number: usize,
        callback: impl Fn() -> Result<(), String> + 'static,
    ) {
        self.stop_when_reading_page = Some(page_number);
        self.stop_request_callback = Some(Box::new(callback));
    }
}

impl SharedConversationScrollablePlace for SharedMailboxFolderPagePlace {
    fn read_current_screen(&self) -> Result<Vec<SharedPlaceMessage>, String> {
        if self.stop_when_reading_page == Some(self.current_page_number()) {
            if let Some(callback) = &self.stop_request_callback {
                callback()?;
            }
        }
        let start = self.current_page.saturating_mul(self.page_size);
        let end = start
            .saturating_add(self.page_size)
            .min(self.messages.len());
        Ok(self.messages[start..end]
            .iter()
            .map(|message| {
                SharedPlaceMessage::new(
                    message.summary.id.clone(),
                    format!("{}\n{}", message.summary.subject, message.body),
                )
            })
            .collect())
    }

    fn scroll_one_screen(&mut self) -> Result<bool, String> {
        let next_start = (self.current_page + 1).saturating_mul(self.page_size);
        if next_start >= self.messages.len() {
            return Ok(false);
        }
        self.current_page += 1;
        self.one_screen_scrolls += 1;
        Ok(true)
    }
}

fn validate_part(value: &str, name: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(format!("{name} is invalid"));
    }
    Ok(())
}
