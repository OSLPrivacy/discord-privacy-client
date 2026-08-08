//! The iCloud Mail half of the shared mailbox reader (TASK 3071), in its own
//! module.
//!
//! Task 1272 ruled that iCloud may be driven, so this is filled in rather than
//! left empty. It lived in `service_connections.rs` and still reads from there
//! through a `pub use`, so gate 3071's own test and every existing
//! `service_connections::…` path are unchanged. It moved so the iCloud mail
//! deleter (TASK 3073) can drive the very mailbox this reader reads without
//! dragging in that file's mail-website plumbing.

use std::collections::BTreeSet;

use crate::shared_mail_reader_types::{
    legacy_mail_owner_call, owner_call_for_sender, readable_sender, who_wrote_it_for_sender,
    OpenedSharedMailMessage, SharedMailLabel, SharedMailMessageRecord, SharedMailMessageSummary,
    SharedMailboxReaderError, SharedMailboxSnapshot,
};

pub const ICLOUD_SERVICE_ID: &str = "icloud";

pub fn read_icloud_shared_mailbox_folders(
    mailbox: &SharedMailboxSnapshot,
) -> Result<Vec<SharedMailLabel>, SharedMailboxReaderError> {
    validate_icloud_mailbox(mailbox)?;
    Ok(mailbox.labels.clone())
}

pub fn read_icloud_shared_mailbox_messages(
    mailbox: &SharedMailboxSnapshot,
    folder_id: &str,
) -> Result<Vec<SharedMailMessageSummary>, SharedMailboxReaderError> {
    validate_icloud_mailbox(mailbox)?;
    ensure_icloud_folder_exists(mailbox, folder_id)?;
    mailbox
        .messages
        .iter()
        .filter(|message| message.label_id == folder_id)
        .map(|message| {
            let sender = readable_sender(message)?;
            let who_wrote_it = who_wrote_it_for_sender(&mailbox.signed_in_address, message)?;
            let called = legacy_mail_owner_call(who_wrote_it).to_owned();
            Ok(SharedMailMessageSummary {
                label_id: message.label_id.clone(),
                message_id: message.message_id.clone(),
                subject: message.subject.clone(),
                time: message.time,
                sender,
                who_wrote_it,
                called,
            })
        })
        .collect()
}

pub fn open_icloud_shared_mailbox_message(
    mailbox: &SharedMailboxSnapshot,
    folder_id: &str,
    message_id: &str,
) -> Result<OpenedSharedMailMessage, SharedMailboxReaderError> {
    validate_icloud_mailbox(mailbox)?;
    ensure_icloud_folder_exists(mailbox, folder_id)?;
    validate_icloud_reader_text(message_id, 180)?;

    let mut matches = mailbox
        .messages
        .iter()
        .filter(|message| message.label_id == folder_id && message.message_id == message_id);
    let Some(message) = matches.next() else {
        return Err(SharedMailboxReaderError::MessageNotFound);
    };
    if matches.next().is_some() {
        return Err(SharedMailboxReaderError::DuplicateMessage);
    }
    validate_icloud_message(message)?;
    Ok(OpenedSharedMailMessage {
        label_id: message.label_id.clone(),
        message_id: message.message_id.clone(),
        subject: message.subject.clone(),
        time: message.time,
        sender: readable_sender(message)?,
        body: message.body.clone(),
        who_wrote_it: who_wrote_it_for_sender(&mailbox.signed_in_address, message)?,
        called: owner_call_for_sender(&mailbox.signed_in_address, message)?,
    })
}

fn validate_icloud_mailbox(
    mailbox: &SharedMailboxSnapshot,
) -> Result<(), SharedMailboxReaderError> {
    validate_icloud_reader_text(&mailbox.signed_in_address, 254)?;
    let mut folder_ids = BTreeSet::new();
    for folder in &mailbox.labels {
        validate_icloud_reader_text(&folder.label_id, 128)?;
        validate_icloud_reader_text(&folder.name, 128)?;
        if !folder_ids.insert(folder.label_id.as_str()) {
            return Err(SharedMailboxReaderError::InvalidIcloudMailbox);
        }
    }
    for message in &mailbox.messages {
        validate_icloud_message(message)?;
        if !folder_ids.contains(message.label_id.as_str()) {
            return Err(SharedMailboxReaderError::UnknownFolder);
        }
    }
    Ok(())
}

fn validate_icloud_message(
    message: &SharedMailMessageRecord,
) -> Result<(), SharedMailboxReaderError> {
    validate_icloud_reader_text(&message.label_id, 128)?;
    validate_icloud_reader_text(&message.message_id, 180)?;
    validate_icloud_reader_text(&message.subject, 512)?;
    validate_icloud_reader_body(&message.body)?;
    if message.time <= 0 {
        return Err(SharedMailboxReaderError::InvalidIcloudMailbox);
    }
    readable_sender(message)?;
    Ok(())
}

fn ensure_icloud_folder_exists(
    mailbox: &SharedMailboxSnapshot,
    folder_id: &str,
) -> Result<(), SharedMailboxReaderError> {
    validate_icloud_reader_text(folder_id, 128)?;
    if mailbox
        .labels
        .iter()
        .any(|folder| folder.label_id == folder_id)
    {
        Ok(())
    } else {
        Err(SharedMailboxReaderError::UnknownFolder)
    }
}

fn validate_icloud_reader_text(
    value: &str,
    max_bytes: usize,
) -> Result<(), SharedMailboxReaderError> {
    if value.trim() == value
        && !value.is_empty()
        && value.len() <= max_bytes
        && !value.chars().any(|character| character.is_control())
    {
        Ok(())
    } else {
        Err(SharedMailboxReaderError::InvalidIcloudMailbox)
    }
}

fn validate_icloud_reader_body(value: &str) -> Result<(), SharedMailboxReaderError> {
    if value.len() <= 256 * 1024
        && value
            .chars()
            .all(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
    {
        Ok(())
    } else {
        Err(SharedMailboxReaderError::InvalidIcloudMailbox)
    }
}
